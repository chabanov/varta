use wasmtime::{Engine, Store};

#[derive(Clone, Default)]
pub struct RuntimeContext {
    pub capabilities: Vec<varta_common::CapabilityGrant>,
}

impl RuntimeContext {
    pub fn has_capability_namespace(&self, namespace: &str) -> bool {
        self.capabilities
            .iter()
            .any(|grant| grant.id.split('.').next() == Some(namespace))
    }

    pub fn allows_env_access(&self, capability_id: &str, name: &str) -> bool {
        for grant in &self.capabilities {
            if grant.id != capability_id && !grant.id.starts_with("env.") {
                continue;
            }
            match &grant.value {
                varta_common::CapabilityValue::Boolean(true) => return true,
                varta_common::CapabilityValue::PathGlobs(patterns)
                    if patterns.iter().any(|pattern| glob_match(pattern, name)) =>
                {
                    return true;
                }
                _ => {}
            }
        }
        false
    }

    pub fn allows_fs_read(&self, path: &str) -> bool {
        for grant in &self.capabilities {
            if grant.id != "fs" && !grant.id.starts_with("fs.") {
                continue;
            }
            if let varta_common::CapabilityValue::PathGlobs(patterns) = &grant.value
                && patterns.iter().any(|pattern| glob_match(pattern, path))
            {
                return true;
            }
        }
        false
    }

    pub fn allows_net_fetch(&self, url: &str) -> bool {
        for grant in &self.capabilities {
            if grant.id != "net.fetch" && !grant.id.starts_with("net.") {
                continue;
            }
            let patterns = match &grant.value {
                varta_common::CapabilityValue::PathGlobs(patterns)
                | varta_common::CapabilityValue::UrlGlobs(patterns) => patterns,
                _ => continue,
            };
            if patterns.iter().any(|pattern| glob_match(pattern, url)) {
                return true;
            }
        }
        false
    }
}

pub struct VartaRuntime {
    engine: Engine,
}

impl Default for VartaRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl VartaRuntime {
    pub fn new() -> Self {
        tracing::info!("initializing varta runtime");
        Self {
            engine: Engine::default(),
        }
    }

    pub fn precompile_module(&self, bytes: &[u8]) -> Result<Vec<u8>, wasmtime::Error> {
        self.engine.precompile_module(bytes)
    }

    pub fn run_wasm_bytes(
        &self,
        bytes: &[u8],
        capabilities: &[varta_common::CapabilityGrant],
    ) -> Result<(), wasmtime::Error> {
        let module = wasmtime::Module::new(&self.engine, bytes)?;
        self.run_module(module, capabilities)
    }

    pub fn run_app_bytes(
        &self,
        bytes: &[u8],
        precompiled: bool,
        capabilities: &[varta_common::CapabilityGrant],
    ) -> Result<(), wasmtime::Error> {
        let module = if precompiled {
            unsafe { wasmtime::Module::deserialize(&self.engine, bytes) }?
        } else {
            wasmtime::Module::new(&self.engine, bytes)?
        };
        self.run_module(module, capabilities)
    }

