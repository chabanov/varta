//! Hostile portable/native HTTP v2 inputs must fail before startup or bind.
use super::*;
use varta_common::http;

fn unsigned(bytes: &[u8], cursor: &mut usize) -> usize {
    let mut value = 0;
    let mut shift = 0;
    loop {
        let byte = bytes[*cursor];
        *cursor += 1;
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
        shift += 7;
    }
}
fn encoded(mut value: usize) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7f) as u8;
        value >>= 7;
        out.push(byte | if value == 0 { 0 } else { 0x80 });
        if value == 0 {
            return out;
        }
    }
}
fn rewrite(bytes: &[u8], mut edit: impl FnMut(u8, &[u8]) -> Vec<Vec<u8>>) -> Vec<u8> {
    let mut result = bytes[..8].to_vec();
    let mut at = 8;
    while at < bytes.len() {
        let id = bytes[at];
        at += 1;
        let len = unsigned(bytes, &mut at);
        let end = at + len;
        for section in edit(id, &bytes[at..end]) {
            result.push(id);
            result.extend(encoded(section.len()));
            result.extend(section);
        }
        at = end;
    }
    result
}
fn metadata(section: &[u8]) -> Option<usize> {
    let mut at = 0;
    let len = unsigned(section, &mut at);
    (section.get(at..at + len) == Some(http::SECTION.as_bytes())).then_some(at + len)
}
fn exports(section: &[u8], action: usize) -> Vec<u8> {
    let mut at = 0;
    let count = unsigned(section, &mut at);
    let mut entries = Vec::new();
    let mut value = None;
    for _ in 0..count {
        let start = at;
        let len = unsigned(section, &mut at);
        let name = std::str::from_utf8(&section[at..at + len]).unwrap();
        at += len;
        let kind = section[at];
        at += 1;
        let index_start = at;
        let index = unsigned(section, &mut at);
        if name == http::VALUE {
            value = Some(index);
        }
        entries.push((name, kind, index_start - start, section[start..at].to_vec()));
    }
    let mut out = Vec::new();
    let mut retained = 0;
    for (name, kind, index_start, mut bytes) in entries {
        if action == 3 && name == http::VALUE {
            continue;
        }
        if action == 4 && name == http::STATE {
            assert_eq!(kind, 0);
            bytes.truncate(index_start);
            bytes.extend(encoded(value.unwrap()));
        }
        if action == 5 && name == http::STATE {
            let offset = encoded(name.len()).len();
            bytes[offset + name.len() - 1] = b'x';
        }
        out.extend(bytes);
        retained += 1;
    }
    let mut result = encoded(retained);
    result.extend(out);
    result
}

#[test]
fn damaged_http_v2_contracts_fail_before_main_or_bind_in_source_free_artifacts() {
    let occupied = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = occupied.local_addr().unwrap();
    let source=r#"
      async function handle(r:VartaHttpRequest):Promise<VartaHttpResponse>{
        await null;const text=varta.env.get("HTTP_VALUE");return {status:200,body:text,headers:[]};
      }
      export function main():void{console.log("HTTP_STARTUP_EXECUTED");varta.env.print("VARTA_HTTP_ARTIFACT_SECRET");}
      varta.http.serve({hostname:"127.0.0.1",port:__PORT__},handle);
    "#.replace("__PORT__",&endpoint.port().to_string());
    let project = RegressionProject::new(
        &source,
        serde_json::json!({"net":{"listen":[endpoint.to_string()]},"env":{"get":["HTTP_VALUE"],"print":["VARTA_HTTP_ARTIFACT_SECRET"]}}),
    );
    project.language_profile(V38_PROFILE);
    let built = project.command(&["build", "--single"]);
    assert!(built.status.success(), "{}", combined(&built));
    let original = varta_project::Artifact::read(&project.0.join("dist/app.wasm")).unwrap();
    fs::remove_dir_all(project.0.join("src")).unwrap();
    fs::remove_file(project.0.join("varta.config.json")).unwrap();
    for action in 0..6 {
        let mut changed = false;
        let wasm = rewrite(&original.wasm, |id, section| {
            if id == 0
                && let Some(start) = metadata(section)
            {
                if action == 2 {
                    changed = true;
                    return vec![section.to_vec(), section.to_vec()];
                }
                if action < 2 {
                    let mut contract: serde_json::Value =
                        serde_json::from_slice(&section[start..]).unwrap();
                    if action == 0 {
                        contract["version"] = serde_json::json!(1);
                        contract["asynchronous"] = serde_json::json!(false);
                    } else {
                        let effects = contract["requiredEffects"].as_array_mut().unwrap();
                        let before = effects.len();
                        effects.retain(|effect| effect != "env.get");
                        assert_eq!(effects.len() + 1, before);
                    }
                    let mut data = section[..start].to_vec();
                    data.extend(serde_json::to_vec(&contract).unwrap());
                    changed = true;
                    return vec![data];
                }
            }
            if id == 7 && action >= 3 {
                changed = true;
                return vec![exports(section, action)];
            }
            vec![section.to_vec()]
        });
        assert!(changed && wasm != original.wasm);
        let artifact = varta_project::Artifact {
            wasm,
            manifest: original.manifest.clone(),
        };
        let portable = project.0.join(format!("damaged-{action}.wasm"));
        let native = project.0.join(format!("damaged-{action}"));
        artifact.write_wasm(&portable).unwrap();
        artifact.write_executable(&native, &runtime_path()).unwrap();
        let commands = [
            (
                varta(),
                vec!["validate".into(), portable.clone().into_os_string()],
            ),
            (
                varta(),
                vec!["inspect".into(), native.clone().into_os_string()],
            ),
            (
                varta(),
                vec!["run".into(), portable.clone().into_os_string()],
            ),
            (
                Command::new(runtime_path()),
                vec![
                    varta_common::launcher::VALIDATE_ARGUMENT.into(),
                    portable.clone().into_os_string(),
                ],
            ),
            (
                Command::new(runtime_path()),
                vec!["inspect".into(), portable.clone().into_os_string()],
            ),
            (
                Command::new(runtime_path()),
                vec!["run".into(), portable.into_os_string()],
            ),
            (Command::new(&native), vec!["--inspect".into()]),
            (Command::new(&native), Vec::new()),
        ];
        for (mut command, args) in commands {
            let output = command
                .current_dir(&project.0)
                .args(args)
                .env("VARTA_HTTP_ARTIFACT_SECRET", "DO_NOT_READ_HTTP_SECRET")
                .output()
                .unwrap();
            let text = combined(&output);
            assert!(!output.status.success(), "variant {action}: {text}");
            assert!(
                !text.contains("HTTP_STARTUP_EXECUTED")
                    && !text.contains("DO_NOT_READ_HTTP_SECRET"),
                "variant {action}: {text}"
            );
            assert!(
                !text.contains("varta listening:") && !text.contains("Address already in use"),
                "variant {action}: {text}"
            );
            assert!(
                text.contains("HTTP") || text.contains("ABI") || text.contains("effect mismatch"),
                "variant {action}: {text}"
            );
        }
    }
}
