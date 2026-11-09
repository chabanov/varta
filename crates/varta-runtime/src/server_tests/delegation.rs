use super::*;
use varta_common::{http, language::LanguageProfile, session::delegation as wire};
const ENV: &str = "VARTA_NATIVE_DELEGATION_ACCEPTANCE";
const TOKEN: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn raw(
    server: &Server,
    method: &str,
    path: &str,
    extra: &str,
    body: &str,
) -> (u16, String, String) {
    server.raw(format!("{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\n\r\n{body}",server.addr,body.len()).as_bytes())
}
fn cookie(h: &str) -> String {
    h.lines()
        .find_map(|l| l.strip_prefix("set-cookie: "))
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .into()
}
#[test]
fn native_delegation_http_self_lookup_issue_relay_revoke_and_restart() {
    // Do not mutate process-global environment in a parallel test runner.
    if std::env::var(ENV).as_deref() != Ok(TOKEN) {
        let output=std::process::Command::new(std::env::current_exe().unwrap()).args(["--exact","server_tests::delegation::native_delegation_http_self_lookup_issue_relay_revoke_and_restart","--nocapture","--test-threads=1"]).env(ENV,TOKEN).output().unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = reserved.local_addr().unwrap();
    let base = format!("http://{addr}");
    let source=r#"
    export function main():void{}
    async function control(route:string,body:string):Promise<string>{
      const response=await fetch('__BASE____PREFIX__'+route,{method:'POST',headers:{'Authorization':'Bearer __TOKEN__','Content-Type':'application/json'},body});
      if(response.status!==200){throw new Error('native control refused');}return await response.text();
    }
    async function handler(request:VartaHttpRequest):Promise<VartaHttpResponse>{
      if(request.target==='/set'){varta.http.session.set('synthetic-parent-secret');return {status:201,body:'created',headers:[]};}
      if(request.target==='/clear'){varta.http.session.clear();return {status:200,body:'cleared',headers:[]};}
      let proof='';for(const h of request.headers){if(h.name==='x-varta-request-proof'){proof=h.value;}}
      if(request.target==='/issue'){
        const verified=await control('/lookup',JSON.stringify({requestProof:proof}));
        const status=await control('/issue',JSON.stringify({requestProof:proof,requestId:'request-00000001',bindingId:'binding-00000001',context:'opaque-context',contextSha256:'__HASH__',pathPrefix:'/private/records',methods:['GET'],ttlSeconds:300}));
        return {status:200,body:status,headers:[]};
      }
      if(request.target==='/revoke'){const input=varta.json.parse<{id:string}>(request.body);return {status:200,body:await control('/revoke',JSON.stringify({requestProof:proof,delegationId:input.id})),headers:[]};}
      if(request.target==='/capture'){return {status:200,body:proof,headers:[]};}
      if(request.target==='/lookup'){return {status:200,body:await control('/lookup',JSON.stringify({requestProof:proof})),headers:[]};}
      if(request.target==='/private/records'){
        let denied=false;try{varta.http.session.set('forbidden');}catch(error){denied=true;}
        const verified=await control('/lookup',JSON.stringify({requestProof:proof}));
        return {status:denied?200:500,body:verified,headers:[]};
      }
      return {status:404,body:'missing',headers:[]};
    }
    varta.http.serve({hostname:'127.0.0.1',port:__PORT__},handler);
    "#.replace("__BASE__",&base).replace("__PREFIX__",wire::CONTROL_PREFIX).replace("__TOKEN__",TOKEN).replace("__HASH__",&format!("{:x}",sha2::Sha256::digest(b"opaque-context"))).replace("__PORT__",&addr.port().to_string());
    use sha2::Digest;
    let bytes = varta_compiler::compile_typescript_with_profile(
        &source,
        LanguageProfile::CheckedF64Utf16V40,
    )
    .unwrap();
    let mut opts = super::sessions::options();
    let policy = opts.http.as_mut().unwrap();
    policy.session.as_mut().unwrap().origin = base.clone();
    policy.limits = Some(http::HostLimits {
        workers: Some(1),
        ..Default::default()
    });
    policy.delegation = Some(wire::DelegationPolicy {
        control_token_env: ENV.into(),
        max_bindings: 4,
        max_request_proofs: 4,
        max_age_seconds: 300,
        path_prefixes: vec!["/private".into()],
        methods: vec!["GET".into()],
    });
    opts.ceiling
        .as_mut()
        .unwrap()
        .retain(|g| g.id != "net.listen");
    opts.ceiling.as_mut().unwrap().push(grant(addr.to_string()));
    let net = CapabilityGrant {
        id: "net.request".into(),
        value: CapabilityValue::Requests(varta_common::RequestScope {
            urls: vec![format!("{base}{}/**", wire::CONTROL_PREFIX)],
            methods: Some(vec!["POST".into()]),
            response_headers: None,
        }),
    };
    opts.requested.push(net.clone());
    opts.ceiling.as_mut().unwrap().push(net);
    drop(reserved);
    let server = Server::start_options(bytes.clone(), opts.clone());
    let control = format!("Authorization: Bearer {TOKEN}\r\n");
    let root = wire::CONTROL_PREFIX;
    assert_eq!(raw(&server, "GET", &format!("{root}/info"), "", "").0, 401);
    for invalid in [
        "Origin: http://evil.test\r\n",
        "Cookie: fake=1\r\n",
        "X-Varta-Request-Proof: forged\r\n",
        "X-Varta-Session: 1\r\n",
    ] {
        assert_eq!(
            raw(
                &server,
                "GET",
                &format!("{root}/info"),
                &format!("{control}{invalid}"),
                ""
            )
            .0,
            403
        );
    }
    let (s, h, b) = raw(&server, "GET", &format!("{root}/info"), &control, "");
    assert_eq!(s, 200, "{b}");
    assert!(h.contains("no-store"));
    assert!(!h.contains("access-control"));
    assert_eq!(
        raw(
            &server,
            "POST",
            &format!("{root}/lookup"),
            &control,
            r#"{"requestProof":"forged"}"#
        )
        .0,
        403
    );
    assert_eq!(
        raw(
            &server,
            "GET",
            "/lookup",
            "X-Varta-Request-Proof: forged\r\n",
            ""
        )
        .0,
        403
    );
    assert_eq!(
        server.raw(format!("POST {root}/lookup HTTP/1.1\r\nHost: {}\r\n{control}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",server.addr,wire::MAX_CONTROL_BYTES+1).as_bytes())
        .0,
        413
    );
    assert_eq!(
        raw(
            &server,
            "POST",
            &format!("{root}/lookup"),
            &control,
            r#"{"requestProof":"forged","mode":"delegated"}"#
        )
        .0,
        403
    );
    let (s, _, expired_proof) = raw(&server, "GET", "/capture", "", "");
    assert_eq!(s, 200);
    assert_eq!(expired_proof.len(), 64);
    assert_eq!(
        raw(
            &server,
            "POST",
            &format!("{root}/lookup"),
            &control,
            &serde_json::json!({"requestProof":expired_proof}).to_string()
        )
        .0,
        403
    );
    // Native proof uses the actual v2 request limit, not the v1/response cap.
    let many_headers: String = (0..95).map(|i| format!("X-Test-{i}: x\r\n")).collect();
    let (s, _, b) = raw(&server, "GET", "/lookup", &many_headers, "");
    assert_eq!(s, 200, "{b}; {:?}", server.diagnostics.lock().unwrap());
    let channel = "Origin: http://127.0.0.1:8788\r\nX-Varta-Session: 1\r\n";
    let (s, h, b) = raw(&server, "POST", "/set", channel, "");
    assert_eq!(s, 201, "{b}; {:?}", server.diagnostics.lock().unwrap());
    let token = cookie(&h);
    let auth = format!("{channel}Cookie: {token}\r\n");
    let (s, _, b) = raw(&server, "POST", "/issue", &auth, "");
    assert_eq!(s, 200, "{b}; {:?}", server.diagnostics.lock().unwrap());
    let status: serde_json::Value = serde_json::from_str(&b).unwrap();
    let id = status["id"].as_str().unwrap();
    assert!(!b.contains("secret"));
    let (s, _, repeat) = raw(&server, "POST", "/issue", &auth, "");
    assert_eq!(s, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&repeat).unwrap(),
        status
    );
    let relay = format!("{root}/relay/{id}/private/records");
    assert_eq!(raw(&server, "GET", &relay, "", "").0, 401);
    let (s, h, b) = raw(&server, "GET", &relay, &control, "");
    assert_eq!(s, 200, "{b}; {:?}", server.diagnostics.lock().unwrap());
    let verified: serde_json::Value = serde_json::from_str(&b).unwrap();
    assert_eq!(verified["mode"], "delegated");
    assert_eq!(verified["context"]["context"], "opaque-context");
    assert_eq!(verified["parentCredential"], "synthetic-parent-secret");
    assert!(!h.contains("set-cookie"));
    assert!(!h.contains("access-control"));
    // Neither the public ID nor a valid owner cookie can substitute for native relay authentication.
    assert_eq!(raw(&server, "GET", &relay, &auth, "").0, 403);
    assert_eq!(raw(&server, "POST", &relay, &control, "").0, 403);
    for suffix in ["/../outside", "/%2e%2e/outside", "X"] {
        assert_eq!(
            raw(&server, "GET", &format!("{relay}{suffix}"), &control, "").0,
            403
        );
    }
    let (s, _, b) = raw(
        &server,
        "POST",
        "/revoke",
        &auth,
        &serde_json::json!({"id":id}).to_string(),
    );
    assert_eq!(s, 200, "{b}");
    assert_eq!(raw(&server, "GET", &relay, &control, "").0, 403);
    let (s, _, b) = raw(&server, "POST", "/issue", &auth, "");
    assert_eq!(s, 200);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&b).unwrap()["state"],
        "revoked"
    );
    assert_eq!(raw(&server, "POST", "/clear", &auth, "").0, 200);
    assert_eq!(raw(&server, "GET", &relay, &control, "").0, 403);
    assert!(
        !server
            .diagnostics
            .lock()
            .unwrap()
            .join(" ")
            .contains("synthetic-parent-secret")
    );
    server.finish();
    let server = Server::start_options(bytes, opts);
    assert_eq!(raw(&server, "GET", &relay, &control, "").0, 403);
    server.finish();
}