    fn run_module(
        &self,
        module: wasmtime::Module,
        capabilities: &[varta_common::CapabilityGrant],
    ) -> Result<(), wasmtime::Error> {
        let mut store = Store::new(
            &self.engine,
            RuntimeContext {
                capabilities: capabilities.to_vec(),
            },
        );

        let mut linker = wasmtime::Linker::new(&self.engine);
        linker.func_wrap("varta", "log_i32", |value: i32| {
            println!("varta log: {value}");
        })?;
        linker.func_wrap(
            "varta",
            "process_exit",
            |caller: wasmtime::Caller<'_, RuntimeContext>, code: i32| {
                if !caller.data().has_capability_namespace("process") {
                    println!("varta log: <capability denied: process>");
                    return;
                }
                std::process::exit(code);
            },
        )?;
        linker.func_wrap(
            "varta",
            "log_string",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
                if ptr < 0 || len < 0 {
                    println!("varta log: <invalid string pointer>");
                    return;
                }
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    println!("varta log: <memory export missing>");
                    return;
                };
                let mut buffer = vec![0u8; len as usize];
                if memory.read(&mut caller, ptr as usize, &mut buffer).is_ok() {
                    println!("varta log: {}", String::from_utf8_lossy(&buffer));
                } else {
                    println!("varta log: <invalid string memory>");
                }
            },
        )?;
        linker.func_wrap(
            "varta",
            "time_now",
            |caller: wasmtime::Caller<'_, RuntimeContext>| -> i32 {
                if !caller.data().has_capability_namespace("time") {
                    println!("varta log: <capability denied: time>");
                    return 0;
                }
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|duration| duration.as_secs() as i32)
                    .unwrap_or(0)
            },
        )?;
        linker.func_wrap(
            "varta",
            "env_has",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| -> i32 {
                if ptr < 0 || len < 0 {
                    return 0;
                }
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    return 0;
                };
                let mut buffer = vec![0u8; len as usize];
                if memory.read(&mut caller, ptr as usize, &mut buffer).is_err() {
                    return 0;
                }
                let Ok(name) = std::str::from_utf8(&buffer) else {
                    return 0;
                };
                if !caller.data().allows_env_access("env.has", name) {
                    return 0;
                }
                std::env::var_os(name).is_some() as i32
            },
        )?;
        linker.func_wrap(
            "varta",
            "env_print",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
                if ptr < 0 || len < 0 {
                    println!("varta log: <invalid string pointer>");
                    return;
                }
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    println!("varta log: <memory export missing>");
                    return;
                };
                let mut buffer = vec![0u8; len as usize];
                if memory.read(&mut caller, ptr as usize, &mut buffer).is_err() {
                    println!("varta log: <invalid string memory>");
                    return;
                }
                let Ok(name) = std::str::from_utf8(&buffer) else {
                    println!("varta log: <invalid environment variable name>");
                    return;
                };
                if !caller.data().allows_env_access("env.print", name) {
                    println!("varta log: <capability denied: env>");
                    return;
                }
                match std::env::var(name) {
                    Ok(value) => println!("varta env: {name}={value}"),
                    Err(_) => println!("varta env: {name}=<unset>"),
                }
            },
        )?;
        linker.func_wrap(
            "varta",
            "env_get",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| -> i32 {
                if ptr < 0 || len < 0 {
                    return 0;
                }
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    return 0;
                };
                let mut buffer = vec![0u8; len as usize];
                if memory.read(&mut caller, ptr as usize, &mut buffer).is_err() {
                    return 0;
                }
                let Ok(name) = std::str::from_utf8(&buffer) else {
                    return 0;
                };
                if !caller.data().allows_env_access("env.get", name) {
                    println!("varta log: <capability denied: env>");
                    return 0;
                }
                let Ok(value) = std::env::var(name) else {
                    let Some(wasmtime::Extern::Global(env_len)) =
                        caller.get_export("env_result_len")
                    else {
                        return 0;
                    };
                    let _ = env_len.set(&mut caller, wasmtime::Val::I32(0));
                    return 0;
                };
                let value = value.into_bytes();
                let Some(wasmtime::Extern::Global(heap)) = caller.get_export("heap") else {
                    return 0;
                };
                let Some(wasmtime::Extern::Global(env_len)) = caller.get_export("env_result_len")
                else {
                    return 0;
                };
                let current_heap = heap.get(&mut caller).i32().unwrap_or(8192);
                let base = if current_heap == 0 {
                    8192
                } else {
                    current_heap
                };
                let end = base + value.len() as i32;
                let size = memory.data_size(&caller) as i32;
                if end > size {
                    let pages = ((end - size + 65535) / 65536) as u64;
                    if memory.grow(&mut caller, pages).is_err() {
                        return 0;
                    }
                }
                if memory.write(&mut caller, base as usize, &value).is_err() {
                    return 0;
                }
                let _ = heap.set(&mut caller, wasmtime::Val::I32(base + value.len() as i32));
                let _ = env_len.set(&mut caller, wasmtime::Val::I32(value.len() as i32));
                base
            },
        )?;
        linker.func_wrap(
            "varta",
            "fs_print",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
                if ptr < 0 || len < 0 {
                    println!("varta log: <invalid string pointer>");
                    return;
                }
                if !caller.data().has_capability_namespace("fs") {
                    println!("varta log: <capability denied: fs>");
                    return;
                }
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    println!("varta log: <memory export missing>");
                    return;
                };
                let mut buffer = vec![0u8; len as usize];
                if memory.read(&mut caller, ptr as usize, &mut buffer).is_err() {
                    println!("varta log: <invalid string memory>");
                    return;
                }
                let Ok(path) = std::str::from_utf8(&buffer) else {
                    println!("varta log: <invalid file path>");
                    return;
                };
                if path.contains("..") || !caller.data().allows_fs_read(path) {
                    println!("varta log: <capability denied: fs.read>");
                    return;
                }
                match std::fs::read_to_string(path) {
                    Ok(contents) => print!("{contents}"),
                    Err(_) => println!("varta log: <failed to read file>"),
                }
            },
        )?;
        linker.func_wrap(
            "varta",
            "fs_read",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| -> i32 {
                if ptr < 0 || len < 0 {
                    return 0;
                }
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    return 0;
                };
                let mut buffer = vec![0u8; len as usize];
                if memory.read(&mut caller, ptr as usize, &mut buffer).is_err() {
                    return 0;
                }
                let Ok(path) = std::str::from_utf8(&buffer) else {
                    return 0;
                };
                if path.contains("..") || !caller.data().allows_fs_read(path) {
                    println!("varta log: <capability denied: fs.read>");
                    return 0;
                }
                let Ok(contents) = std::fs::read(path) else {
                    println!("varta log: <failed to read file>");
                    return 0;
                };
                let Some(wasmtime::Extern::Global(heap)) = caller.get_export("heap") else {
                    return 0;
                };
                let Some(wasmtime::Extern::Global(result_len)) =
                    caller.get_export("env_result_len")
                else {
                    return 0;
                };
                let current_heap = heap.get(&mut caller).i32().unwrap_or(0);
                let base = if current_heap == 0 {
                    memory.data_size(&caller) as i32
                } else {
                    current_heap
                };
                let end = base + contents.len() as i32;
                let size = memory.data_size(&caller) as i32;
                if end > size {
                    let pages = ((end - size + 65535) / 65536) as u64;
                    if memory.grow(&mut caller, pages).is_err() {
                        return 0;
                    }
                }
                if memory.write(&mut caller, base as usize, &contents).is_err() {
                    return 0;
                }
                let _ = heap.set(
                    &mut caller,
                    wasmtime::Val::I32(base + contents.len() as i32),
                );
                let _ = result_len.set(&mut caller, wasmtime::Val::I32(contents.len() as i32));
                base
            },
        )?;
        linker.func_wrap(
            "varta",
            "net_fetch",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| -> i32 {
                if ptr < 0 || len < 0 {
                    return -1;
                }
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    return -1;
                };
                let mut buffer = vec![0u8; len as usize];
                if memory.read(&mut caller, ptr as usize, &mut buffer).is_err() {
                    return -1;
                }
                let Ok(url) = std::str::from_utf8(&buffer) else {
                    return -1;
                };
                if !caller.data().allows_net_fetch(url) {
                    println!("varta log: <capability denied: net.fetch>");
                    return -1;
                }
                fetch_http_status(url)
            },
        )?;

        linker.func_wrap(
            "varta",
            "net_get",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| -> i32 {
                if ptr < 0 || len < 0 {
                    return 0;
                }
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    return 0;
                };
                let mut buffer = vec![0u8; len as usize];
                if memory.read(&mut caller, ptr as usize, &mut buffer).is_err() {
                    return 0;
                }
                let Ok(url) = std::str::from_utf8(&buffer) else {
                    return 0;
                };
                if !caller.data().allows_net_fetch(url) {
                    println!("varta log: <capability denied: net.fetch>");
                    return 0;
                }
                let Some(body) = fetch_http_body(url) else {
                    println!("varta log: <no response body>");
                    return 0;
                };

                let Some(wasmtime::Extern::Global(heap)) = caller.get_export("heap") else {
                    return 0;
                };
                let Some(wasmtime::Extern::Global(result_len)) =
                    caller.get_export("env_result_len")
                else {
                    return 0;
                };

                let current_heap = heap.get(&mut caller).i32().unwrap_or(0);
                let base = if current_heap == 0 {
                    memory.data_size(&caller) as i32
                } else {
                    current_heap
                };
                let end = base + body.len() as i32;
                let size = memory.data_size(&caller) as i32;
                if end > size {
                    let pages = ((end - size + 65535) / 65536) as u64;
                    if memory.grow(&mut caller, pages).is_err() {
                        return 0;
                    }
                }
                if memory.write(&mut caller, base as usize, &body).is_err() {
                    println!("varta log: <failed to write response>");
                    return 0;
                }
                let _ = heap.set(&mut caller, wasmtime::Val::I32(base + body.len() as i32));
                let _ = result_len.set(&mut caller, wasmtime::Val::I32(body.len() as i32));
                base
            },
        )?;

        let instance = linker.instantiate(&mut store, &module)?;
        let start = instance.get_func(&mut store, "_start");
        if let Some(start) = start {
            start.call(&mut store, &[], &mut [])?;
        }
        Ok(())
    }
}

fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let mut pattern_index = 0usize;
    let mut text_index = 0usize;
    let mut star_index = None;
    let mut match_index = 0usize;

    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == '?' || pattern[pattern_index] == text[text_index])
        {
            pattern_index += 1;
            text_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star_index = Some(pattern_index);
            pattern_index += 1;
            match_index = text_index;
        } else if let Some(star) = star_index {
            pattern_index = star + 1;
            match_index += 1;
            text_index = match_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

fn fetch_http_body(url: &str) -> Option<Vec<u8>> {
    let rest = url.strip_prefix("http://")?;
    let (host_part, raw_path) = match rest.split_once('/') {
        Some((host, path)) => (host, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match host_part.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            (host, port.to_string())
        }
        _ => (host_part, "80".to_string()),
    };

    let Ok(stream) = std::net::TcpStream::connect(format!("{host}:{port}")) else {
        return None;
    };
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));

    use std::io::{BufRead, Read, Write};
    let mut stream = stream;
    if write!(
        stream,
        "GET {raw_path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .is_err()
    {
        return None;
    }

    let mut reader = std::io::BufReader::new(stream);
    let mut status_line = String::new();
    if reader.read_line(&mut status_line).is_err() {
        return None;
    }
    if !status_line.contains(" 2") {
        return None;
    }

    let mut headers = String::new();
    loop {
        headers.clear();
        if reader.read_line(&mut headers).is_err() {
            return None;
        }
        if headers == "\r\n" || headers == "\n" || headers.is_empty() {
            break;
        }
    }

    let mut body = Vec::new();
    if reader.read_to_end(&mut body).is_err() {
        return None;
    }
    Some(body)
}

