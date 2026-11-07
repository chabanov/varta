//! Managed KV values must cross asynchronous request Stores through the same
//! checked allocator/retention boundary as ordinary application execution.
use super::*;
use varta_common::language::LanguageProfile;
struct Directory(std::path::PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "varta-http-kv-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn options(&self) -> RunOptions {
        let mut roots = crate::FileRoots::new();
        roots.open("data", &self.0).unwrap();
        RunOptions {
            file_roots: roots,
            requested: ["kv.read", "kv.write"]
                .into_iter()
                .map(|id| CapabilityGrant {
                    id: id.into(),
                    value: CapabilityValue::PathGlobs(vec!["/data/state.kv".into()]),
                })
                .collect(),
            ..Default::default()
        }
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
const RECORD: &str = r#"
interface StoredRecord { text:string;revision:number; }
export function main():void {}
async function handler(request:VartaHttpRequest):Promise<VartaHttpResponse>{
 const kv=await varta.kv.open('/data/state.kv');
 try {
  const entry=await kv.get<StoredRecord>(['record']);
  if(request.method==='POST'&&entry.value===null){
   const value:StoredRecord={text:'synthetic',revision:1};await kv.set(['record'],value);
   return {status:201,body:JSON.stringify(value),headers:[]};
  }
  return {status:entry.value===null?404:200,body:JSON.stringify(entry.value),headers:[]};
 }finally{kv.close();}
}
varta.http.serve({hostname:'127.0.0.1',port:0},handler);
"#;
#[test]
fn async_http_kv_record_reads_across_requests_and_server_restart() {
    for profile in [
        LanguageProfile::CheckedF64Utf16V38,
        LanguageProfile::CheckedF64Utf16V39,
    ] {
        let directory = Directory::new();
        let bytes = varta_compiler::compile_typescript_with_profile(RECORD, profile).unwrap();
        let server = Server::start_options(bytes.clone(), directory.options());
        let (status, _, body) = server.request("POST", "/", b"");
        assert_eq!(
            status,
            201,
            "{body}; {:?}",
            server.diagnostics.lock().unwrap()
        );
        for method in ["GET", "POST"] {
            let (status, _, body) = server.request(method, "/", b"");
            assert_eq!(
                status,
                200,
                "{body}; {:?}",
                server.diagnostics.lock().unwrap()
            );
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(&body).unwrap(),
                serde_json::json!({"text":"synthetic","revision":1})
            );
        }
        server.finish();
        let server = Server::start_options(bytes, directory.options());
        assert_eq!(server.request("GET", "/", b"").0, 200);
        server.finish();
    }
}

