//! Build fixed real-browser acceptance fixtures through the public project API.
use base64::Engine;
use std::path::PathBuf;
use varta_common::{Manifest, budget::BudgetRequest, language::LanguageProfile};
use varta_project::{
    Artifact,
    browser::{BrowserArtifact, BrowserResource},
};

struct FixedPrompt(varta_common::permission::PromptAnswer);
impl varta_runtime::PermissionPrompter for FixedPrompt {
    fn prompt(
        &mut self,
        _: &varta_common::permission::PermissionPrompt,
    ) -> varta_common::permission::PromptAnswer {
        self.0
    }
}

/// The resource section of `view-font-packaged`: regular, bold and italic
/// faces of one family and a monospace family, validated and ordered as the
/// native shell loads them.
fn packaged_faces() -> Result<Vec<BrowserResource>, Box<dyn std::error::Error>> {
    use varta_artifact::{FontFace, FontStyle, GenericFamily, NORMAL_FONT_WEIGHT, Resources};
    const SANS: &str = "Varta Fixture 400";
    const MONO: &str = "Varta Fixture 900";
    const BOLD_WEIGHT: u16 = 700;
    let face = |family: &str, weight, style, generics: &[GenericFamily], data: &[u8]| FontFace {
        family: family.into(),
        weight,
        style,
        generics: generics.to_vec(),
        data: data.to_vec(),
    };
    let section = Resources::new(vec![
        face(
            SANS,
            NORMAL_FONT_WEIGHT,
            FontStyle::Normal,
            &[],
            include_bytes!("../../varta-view/tests/fixtures/advance-400.ttf"),
        ),
        face(
            SANS,
            BOLD_WEIGHT,
            FontStyle::Normal,
            &[],
            include_bytes!("../../varta-view/tests/fixtures/bold-900.ttf"),
        ),
        face(
            SANS,
            NORMAL_FONT_WEIGHT,
            FontStyle::Italic,
            &[],
            include_bytes!("../../varta-view/tests/fixtures/italic-900.ttf"),
        ),
        face(
            MONO,
            NORMAL_FONT_WEIGHT,
            FontStyle::Normal,
            &[GenericFamily::Monospace],
            include_bytes!("../../varta-view/tests/fixtures/advance-900.ttf"),
        ),
    ])?;
    Ok(section
        .fonts()
        .iter()
        .map(BrowserResource::packaged)
        .collect::<Result<_, _>>()?)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .ok_or("usage: browser_bundle_probe NEW_DIRECTORY")?;
    std::fs::create_dir(&directory)?;
    let network_base = std::env::args().nth(2);
    let mut cases: Vec<(&str, String)> = [
        (
            "ok",
            r#"export function main():void {console.log(123);console.log("Привіт 😀");console.log("\uFEFFstart\uD800end");console.log(JSON.stringify([1,2,3]));}"#,
        ),
        (
            "throw",
            r#"export function main():void { console.log("before throw"); throw new Error("boom"); }"#,
        ),
        (
            "loop",
            r#"export function main():void { console.log("before loop"); while(true){} }"#,
        ),
        ("fuel", r#"export function main():void {console.log(1);}"#),
        (
            "calls",
            r#"export function main():void {console.log(1);console.log(2);}"#,
        ),
        (
            "output",
            r#"export function main():void {console.log("a".repeat(300));}"#,
        ),
        (
            "chunks",
            r#"export function main():void {console.log("\uFEFF"+"a".repeat(32766)+"😀\uD800"+"b".repeat(32769));}"#,
        ),
        (
            "async",
            r#"async function task():Promise<void>{console.log("entered");try{const n=await Promise.resolve(7);console.log(n);await 8;}finally{console.log("finally");}} export function main():void{task();console.log("sync");}"#,
        ),
        (
            "fifo",
            r#"export function main():void{const p=Promise.resolve(1);p.then(()=>{console.log("a");Promise.resolve(2).then(()=>{console.log("c");});});p.then(()=>{console.log("b");});console.log("sync");}"#,
        ),
        (
            "handled",
            r#"export function main():void{const p=Promise.reject<number>(new Error("late"));Promise.resolve(0).then(()=>1).then(()=>{p.catch(()=>{console.log("handled");return 0;});});console.log("sync");}"#,
        ),
        (
            "unhandled",
            r#"export function main():void{Promise.reject<number>(new TypeError("boom"));console.log("before rejection");}"#,
        ),
        (
            "reject-string",
            r#"export function main():void{Promise.reject<number>("\uFEFFhello\uD800".repeat(300));}"#,
        ),
        (
            "reject-number",
            r#"export function main():void{Promise.reject<number>(-0.0);Promise.reject<number>(0.0000001);}"#,
        ),
        (
            "reject-bigint",
            r#"export function main():void{Promise.reject<number>(-12345678901234567890n);}"#,
        ),
        (
            "reject-class",
            r#"class Failure extends Error {constructor(){super("custom");this.name="Failure";}}export function main():void{Promise.reject<number>(new Failure());}"#,
        ),
        (
            "pending",
            r#"export function main():void{const p=new Promise<number>((resolve,reject)=>{});p.then(()=>{console.log("never");});console.log("pending");}"#,
        ),
        (
            "async-calls",
            r#"export function main():void{Promise.resolve(1).then(()=>{console.log(1);});}"#,
        ),
        (
            "async-budget",
            r#"function again():void{Promise.resolve(1).then(()=>{again();});}export function main():void{again();}"#,
        ),
        (
            "reject-long-error",
            r#"export function main():void{Promise.reject<number>(new Error("x".repeat(600000)));}"#,
        ),
        (
            "reject-long-class",
            r#"class Failure extends Error {constructor(){super("x".repeat(600000));this.name="Failure";}}export function main():void{Promise.reject<number>(new Failure());}"#,
        ),
        (
            "reject-exponent",
            r#"export function main():void{Promise.reject<number>(0.0000001);}"#,
        ),
        (
            "reject-retention",
            r#"function reject(n:number):void{const p=Promise.reject<number>(n);if(n<16){Promise.resolve(0).then(()=>{p.catch(()=>0);});}}export function main():void{for(let i=0;i<20;i++)reject(i);}"#,
        ),
        (
            "timer-order",
            r#"export function main():void{setTimeout(()=>{console.log("first");Promise.resolve(1).then(()=>{console.log("microtask");});},2);setTimeout(()=>{console.log("second");},2);console.log("sync");}"#,
        ),
        (
            "timer-cancel",
            r#"export function main():void{const id=setTimeout(()=>{console.log("never");},1);clearTimeout(id);setTimeout(()=>{console.log("alive");},2);}"#,
        ),
        (
            "timer-interval",
            r#"export function main():void{let n=0;const id=setInterval(()=>{console.log(++n);if(n===3)clearInterval(id);},1);}"#,
        ),
        (
            "timer-denied",
            r#"export function main():void{try{setTimeout(()=>{console.log("never");},1);}catch(e){console.log("denied");}console.log("after");}"#,
        ),
        (
            "timer-optional",
            r#"export function main():void{try{setTimeout(()=>{console.log("never");},1);}catch(e){console.log("optional denied");}}"#,
        ),
        (
            "timer-throw",
            r#"export function main():void{setTimeout(()=>{throw new RangeError("timer failed");},1);}"#,
        ),
        (
            "timer-deadline",
            r#"export function main():void{setTimeout(()=>{console.log("never");},60000);}"#,
        ),
        (
            "timer-calls",
            r#"export function main():void{setTimeout(()=>{console.log("renewed");},1);}"#,
        ),
        (
            "timer-unref",
            r#"export function main():void{const signal=AbortSignal.timeout(60000);console.log(signal.aborted);}"#,
        ),
        (
            "timer-normalize",
            r#"export function main():void{setTimeout(()=>{console.log("two");},2.9);setTimeout(()=>{console.log("one");},0.5);}"#,
        ),
        (
            "timer-ceiling",
            r#"export function main():void{try{setTimeout(()=>{console.log("allowed");},1);}catch(e){console.log("denied");}}"#,
        ),
        (
            "timer-v36",
            r#"export function main():void{setTimeout(()=>{console.log("legacy timer");},1);}"#,
        ),
        (
            "timer-v37",
            r#"export function main():void{setTimeout(()=>{console.log("legacy timer");},1);}"#,
        ),
        (
            "timer-unref-alive",
            r#"export function main():void{const signal=AbortSignal.timeout(1);setTimeout(()=>{console.log(signal.aborted);},3);}"#,
        ),
        (
            "timer-long-throw",
            r#"class Failure extends Error{constructor(){super("x".repeat(300));this.name="Failure";}}export function main():void{setTimeout(()=>{throw new Failure();},1);}"#,
        ),
        ("view-live",r#"export function main():void{const root=View.node();View.createElement(root,1);const button=View.node();View.createElement(button,20);View.setAttribute(button,1,"counter");const label=View.node();View.createText(label,"0");View.insert(button,label,0);View.insert(root,button,0);let count=0;View.on(button,1,(event:ViewEvent):void=>{count++;View.setText(label,String(count));console.log(count);},0,0,0,0);const box=View.node();View.createElement(box,24);View.setAttribute(box,1,"entry");View.insert(root,box,0);View.on(box,2,(event:ViewEvent):void=>{View.setText(label,event.value);console.log(event.value);},0,0,0,0);View.mount(root);console.log("main");}"#),
        ("view-invalid",r#"export function main():void{const root=View.node();View.createElement(root,1);View.setAttribute(root,1,"duplicate");const other=View.node();View.createElement(other,1);View.setAttribute(other,1,"duplicate");View.insert(root,other,0);View.mount(root);console.log("must not run");}"#),
        ("view-second",r#"export function main():void{const root=View.node();View.createElement(root,1);View.mount(root);View.setTitle("second");View.commit();console.log("must not run");}"#),
        ("view-image",r#"export function main():void{console.log("before image");const root=View.node();View.createElement(root,1);const image=View.node();View.createElement(image,30);View.setAttribute(image,13,"icons/red.png");View.insert(root,image,0);View.mount(root);console.log("image ready");}"#),
        ("view-image-broken",r#"export function main():void{console.log("before image");const root=View.node();View.createElement(root,1);const image=View.node();View.createElement(image,30);View.setAttribute(image,13,"icons/red.png");View.insert(root,image,0);View.mount(root);console.log("image ready");}"#),
        ("view-image-missing",r#"export function main():void{const root=View.node();View.createElement(root,30);View.setAttribute(root,13,"icons/missing.png");View.mount(root);console.log("must not run");}"#),
        ("view-loop",r#"export function main():void{const root=View.node();View.createElement(root,1);const button=View.node();View.createElement(button,20);View.setAttribute(button,1,"counter");const label=View.node();View.createText(label,"Run");View.insert(button,label,0);View.insert(root,button,0);View.on(button,1,(event:ViewEvent):void=>{console.log("entered callback");while(true){}},0,0,0,0);View.mount(root);}"#),
        ("view-permissions",r#"export function main():void{const root=View.node();View.createElement(root,1);const button=View.node();View.createElement(button,20);View.setAttribute(button,1,"copy");const label=View.node();View.createText(label,"Copy");View.insert(button,label,0);View.insert(root,button,0);View.on(button,1,(event:ViewEvent):void=>{View.copyText("test clipboard",(status:number):void=>{console.log("copy "+status);});},0,0,0,0);const link=View.node();View.createElement(link,21);View.setAttribute(link,1,"external");View.setAttribute(link,12,"https://not-authorized.invalid/path");const text=View.node();View.createText(text,"External");View.insert(link,text,0);View.insert(root,link,0);View.onLinkResult((event:ViewEvent):void=>{console.log("link "+event.status);});View.mount(root);}"#),
        ("view-async",r#"export function main():void{const root=View.node();View.createElement(root,1);const button=View.node();View.createElement(button,20);View.setAttribute(button,1,"counter");const label=View.node();View.createText(label,"0");View.insert(button,label,0);View.insert(root,button,0);View.on(button,1,(event:ViewEvent):void=>{View.setText(label,"clicked");console.log("click");Promise.resolve(1).then(()=>{console.log("micro");});setTimeout(()=>{View.setText(label,"timer");View.commit();console.log("timer");},10);},0,0,0,0);View.mount(root);}"#),
    ].into_iter().map(|(name, source)| (name, source.to_owned())).collect();
    let font_source = r#"export function main():void{console.log("before font");View.stylesheet(1,':root { --face: Brand; --alias: var(--face); --paint: red; } .via { font-family: var(--alias, serif); font-size: 20px; color: var(--paint); } @media (min-width: 1px) { .quoted { font-family: "Brand"; font-size: 20px; } }');const root=View.node();View.createElement(root,1);const direct=View.node();View.createElement(direct,1);View.setAttribute(direct,1,"direct");View.setStyle(direct,53,"Brand");View.setStyle(direct,54,"20px");const a=View.node();View.createText(a,"AAAA");View.insert(direct,a,0);View.insert(root,direct,0);const via=View.node();View.createElement(via,1);View.setAttribute(via,1,"via");View.setClass(via,"via");const b=View.node();View.createText(b,"AAAA");View.insert(via,b,0);View.insert(root,via,0);const quoted=View.node();View.createElement(quoted,1);View.setAttribute(quoted,1,"quoted");View.setClass(quoted,"quoted");const c=View.node();View.createText(c,"AAAA");View.insert(quoted,c,0);View.insert(root,quoted,0);View.mount(root);console.log("font ready");}"#;
    for name in [
        "view-font-ttf",
        "view-font-otf",
        "view-font-woff",
        "view-font-woff2",
        "view-font-wide",
        "view-font-broken",
    ] {
        cases.push((name, font_source.into()));
    }
    // Packaged faces (configuration `fonts`): the default family without a
    // font-family, real bold and italic faces, the `monospace` alias reached
    // by a guest-set family and by the user-agent family of `code` and
    // `pre`, with an absolute, the medium (keyword) and an inherited
    // relative size, form controls (div 1, span 2, strong 15, code 17,
    // pre 18, button 20, textarea 25) and a bold run in a family without a
    // bold face.
    cases.push(("view-font-packaged",r#"function item(parent:number,tag:number,id:string,size:string,property:number,value:string,text:string):number{const e=View.node();View.createElement(e,tag);View.setAttribute(e,1,id);if(size!==""){View.setStyle(e,54,size);}if(property!==0){View.setStyle(e,property,value);}if(text!==""){const t=View.node();View.createText(t,text);View.insert(e,t,0);}View.insert(parent,e,0);return e;}export function main():void{const root=View.node();View.createElement(root,1);item(root,1,"regular","20px",0,"","AAAA");item(root,1,"bold","20px",55,"700","AAAA");item(root,1,"italic","20px",56,"italic","AAAA");item(root,1,"generic","20px",53,"monospace","AAAA");item(root,17,"ua-code","20px",0,"","AAAA");item(root,18,"ua-pre","20px",0,"","AAAA");item(root,17,"ua-code-medium","",0,"","AAAA");item(root,1,"generic-medium","",53,"monospace","AAAA");const pre=item(root,18,"pre-outer","",0,"","");item(pre,2,"pre-inherited","150%",0,"","AAAA");item(root,20,"control","20px",0,"","AAAA");item(root,25,"field","20px",0,"","");item(root,15,"synthesis","20px",53,"monospace","AAAA");View.mount(root);console.log("packaged fonts ready");}"#.into()));
    cases.push(("view-font-as-image",r#"export function main():void{const root=View.node();View.createElement(root,30);View.setAttribute(root,13,"Brand");View.mount(root);console.log("must not run");}"#.into()));
    cases.push(("permission-query",r#"export async function main():Promise<void>{console.log((await varta.permissions.query({name:"time.timer"})).state);console.log((await varta.permissions.request({name:"time.timer"})).state);console.log((await varta.permissions.query({name:"net.request",resource:"https://api.test/public/item"})).state);console.log((await varta.permissions.query({name:"net.request",resource:"https://api.test/public/item?"})).state);try{await varta.permissions.query({name:"time.fake"});}catch(e){console.log((e as Error).name);}try{await varta.permissions.request({name:"time.timer",resource:"x"});}catch(e){console.log((e as Error).name);}}"#.into()));
    let permission_source = r#"export async function main():Promise<void>{console.log("before permission");console.log((await varta.permissions.query({name:"time.timer"})).state);console.log((await varta.permissions.request({name:"time.timer"})).state);try{setTimeout(()=>{console.log("timer");},1);}catch(e){console.log((e as Error).name);}}"#;
    for name in [
        "permission-optional",
        "permission-optional-allow",
        "permission-optional-deny",
        "permission-optional-other",
        "permission-optional-empty",
        "permission-required-deny",
        "permission-required-deny-empty",
    ] {
        cases.push((name, permission_source.into()));
    }
    let invalid_text = r#"export async function main():Promise<void>{console.log("before invalid");try{await varta.permissions.query({name:"time.timer",resource:RESOURCE});console.log("must not run");}catch(e){console.log("must not catch fatal input");}}"#;
    for (name, resource) in [
        ("permission-bad-nul", r#""\0""#),
        ("permission-bad-surrogate", r#""\uD800""#),
        ("permission-bad-utf8", r#""\u0800".repeat(6000)"#),
    ] {
        cases.push((name, invalid_text.replace("RESOURCE", resource)));
    }
    cases.push(("permission-platform",r#"export async function main():Promise<void>{const root=View.node();View.createElement(root,1);const button=View.node();View.createElement(button,20);View.setAttribute(button,1,"permission");const label=View.node();View.createText(label,"Query");View.insert(button,label,0);View.insert(root,button,0);View.on(button,1,(event:ViewEvent):void=>{varta.permissions.query({name:"clipboard.write"}).then(status=>{console.log(status.state);});},0,0,0,0);View.mount(root);const q=await varta.permissions.query({name:"clipboard.write"});const r=await varta.permissions.request({name:"clipboard.write"});console.log(q.state+" "+r.state);}"#.into()));
    for name in ["consent-optional", "consent-required", "consent-parallel"] {
        let extra = if name == "consent-parallel" {
            r#"varta.permissions.request({name:"clipboard.write"}).then(status=>{console.log("parallel "+status.state);});"#
        } else {
            ""
        };
        cases.push((name,format!(r#"export async function main():Promise<void>{{const root=View.node();View.createElement(root,1);const button=View.node();View.createElement(button,20);View.setAttribute(button,1,"permission");const label=View.node();View.createText(label,"Request");View.insert(button,label,0);View.insert(root,button,0);View.on(button,1,(event:ViewEvent):void=>{{varta.permissions.request({{name:"clipboard.write"}}).then(status=>{{console.log("answer "+status.state);}});{extra}}},0,0,0,0);View.mount(root);const q=await varta.permissions.query({{name:"clipboard.write"}});const r=await varta.permissions.request({{name:"clipboard.write"}});console.log("initial "+q.state+" "+r.state);}}"#)));
    }
    cases.push(("events-live",r#"export function main():void{console.log("init");const id:number=varta.events.listen("ui",(record:string):void=>{console.log(record);if(record==="stop")varta.events.close(id);});try{varta.events.listen("ui",(record:string):void=>{});}catch(error){console.log((error as Error).name);}}"#.into()));
    cases.push(("events-reopen",r#"export function main():void{const id:number=varta.events.listen("ui",(record:string):void=>{console.log("old "+record);varta.events.close(id);const newer:number=varta.events.listen("ui",(next:string):void=>{console.log("new "+next);if(next==="stop")varta.events.close(newer);});});}"#.into()));
    cases.push(("events-throw",r#"export function main():void{varta.events.listen("ui",(record:string):void=>{console.log(record);throw new Error("listener failed");});}"#.into()));
    cases.push(("events-permission",r#"export function main():void{const id:number=varta.events.listen("ui",(record:string):void=>{varta.permissions.request({name:"clipboard.write"}).then(status=>{console.log(status.state);varta.events.close(id);});});}"#.into()));
    for name in [
        "view-link-wide",
        "view-link-explicit",
        "view-link-hostwide",
        "view-link-hostexplicit",
        "view-link-hostdeny",
    ] {
        cases.push((name,r#"export function main():void{const root=View.node();View.createElement(root,1);const link=View.node();View.createElement(link,21);View.setAttribute(link,1,"external");View.setAttribute(link,12,"https://destination.invalid/path?x=1#part");const text=View.node();View.createText(text,"Open destination");View.insert(link,text,0);View.insert(root,link,0);View.on(link,1,(event:ViewEvent):void=>{console.log("click");},0,0,0,0);View.onLinkResult((event:ViewEvent):void=>{console.log("link "+event.status);});View.mount(root);}"#.into()));
    }
    if let Some(base) = &network_base {
        cases.extend([
            ("net-ceiling", format!(r#"export async function main():Promise<void>{{try{{const r=await fetch("{base}/_net/ok");console.log(await r.text());}}catch(e){{console.log((e as Error).name);}}}}"#)),
            ("net-active-abort", format!(r#"export async function main():Promise<void>{{const c=new AbortController();setTimeout(()=>{{c.abort();}},5);try{{await fetch("{base}/_net/slow",{{signal:c.signal}});}}catch(e){{console.log((e as Error).name);}}}}"#)),
            ("net-custom", format!(r#"export async function main():Promise<void>{{const r=await fetch("{base}/_net/echo",{{method:"POST",headers:{{"x-request":"yes"}},body:"custom"}});console.log(await r.text());}}"#)),
            ("net-redirect", format!(r#"export async function main():Promise<void>{{try{{await fetch("{base}/_net/redirect");}}catch(e){{console.log((e as Error).message);}}}}"#)),
            ("net-get", format!(r#"export async function main():Promise<void>{{const r=await fetch("{base}/_net/ok");console.log(r.status);console.log(r.headers.get("x-id"));console.log(await r.text());}}"#)),
            ("net-post", format!(r#"export async function main():Promise<void>{{const r=await fetch("{base}/_net/echo",{{method:"POST",body:"Привіт 😀"}});console.log(await r.text());}}"#)),
            ("net-bytes", format!(r#"export async function main():Promise<void>{{const r=await fetch("{base}/_net/echo",{{method:"POST",body:new Uint8Array([0,255,65])}});const bytes=new Uint8Array(await r.arrayBuffer());console.log(bytes.length);console.log(bytes[0]);console.log(bytes[1]);console.log(bytes[2]);}}"#)),
            ("net-denied", format!(r#"export async function main():Promise<void>{{try{{await fetch("{base}/_net/denied");}}catch(e){{console.log((e as Error).name);}}}}"#)),
            ("net-abort", format!(r#"export async function main():Promise<void>{{const c=new AbortController();c.abort();try{{await fetch("{base}/_net/ok",{{signal:c.signal}});}}catch(e){{console.log((e as Error).name);}}}}"#)),
        ]);
    }
    for (name, source) in cases {
        let profile = match name {
            "timer-v36" => LanguageProfile::CheckedF64Utf16V36,
            "timer-v37" => LanguageProfile::CheckedF64Utf16V37,
            _ => LanguageProfile::CheckedF64Utf16V38,
        };
        let wasm = varta_compiler::compile_typescript_with_profile(&source, profile)?;
        let grants = if name.starts_with("timer-") && name != "timer-denied"
            || matches!(name, "net-active-abort" | "view-async")
        {
            vec![varta_common::CapabilityGrant {
                id: "time.timer".into(),
                value: varta_common::CapabilityValue::Boolean(true),
            }]
        } else {
            vec![]
        };
        let mut grants = grants;
        if name.starts_with("net-") && name != "net-denied" {
            grants.push(varta_common::CapabilityGrant {
                id: "net.request".into(),
                value: varta_common::CapabilityValue::Requests(varta_common::RequestScope {
                    urls: vec![format!("{}/_net/**", network_base.as_ref().unwrap())],
                    methods: None,
                    response_headers: Some(vec!["content-type".into(), "x-id".into()]),
                }),
            });
        }
        if name == "view-permissions" {
            grants.push(varta_common::CapabilityGrant {
                id: "clipboard.write".into(),
                value: varta_common::CapabilityValue::Boolean(true),
            });
            grants.push(varta_common::CapabilityGrant {
                id: "ui.open_url".into(),
                value: varta_common::CapabilityValue::UrlGlobs(vec![
                    "https://not-authorized.invalid/**".into(),
                ]),
            });
        }
        if name.starts_with("view-link-") {
            grants.push(varta_common::CapabilityGrant {
                id: "ui.open_url".into(),
                value: varta_common::CapabilityValue::UrlGlobs(vec![if matches!(
                    name,
                    "view-link-explicit" | "view-link-hostwide"
                ) {
                    "https://destination.invalid/**".into()
                } else {
                    varta_common::view::ANY_HTTPS_PATTERN.into()
                }]),
            });
        }
        if name.starts_with("permission-") && name != "permission-platform" {
            grants.push(varta_common::CapabilityGrant {
                id: "time.timer".into(),
                value: varta_common::CapabilityValue::Boolean(true),
            });
            if name == "permission-query" {
                grants.push(varta_common::CapabilityGrant {
                    id: "net.request".into(),
                    value: varta_common::CapabilityValue::Requests(varta_common::RequestScope {
                        urls: vec!["https://api.test/public/item".into()],
                        methods: Some(vec!["GET".into()]),
                        response_headers: None,
                    }),
                });
            }
        }
        if name == "permission-platform"
            || name.starts_with("consent-")
            || name == "events-permission"
        {
            grants.push(varta_common::CapabilityGrant {
                id: "clipboard.write".into(),
                value: varta_common::CapabilityValue::Boolean(true),
            });
        }
        let mut manifest = Manifest::new(&grants);
        if name == "timer-optional" || name.starts_with("permission-optional") {
            manifest.capabilities[0].required = Some(false);
        }
        if name.starts_with("consent-") {
            manifest.capabilities[0].required = Some(name == "consent-required");
            manifest.capabilities[0].purpose = Some("Quoted <package> purpose".into());
        }
        if name == "events-permission" {
            manifest.capabilities[0].required = Some(false);
        }
        manifest.abi_version = profile.host_abi();
        if matches!(name, "loop" | "view-loop") {
            manifest.budgets = Some(BudgetRequest {
                fuel_per_turn: Some(varta_common::budget::MAX_FUEL_PER_TURN),
                deadline_per_turn_ms: Some(if name == "view-loop" { 500 } else { 250 }),
                ..Default::default()
            });
        }
        let requested = match name {
            "fuel" => Some(BudgetRequest {
                fuel_per_turn: Some(1),
                ..Default::default()
            }),
            "calls" | "async-calls" | "timer-calls" => Some(BudgetRequest {
                host_calls_per_turn: Some(1),
                ..Default::default()
            }),
            "output" => Some(BudgetRequest {
                output_bytes: Some(16),
                ..Default::default()
            }),
            _ => None,
        };
        if let Some(budgets) = requested {
            manifest.budgets = Some(budgets);
        }
        let artifact = Artifact { wasm, manifest };
        if name.starts_with("events-") {
            let runtime = varta_runtime::VartaRuntime::new();
            let mut app = runtime.start_application(
                &artifact.wasm,
                varta_runtime::ApplicationOptions {
                    run: varta_runtime::RunOptions {
                        requested: artifact.manifest.grants()?,
                        consents: artifact.manifest.consents(),
                        ..Default::default()
                    },
                    prompter: if name == "events-permission" {
                        Some(Box::new(FixedPrompt(
                            varta_common::permission::PromptAnswer::Allow,
                        )))
                    } else {
                        None
                    },
                    ..Default::default()
                },
            )?;
            let records = match name {
                "events-live" => vec![("a", false), ("old", true), ("new", true), ("stop", false)],
                "events-reopen" => vec![("reopen", false), ("stale", false)],
                "events-permission" => vec![("gesture", false)],
                _ => vec![("boom", false)],
            };
            let mut deliveries = Vec::new();
            for (record, coalesce) in records {
                deliveries.push(format!(
                    "{:?}",
                    app.deliver(varta_runtime::HostEvent {
                        source: "ui".into(),
                        record: record.into(),
                        gesture: name == "events-permission",
                        coalesce
                    })?
                ));
            }
            app.run();
            if name == "events-reopen" {
                app.deliver(varta_runtime::HostEvent {
                    source: "ui".into(),
                    record: "stop".into(),
                    gesture: false,
                    coalesce: false,
                })?;
                app.run();
            }
            let budget = app.budget_report();
            let usage = serde_json::json!({"fuelRemaining":budget.budgets.fuel_per_turn.saturating_sub(budget.turns.last().map_or(0,|turn|turn.fuel)),"fuelUsed":budget.total_fuel,"turns":budget.total_turns,"hostCalls":budget.turns.iter().map(|turn|u64::from(turn.host_calls)).sum::<u64>(),"microtasks":budget.turns.iter().map(|turn|turn.microtasks).sum::<u64>(),"outputBytes":budget.output_bytes});
            std::fs::write(
                directory.join(format!("{name}.events.golden.json")),
                serde_json::to_vec(
                    &serde_json::json!({"logs":app.take_output(),"usage":usage,"deliveries":deliveries,"failed":app.failure().is_some()}),
                )?,
            )?;
        }
        if name != "loop" {
            let ceiling = if matches!(
                name,
                "timer-ceiling"
                    | "net-ceiling"
                    | "permission-optional-empty"
                    | "permission-required-deny-empty"
            ) {
                let mut empty = Manifest::new(&[]);
                empty.abi_version = profile.host_abi();
                Some(varta_project::HostPolicy::from_manifest(empty)?)
            } else {
                None
            };
            let decisions = if name.starts_with("permission-optional-")
                || name.starts_with("permission-required-")
            {
                let identity = if name.ends_with("-other") {
                    format!("sha256:{}", "b".repeat(64))
                } else {
                    varta_runtime::artifact_identity(&artifact.wasm)
                };
                let decision = if name.contains("-deny") {
                    "deny"
                } else {
                    "allow"
                };
                let bytes = serde_json::to_vec(
                    &serde_json::json!({"applications":[{"artifact":identity,"decisions":[{"id":"time.timer","decision":decision}]}]}),
                )?;
                std::fs::write(directory.join(format!("{name}.decisions.json")), &bytes)?;
                Some(varta_common::permission::DecisionFile::parse(&bytes)?)
            } else {
                None
            };
            let report = varta_project::run_report(
                &artifact,
                ceiling.as_ref(),
                varta_project::RunSettings {
                    decisions,
                    ..Default::default()
                },
            )?;
            let usage=report.budget.as_ref().map(|budget|serde_json::json!({
                "fuelRemaining":budget.budgets.fuel_per_turn.saturating_sub(budget.turns.last().map_or(budget.total_fuel, |turn| turn.fuel)),
                "fuelUsed":budget.total_fuel,"turns":budget.total_turns,
                "hostCalls":budget.turns.iter().map(|turn|u64::from(turn.host_calls)).sum::<u64>(),
                "microtasks":budget.turns.iter().map(|turn|turn.microtasks).sum::<u64>(),"outputBytes":budget.output_bytes,
            }));
            let observation = serde_json::json!({"status":if report.failure.is_none(){"ok"}else{"failed"},"logs":report.output.lines,"error":report.failure.as_ref().map(ToString::to_string),"usage":usage,"permissions":{"artifact":report.artifact,"audit":report.audit,"auditDropped":report.audit_dropped,"decisions":report.decisions}});
            std::fs::write(
                directory.join(format!("{name}.golden.json")),
                serde_json::to_vec(&observation)?,
            )?;
        }
        if matches!(name, "view-live" | "view-async") {
            let live=br##"{"inputs":[{"input":"click","target":"#counter"},{"input":"type","target":"#entry","text":"hello"},{"input":"click","target":"#counter"}]}"##;
            let timed =
                br##"{"inputs":[{"input":"click","target":"#counter"},{"input":"wait","ms":10}]}"##;
            let script = varta_runtime::ViewScript::parse(if name == "view-live" {
                live.as_slice()
            } else {
                timed.as_slice()
            })?;
            let report = varta_runtime::VartaRuntime::new().run_view(
                &artifact.wasm,
                varta_runtime::ApplicationOptions {
                    run: varta_runtime::RunOptions {
                        requested: artifact.manifest.grants()?,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                &script,
            );
            if let Some(error) = report.report.failure {
                return Err(error.into());
            }
            let budget = report.report.budget.ok_or("no native view budget")?;
            let usage = serde_json::json!({"fuelRemaining":budget.budgets.fuel_per_turn.saturating_sub(budget.turns.last().map_or(0,|turn|turn.fuel)),"fuelUsed":budget.total_fuel,"turns":budget.total_turns,"hostCalls":budget.turns.iter().map(|turn|u64::from(turn.host_calls)).sum::<u64>(),"microtasks":budget.turns.iter().map(|turn|turn.microtasks).sum::<u64>(),"outputBytes":budget.output_bytes});
            std::fs::write(
                directory.join(format!("{name}.events.golden.json")),
                serde_json::to_vec(
                    &serde_json::json!({"logs":report.report.output.lines,"usage":usage,"snapshot":report.snapshot}),
                )?,
            )?;
        }
        if name == "view-permissions" {
            let script=varta_runtime::ViewScript::parse(br##"{"inputs":[{"input":"click","target":"#copy"},{"input":"click","target":"#external"}]}"##)?;
            let options = varta_runtime::ApplicationOptions {
                run: varta_runtime::RunOptions {
                    requested: artifact.manifest.grants()?,
                    ceiling: Some(vec![]),
                    ..Default::default()
                },
                ..Default::default()
            };
            let report =
                varta_runtime::VartaRuntime::new().run_view(&artifact.wasm, options, &script);
            if let Some(error) = report.report.failure {
                return Err(error.into());
            }
            let budget = report.report.budget.ok_or("no native view budget")?;
            let usage = serde_json::json!({"fuelRemaining":budget.budgets.fuel_per_turn.saturating_sub(budget.turns.last().map_or(0,|turn|turn.fuel)),"fuelUsed":budget.total_fuel,"turns":budget.total_turns,"hostCalls":budget.turns.iter().map(|turn|u64::from(turn.host_calls)).sum::<u64>(),"microtasks":budget.turns.iter().map(|turn|turn.microtasks).sum::<u64>(),"outputBytes":budget.output_bytes});
            std::fs::write(
                directory.join("view-permissions.denied.golden.json"),
                serde_json::to_vec(
                    &serde_json::json!({"logs":report.report.output.lines,"usage":usage}),
                )?,
            )?;
        }
        if name.starts_with("consent-") {
            use varta_common::permission::PromptAnswer;
            let script=varta_runtime::ViewScript::parse(br##"{"inputs":[{"input":"click","target":"#permission"},{"input":"click","target":"#permission"},{"input":"click","target":"#permission"}]}"##)?;
            for (label, answer) in [
                ("allow", PromptAnswer::Allow),
                ("deny", PromptAnswer::Deny),
                ("dismiss", PromptAnswer::Dismiss),
            ] {
                let report = varta_runtime::VartaRuntime::new()
                    .run_view(
                        &artifact.wasm,
                        varta_runtime::ApplicationOptions {
                            run: varta_runtime::RunOptions {
                                requested: artifact.manifest.grants()?,
                                consents: artifact.manifest.consents(),
                                ..Default::default()
                            },
                            prompter: Some(Box::new(FixedPrompt(answer))),
                            ..Default::default()
                        },
                        &script,
                    )
                    .report;
                let usage=report.budget.as_ref().map(|budget|serde_json::json!({"fuelRemaining":budget.budgets.fuel_per_turn.saturating_sub(budget.turns.last().map_or(0,|turn|turn.fuel)),"fuelUsed":budget.total_fuel,"turns":budget.total_turns,"hostCalls":budget.turns.iter().map(|turn|u64::from(turn.host_calls)).sum::<u64>(),"microtasks":budget.turns.iter().map(|turn|turn.microtasks).sum::<u64>(),"outputBytes":budget.output_bytes}));
                std::fs::write(
                    directory.join(format!("{name}.{label}.golden.json")),
                    serde_json::to_vec(
                        &serde_json::json!({"logs":report.output.lines,"usage":usage,"error":report.failure.as_ref().map(ToString::to_string),"permissions":{"artifact":report.artifact,"audit":report.audit,"auditDropped":report.audit_dropped,"decisions":report.decisions}}),
                    )?,
                )?;
            }
        }
        if name.starts_with("view-link-") {
            use varta_common::permission::PromptAnswer;
            let bytes = serde_json::to_vec(
                &serde_json::json!({"applications":[{"artifact":varta_runtime::artifact_identity(&artifact.wasm),"decisions":[{"id":"ui.open_url","decision":"allow"}]}]}),
            )?;
            std::fs::write(directory.join(format!("{name}.decisions.json")), &bytes)?;
            let script=varta_runtime::ViewScript::parse(br##"{"inputs":[{"input":"click","target":"#external"},{"input":"click","target":"#external"},{"input":"click","target":"#external"}]}"##)?;
            for (label, answer) in [
                ("allow", PromptAnswer::Allow),
                ("deny", PromptAnswer::Deny),
                ("dismiss", PromptAnswer::Dismiss),
            ] {
                let ceiling = match name {
                    "view-link-hostwide" | "view-link-hostexplicit" => {
                        Some(vec![varta_common::CapabilityGrant {
                            id: "ui.open_url".into(),
                            value: varta_common::CapabilityValue::UrlGlobs(vec![if name
                                == "view-link-hostwide"
                            {
                                varta_common::view::ANY_HTTPS_PATTERN.into()
                            } else {
                                "https://destination.invalid/**".into()
                            }]),
                        }])
                    }
                    "view-link-hostdeny" => Some(vec![]),
                    _ => None,
                };
                let result = varta_runtime::VartaRuntime::new().run_view(
                    &artifact.wasm,
                    varta_runtime::ApplicationOptions {
                        run: varta_runtime::RunOptions {
                            requested: artifact.manifest.grants()?,
                            consents: artifact.manifest.consents(),
                            decisions: Some(varta_common::permission::DecisionFile::parse(&bytes)?),
                            ceiling,
                            ..Default::default()
                        },
                        prompter: Some(Box::new(FixedPrompt(answer))),
                        ..Default::default()
                    },
                    &script,
                );
                let report = result.report;
                if let Some(error) = report.failure {
                    return Err(error.into());
                }
                let budget = report.budget.ok_or("no link budget")?;
                let usage = serde_json::json!({"fuelRemaining":budget.budgets.fuel_per_turn.saturating_sub(budget.turns.last().map_or(0,|turn|turn.fuel)),"fuelUsed":budget.total_fuel,"turns":budget.total_turns,"hostCalls":budget.turns.iter().map(|turn|u64::from(turn.host_calls)).sum::<u64>(),"microtasks":budget.turns.iter().map(|turn|turn.microtasks).sum::<u64>(),"outputBytes":budget.output_bytes});
                std::fs::write(
                    directory.join(format!("{name}.{label}.events.golden.json")),
                    serde_json::to_vec(
                        &serde_json::json!({"logs":report.output.lines,"usage":usage,"snapshot":result.snapshot,"permissions":{"artifact":report.artifact,"audit":report.audit,"auditDropped":report.audit_dropped,"decisions":report.decisions}}),
                    )?,
                )?;
            }
        }
        let prepared = BrowserArtifact::prepare(&artifact, None)?;
        let mut resources = if matches!(
            name,
            "view-image" | "view-image-missing" | "view-image-broken"
        ) {
            let mut bytes=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg==")?;
            if name == "view-image-broken" {
                bytes[42] ^= 255;
            }
            vec![BrowserResource::png("icons/red.png", bytes)?]
        } else {
            Vec::new()
        };
        if name == "view-font-packaged" {
            resources.extend(packaged_faces()?);
        } else if name.starts_with("view-font-") {
            let mut bytes = match name {
                "view-font-otf" => {
                    include_bytes!("../tests/fixtures/browser-fonts/advance-400.otf").to_vec()
                }
                "view-font-woff" => {
                    include_bytes!("../tests/fixtures/browser-fonts/advance-400.woff").to_vec()
                }
                "view-font-woff2" => {
                    include_bytes!("../tests/fixtures/browser-fonts/advance-400.woff2").to_vec()
                }
                "view-font-wide" => {
                    include_bytes!("../tests/fixtures/browser-fonts/advance-900.ttf").to_vec()
                }
                _ => include_bytes!("../tests/fixtures/browser-fonts/advance-400.ttf").to_vec(),
            };
            if name == "view-font-broken" {
                let at = (12..12 + 10 * 16)
                    .step_by(16)
                    .find(|at| &bytes[*at..*at + 4] == b"head")
                    .ok_or("no head table")?;
                bytes[at..at + 4].copy_from_slice(b"zzzz");
            }
            resources.push(BrowserResource::font("Brand", bytes)?);
        }
        prepared
            .bundle_with_resources(&resources)?
            .write_new(&directory.join(name))?;
    }
    println!("browser bundles generated in {}", directory.display());
    Ok(())
}