fn fetch_http_status(url: &str) -> i32 {
    let Some(rest) = url.strip_prefix("http://") else {
        return -1;
    };
    let (host_part, raw_path) = match rest.split_once('/') {
        Some((host, path)) => (host, format!("/{path}")),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match host_part.rsplit_once(':') {
        Some((host, port)) if !host.is_empty() && port.chars().all(|c| c.is_ascii_digit()) => {
            (host, port.to_string())
        }
        _ => (host_part, "80".to_string()),
    };

    let Ok(stream) = std::net::TcpStream::connect(format!("{host}:{port}")) else {
        return -1;
    };
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let _ = stream.set_write_timeout(Some(std::time::Duration::from_secs(5)));

    use std::io::Write;
    let mut stream = stream;
    if write!(
        stream,
        "GET {raw_path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .is_err()
    {
        return -1;
    }

    use std::io::BufRead;
    let mut reader = std::io::BufReader::new(stream);
    let mut status_line = String::new();
    if reader.read_line(&mut status_line).is_err() {
        return -1;
    }

    status_line
        .split_whitespace()
        .nth(1)
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(-1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_minimal_wasm_module() {
        let runtime = VartaRuntime::new();
        let module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        runtime.run_wasm_bytes(&module, &[]).unwrap();
        runtime.run_app_bytes(&module, false, &[]).unwrap();
    }

    #[test]
    fn validates_string_module() {
        let source = "export function main(): void { console.log(\"hello\"); }";
        let bytes = varta_compiler::compile_typescript(source).unwrap();
        let engine = wasmtime::Engine::default();
        let result = wasmtime::Module::new(&engine, &bytes);
        assert!(result.is_ok(), "module validation failed: {result:?}");
    }

    #[test]
    fn validates_control_flow_module() {
        // `&&`, `||` and `switch` use compiler temporaries beyond named
        // variables; the function must declare those local slots.
        let source = r#"
export function main(): void {
  let a = 1;
  let b = 2;
  console.log(a && b);
  console.log(a || b);
  switch (a) {
    case 1:
      console.log("one");
      break;
    default:
      console.log("other");
  }
}
"#;
        let bytes = varta_compiler::compile_typescript(source).unwrap();
        let engine = wasmtime::Engine::default();
        let result = wasmtime::Module::new(&engine, &bytes);
        assert!(result.is_ok(), "module validation failed: {result:?}");
    }

    #[test]
    fn validates_host_result_logging_module() {
        // `console.log(varta.env.get(...))` and `console.log(varta.fs.read(...))`
        // must call the resolved log_string import, not a literal import index.
        let source = r#"
export function main(): void {
  console.log(varta.env.get("HOME"));
  console.log(varta.fs.read("/data/hello.txt"));
}
"#;
        let bytes = varta_compiler::compile_typescript(source).unwrap();
        let engine = wasmtime::Engine::default();
        let result = wasmtime::Module::new(&engine, &bytes);
        assert!(result.is_ok(), "module validation failed: {result:?}");
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use varta_common::{CapabilityGrant, CapabilityValue};

    #[test]
    fn runs_minimal_module() {
        let runtime = VartaRuntime::new();
        let module = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        runtime.run_wasm_bytes(&module, &[]).unwrap();
    }

    #[test]
    fn fs_capability_allows_exact_and_glob() {
        let context = RuntimeContext {
            capabilities: vec![CapabilityGrant {
                id: "fs.read".to_string(),
                value: CapabilityValue::PathGlobs(vec!["/data/**".to_string()]),
            }],
        };
        assert!(context.allows_fs_read("/data/hello.txt"));
        assert!(!context.allows_fs_read("/secret.txt"));
    }

    #[test]
    fn env_capability_requires_name() {
        let context = RuntimeContext {
            capabilities: vec![CapabilityGrant {
                id: "env.has".to_string(),
                value: CapabilityValue::PathGlobs(vec!["PATH".to_string()]),
            }],
        };
        assert!(context.allows_env_access("env.has", "PATH"));
        assert!(!context.allows_env_access("env.has", "SECRET"));
    }
}