const COMPLEX: &str = r#"
interface Item {text:string;revision:number;labels:string[];meta:{[key:string]:number};value:number|string;note?:string;}
export function main():void{}
async function handler(request:VartaHttpRequest):Promise<VartaHttpResponse>{
 const kv=await varta.kv.open('/data/state.kv');
 try{
  if(request.target==='/seed'){
   const labels:string[]=[];for(let i=0;i<16;i++){labels.push('label-'+i);}
   const item:Item={text:'stored🦀',revision:1,labels,meta:{alpha:7,beta:9},value:'union'};
   await kv.set(['items','a'],item);await kv.set(['items','b'],item);
   return {status:201,body:'seeded',headers:[]};
  }
  if(request.target==='/wrong'){
   let caught=false;try{await kv.get<number>(['items','a']);}catch(error){caught=error instanceof TypeError;}
   const after=await kv.get<Item>(['items','a']);
   return {status:caught&&after.value!==null&&after.value.meta.beta===9?200:500,body:'recovered',headers:[]};
  }
  if(request.target==='/write'){
   try{await kv.set(['denied'],123);}catch(error){return {status:error instanceof Error?403:500,body:'denied',headers:[]};}
   return {status:500,body:'unexpected write',headers:[]};
  }
  if(request.target==='/missing'){
   const entry=await kv.get<number>(['denied']);return {status:entry.value===null?404:500,body:'missing',headers:[]};
  }
  if(request.target==='/update'){
   const current=await kv.get<Item>(['items','a']);if(current.value===null){throw new Error('missing');}
   const changed:Item={...current.value,revision:2,note:'updated'};
   const result=await kv.atomic().check(current).set(['items','a'],changed).commit();
   return {status:result.ok?200:409,body:'updated',headers:[]};
  }
  let count=0;
  for await(const entry of kv.list<Item>({prefix:['items']})){
   const value=entry.value;
   if(JSON.stringify(entry.key)!==(count===0?'["items","a"]':'["items","b"]')){throw new Error('key');}
   if(value.text!=='stored🦀'||value.labels[15]!=='label-15'||value.meta.alpha!==7||value.value!=='union'){throw new Error('value');}
   count++;
  }
  const first=await kv.get<Item>(['items','a']);
  if(first.value!==null&&first.value.revision===2&&first.value.note!=='updated'){throw new Error('optional');}
  return {status:200,body:String(count),headers:[]};
 }finally{kv.close();}
}
varta.http.serve({hostname:'127.0.0.1',port:0},handler);
"#;
fn complex(stress: bool) -> Vec<u8> {
    let compile = || {
        varta_compiler::compile_typescript_with_profile(
            COMPLEX,
            LanguageProfile::CheckedF64Utf16V39,
        )
        .unwrap()
    };
    if stress {
        varta_compiler::with_collector_stress(8, compile)
    } else {
        compile()
    }
}
#[test]
fn async_http_kv_nested_values_keys_mismatch_recovery_and_atomic_updates_survive_gc() {
    let directory = Directory::new();
    let bytes = complex(true);
    let server = Server::start_options(bytes.clone(), directory.options());
    for (method, path, status, body) in [
        ("POST", "/seed", 201, "seeded"),
        ("GET", "/list", 200, "2"),
        ("GET", "/wrong", 200, "recovered"),
        ("POST", "/update", 200, "updated"),
        ("GET", "/list", 200, "2"),
    ] {
        let (actual, _, text) = server.request(method, path, b"");
        assert_eq!(
            (actual, text.as_str()),
            (status, body),
            "{:?}",
            server.diagnostics.lock().unwrap()
        );
    }
    server.finish();
    let server = Server::start_options(bytes, directory.options());
    assert_eq!(server.request("GET", "/list", b"").2, "2");
    server.finish();
}
#[test]
fn async_http_kv_host_ceiling_keeps_reads_and_refuses_writes_without_persistence() {
    let directory = Directory::new();
    let bytes = complex(false);
    let server = Server::start_options(bytes.clone(), directory.options());
    assert_eq!(server.request("POST", "/seed", b"").0, 201);
    server.finish();
    let mut options = directory.options();
    options.ceiling = Some(vec![
        grant("127.0.0.1:0".into()),
        CapabilityGrant {
            id: "kv.read".into(),
            value: CapabilityValue::PathGlobs(vec!["/data/state.kv".into()]),
        },
    ]);
    let server = Server::start_options(bytes.clone(), options);
    assert_eq!(server.request("GET", "/list", b"").0, 200);
    assert_eq!(server.request("POST", "/write", b"").0, 403);
    assert_eq!(server.request("GET", "/list", b"").0, 200);
    server.finish();
    let server = Server::start_options(bytes, directory.options());
    assert_eq!(server.request("GET", "/missing", b"").0, 404);
    server.finish();
}

#[test]
fn async_http_kv_callbacks_fit_a_single_worker_pool() {
    let directory = Directory::new();
    let bytes = varta_compiler::compile_typescript_with_profile(
        RECORD,
        LanguageProfile::CheckedF64Utf16V39,
    )
    .unwrap();
    let mut options = directory.options();
    options.http = Some(varta_common::http::HttpPolicy {
        limits: Some(varta_common::http::HostLimits {
            workers: Some(1),
            ..Default::default()
        }),
        tls: None,
    });
    let server = Server::start_options(bytes, options);
    assert_eq!(server.request("POST", "/", b"").0, 201);
    let (status, _, body) = server.request("GET", "/", b"");
    assert_eq!(
        status,
        200,
        "{body}; {:?}",
        server.diagnostics.lock().unwrap()
    );
    server.finish();
}
