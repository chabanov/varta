use super::*;
const PROFILE: &str = "checked-f64-utf16-v40";
#[test]
fn protected_session_native_api_needs_each_grant_and_an_http_context_in_dev_and_standalone() {
    let source = include_str!("../../../../conformance/session-v40/cases/server-context.ts");
    let project = RegressionProject::new(source, serde_json::json!({}));
    project.language_profile(PROFILE);
    let output = project.command(&["dev"]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(combined(&output).contains("varta log: true true true"));
    project
        .set_capabilities(serde_json::json!({"session":{"read":true,"write":true,"clear":true}}));
    // A declared package grant never supplies an implicit session host ceiling.
    let output = project.command(&["dev"]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(combined(&output).contains("varta log: true true true"));
    fs::write(project.0.join("src/main.ts"),r#"export function main():void {
 let denied=0;
 try{varta.http.session.get();}catch(e){if(e instanceof DOMException&&e.name==='InvalidStateError')denied+=1;}
 try{varta.http.session.set('synthetic');}catch(e){if(e instanceof DOMException&&e.name==='InvalidStateError')denied+=1;}
 try{varta.http.session.clear();}catch(e){if(e instanceof DOMException&&e.name==='InvalidStateError')denied+=1;}
 console.log(denied);
}"#).unwrap();
    fs::write(project.0.join("host-policy.json"),serde_json::to_vec(&serde_json::json!({"schemaVersion":1,"abiVersion":36,"capabilities":[{"id":"session.read","enabled":true},{"id":"session.write","enabled":true},{"id":"session.clear","enabled":true}]})).unwrap()).unwrap();
    let output = project.command(&["dev", "--policy", "host-policy.json"]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(combined(&output).contains("varta log: 3"));
    let built = project.command(&["build", "--single"]);
    assert!(built.status.success(), "{}", combined(&built));
    fs::remove_dir_all(project.0.join("src")).unwrap();
    let output = Command::new(project.0.join("dist/app"))
        .current_dir(&project.0)
        .args(["--policy", "host-policy.json"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", combined(&output));
    assert!(combined(&output).contains("varta log: 3"));
}
#[test]
fn protected_session_native_client_fails_closed_and_old_profile_cannot_admit_it() {
    let project = RegressionProject::new(
        include_str!("../../../../conformance/session-v40/cases/native-channel.ts"),
        serde_json::json!({"net":{"session":{"urls":["https://api.test/**"],"methods":["GET"]}}}),
    );
    project.language_profile(PROFILE);
    let output = project.command(&["dev"]);
    assert!(output.status.success(), "{}", combined(&output));
    assert!(combined(&output).contains("native channel refused true"));
    project.language_profile("checked-f64-utf16-v39");
    let output = project.command(&["check"]);
    assert!(!output.status.success());
}
