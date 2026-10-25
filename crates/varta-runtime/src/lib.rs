use wasmtime::{Engine, Store};

/// Maximum bytes accepted from a Wasm guest for an input pointer/length pair.
const MAX_HOST_INPUT_BYTES: usize = 64 * 1024;
/// Maximum bytes a host function may write into guest memory during one call.
const MAX_HOST_RESULT_BYTES: usize = 1_048_576;
/// Maximum total linear memory size maintained by host calls.
const MAX_RUNTIME_MEMORY_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct RuntimeContext {
    pub capabilities: Vec<varta_common::CapabilityGrant>,
}

impl RuntimeContext {
    pub fn has_capability(&self, capability_id: &str) -> bool {
        self.capabilities.iter().any(|grant| {
            grant.id == capability_id
                && match &grant.value {
                    varta_common::CapabilityValue::Boolean(enabled) => *enabled,
                    varta_common::CapabilityValue::PathGlobs(patterns) => !patterns.is_empty(),
                    varta_common::CapabilityValue::UrlGlobs(patterns) => !patterns.is_empty(),
                    varta_common::CapabilityValue::Ports(ports) => !ports.is_empty(),
                }
        })
    }

    pub fn allows_env_access(&self, capability_id: &str, name: &str) -> bool {
        for grant in &self.capabilities {
            if grant.id != capability_id {
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
        let Some(normalized) = Self::canonical_fs_path(path) else {
            return false;
        };
        for grant in &self.capabilities {
            if grant.id != "fs.read" {
                continue;
            }
            if let varta_common::CapabilityValue::PathGlobs(patterns) = &grant.value
                && patterns
                    .iter()
                    .any(|pattern| glob_match(&Self::canonical_fs_pattern(pattern), &normalized))
            {
                return true;
            }
        }
        false
    }

    pub fn allows_fs_print(&self, path: &str) -> bool {
        let Some(normalized) = Self::canonical_fs_path(path) else {
            return false;
        };
        for grant in &self.capabilities {
            if grant.id != "fs.print" {
                continue;
            }
            if let varta_common::CapabilityValue::PathGlobs(patterns) = &grant.value
                && patterns
                    .iter()
                    .any(|pattern| glob_match(&Self::canonical_fs_pattern(pattern), &normalized))
            {
                return true;
            }
        }
        false
    }

    /// Canonicalize the non-wildcard part of a capability pattern so that
    /// grants match the canonical input path rather than its undeclared alias.
    fn canonical_fs_pattern(pattern: &str) -> String {
        use std::path::Path;

        let Some(wildcard) = pattern.find(['*', '?']) else {
            return Self::canonical_fs_path(pattern).unwrap_or_else(|| pattern.to_string());
        };

        let prefix = &pattern[..wildcard];
        let suffix = &pattern[wildcard..];
        let anchor = prefix.trim_end_matches('/');
        if anchor.is_empty() {
            return pattern.to_string();
        }

        if let Ok(base) = Path::new(anchor).canonicalize() {
            return format!("{}{}", base.to_string_lossy(), suffix);
        }

        if let Some(parent) = Path::new(anchor).parent()
            && parent.exists()
            && let Ok(base) = parent.canonicalize()
        {
            let name = Path::new(anchor)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            return format!("{}{}{}", base.to_string_lossy(), name, suffix);
        }

        pattern.to_string()
    }

    /// Resolve a guest path to its canonical form before capability matching.
    ///
    /// Traversal is rejected outright, existing paths are followed through
    /// `std::fs::canonicalize` (including symlink targets), and missing
    /// final path components fall back to their existing parent or to the
    /// lexically validated input only when no `..` traversal was present.
    fn canonical_fs_path(path: &str) -> Option<String> {
        use std::path::{Component, Path};

        let input = Path::new(path);
        for component in input.components() {
            if matches!(component, Component::ParentDir) {
                return None;
            }
        }

        if let Ok(canonical) = input.canonicalize() {
            return Some(canonical.to_string_lossy().into_owned());
        }

        if let (Some(parent), Some(name)) = (input.parent(), input.file_name())
            && !parent.as_os_str().is_empty()
            && parent.exists()
            && let Ok(base) = parent.canonicalize()
        {
            return Some(base.join(name).to_string_lossy().into_owned());
        }

        Some(path.to_string())
    }

    pub fn allows_net_fetch(&self, url: &str) -> bool {
        for grant in &self.capabilities {
            if grant.id != "net.fetch" {
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

    pub fn allows_net_get(&self, url: &str) -> bool {
        for grant in &self.capabilities {
            if grant.id != "net.get" {
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
                if !caller.data().has_capability("process.exit") {
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
                if ptr < 0 || len < 0 || len as usize > MAX_HOST_INPUT_BYTES {
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
                if let Ok(text) = std::str::from_utf8(&buffer) {
                    println!("varta log: {text}");
                } else {
                    println!("varta log: <invalid utf-8>");
                }
            },
        )?;
        linker.func_wrap(
            "varta",
            "time_now",
            |caller: wasmtime::Caller<'_, RuntimeContext>| -> i32 {
                if !caller.data().has_capability("time.now") {
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
                if ptr < 0 || len < 0 || len as usize > MAX_HOST_INPUT_BYTES {
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
                if ptr < 0 || len < 0 || len as usize > MAX_HOST_INPUT_BYTES {
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
                if ptr < 0 || len < 0 || len as usize > MAX_HOST_INPUT_BYTES {
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
                if value.len() > MAX_HOST_RESULT_BYTES {
                    println!("varta log: <result too large>");
                    return 0;
                }
                let Some(base) = reserve_result_region(&mut caller, value.len()) else {
                    println!("varta log: <result too large>");
                    return 0;
                };
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    println!("varta log: <memory export missing>");
                    return 0;
                };
                if memory.write(&mut caller, base, &value).is_err() {
                    println!("varta log: <failed to write response>");
                    return 0;
                }
                let Some(wasmtime::Extern::Global(heap)) = caller.get_export("heap") else {
                    return 0;
                };
                let Some(wasmtime::Extern::Global(result_len)) =
                    caller.get_export("env_result_len")
                else {
                    return 0;
                };
                let _ = heap.set(
                    &mut caller,
                    wasmtime::Val::I32(base as i32 + value.len() as i32),
                );
                let _ = result_len.set(&mut caller, wasmtime::Val::I32(value.len() as i32));
                base as i32
            },
        )?;
        linker.func_wrap(
            "varta",
            "fs_print",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| {
                if ptr < 0 || len < 0 || len as usize > MAX_HOST_INPUT_BYTES {
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
                let Ok(path) = std::str::from_utf8(&buffer) else {
                    println!("varta log: <invalid file path>");
                    return;
                };
                if !caller.data().allows_fs_print(path) {
                    println!("varta log: <capability denied: fs.print>");
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
                if ptr < 0 || len < 0 || len as usize > MAX_HOST_INPUT_BYTES {
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
                if !caller.data().allows_fs_read(path) {
                    println!("varta log: <capability denied: fs.read>");
                    return 0;
                }
                let Ok(contents) = std::fs::read(path) else {
                    println!("varta log: <failed to read file>");
                    return 0;
                };
                if contents.len() > MAX_HOST_RESULT_BYTES {
                    println!("varta log: <result too large>");
                    return 0;
                }
                let Some(base) = reserve_result_region(&mut caller, contents.len()) else {
                    println!("varta log: <result too large>");
                    return 0;
                };
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    return 0;
                };
                if memory.write(&mut caller, base, &contents).is_err() {
                    println!("varta log: <failed to write response>");
                    return 0;
                }
                let Some(wasmtime::Extern::Global(heap)) = caller.get_export("heap") else {
                    return 0;
                };
                let Some(wasmtime::Extern::Global(result_len)) =
                    caller.get_export("env_result_len")
                else {
                    return 0;
                };
                let _ = heap.set(
                    &mut caller,
                    wasmtime::Val::I32(base as i32 + contents.len() as i32),
                );
                let _ = result_len.set(&mut caller, wasmtime::Val::I32(contents.len() as i32));
                base as i32
            },
        )?;
        linker.func_wrap(
            "varta",
            "net_fetch",
            |mut caller: wasmtime::Caller<'_, RuntimeContext>, ptr: i32, len: i32| -> i32 {
                if ptr < 0 || len < 0 || len as usize > MAX_HOST_INPUT_BYTES {
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
                if ptr < 0 || len < 0 || len as usize > MAX_HOST_INPUT_BYTES {
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
                if !caller.data().allows_net_get(url) {
                    println!("varta log: <capability denied: net.get>");
                    return 0;
                }
                let Some(body) = fetch_http_body(url) else {
                    println!("varta log: <no response body>");
                    return 0;
                };

                if body.len() > MAX_HOST_RESULT_BYTES {
                    println!("varta log: <result too large>");
                    return 0;
                }
                let Some(base) = reserve_result_region(&mut caller, body.len()) else {
                    println!("varta log: <result too large>");
                    return 0;
                };
                let Some(wasmtime::Extern::Memory(memory)) = caller.get_export("memory") else {
                    return 0;
                };
                if memory.write(&mut caller, base, &body).is_err() {
                    println!("varta log: <failed to write response>");
                    return 0;
                }
                let Some(wasmtime::Extern::Global(heap)) = caller.get_export("heap") else {
                    return 0;
                };
                let Some(wasmtime::Extern::Global(result_len)) =
                    caller.get_export("env_result_len")
                else {
                    return 0;
                };
                let _ = heap.set(
                    &mut caller,
                    wasmtime::Val::I32(base as i32 + body.len() as i32),
                );
                let _ = result_len.set(&mut caller, wasmtime::Val::I32(body.len() as i32));
                base as i32
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

/// Reserve guest linear memory for a host result. The caller writes after
/// a successful reservation, which avoids mutating caller state when the write
/// fails. Returns the base pointer or `None` when the result cannot be stored.
fn reserve_result_region(
    caller: &mut wasmtime::Caller<'_, RuntimeContext>,
    len: usize,
) -> Option<usize> {
    if len > MAX_HOST_RESULT_BYTES {
        return None;
    }

    let memory = match caller.get_export("memory") {
        Some(wasmtime::Extern::Memory(memory)) => memory,
        _ => return None,
    };
    let heap = match caller.get_export("heap") {
        Some(wasmtime::Extern::Global(heap)) => heap,
        _ => return None,
    };
    let current_heap = heap.get(&mut *caller).i32().unwrap_or(0);
    let base = if current_heap == 0 {
        memory.data_size(&*caller) as i32
    } else {
        current_heap
    };
    if base < 0 || base as usize > MAX_RUNTIME_MEMORY_BYTES {
        return None;
    }
    let desired = base as usize + len;
    if desired > MAX_RUNTIME_MEMORY_BYTES {
        return None;
    }
    if desired > memory.data_size(&*caller) {
        let pages = desired
            .saturating_sub(memory.data_size(&*caller))
            .div_ceil(65536) as u64;
        if memory.grow(&mut *caller, pages).is_err() {
            return None;
        }
    }
    Some(base as usize)
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
    let Some(rest) = url.strip_prefix("http://") else {
        println!("varta log: <unsupported URL scheme: only http:// is supported>");
        return None;
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
        println!("varta log: <unsupported URL scheme: only http:// is supported>");
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

    #[test]
    fn env_get_rejects_result_larger_than_limit() {
        unsafe {
            std::env::set_var(
                "VARTA_RUNTIME_LARGE_ENV",
                "A".repeat(MAX_HOST_RESULT_BYTES + 1),
            );
        }
        let source = r#"
export function main(): void {
  console.log(varta.env.get("VARTA_RUNTIME_LARGE_ENV"));
}
"#;
        let bytes = varta_compiler::compile_typescript(source).unwrap();
        let runtime = VartaRuntime::new();
        runtime
            .run_wasm_bytes(
                &bytes,
                &[varta_common::CapabilityGrant {
                    id: "env.get".to_string(),
                    value: varta_common::CapabilityValue::PathGlobs(vec![
                        "VARTA_RUNTIME_LARGE_ENV".to_string(),
                    ]),
                }],
            )
            .unwrap();
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

    #[test]
    fn fs_capability_rejects_traversal_and_symlink_escape() {
        use std::fs;
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join("varta-runtime-fs-cap-test");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("granted")).unwrap();
        let granted_file = root.join("granted").join("secret.txt");
        let outside_file = root.join("outside.txt");
        fs::write(&granted_file, "safe").unwrap();
        fs::write(&outside_file, "unsafe").unwrap();
        let escaped_link = root.join("granted").join("escape.txt");
        let _ = fs::remove_file(&escaped_link);
        symlink(&outside_file, &escaped_link).unwrap();

        let context = RuntimeContext {
            capabilities: vec![CapabilityGrant {
                id: "fs.read".to_string(),
                value: CapabilityValue::PathGlobs(vec![format!(
                    "{}/**",
                    root.join("granted").display()
                )]),
            }],
        };

        assert!(context.allows_fs_read(granted_file.to_str().unwrap()));
        assert!(!context.allows_fs_read(escaped_link.to_str().unwrap()));
        assert!(!context.allows_fs_read("/data/../secret.txt"));

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn env_capability_checks_are_operation_specific() {
        let has_only = RuntimeContext {
            capabilities: vec![CapabilityGrant {
                id: "env.has".to_string(),
                value: CapabilityValue::PathGlobs(vec!["PATH".to_string()]),
            }],
        };
        assert!(has_only.allows_env_access("env.has", "PATH"));
        assert!(!has_only.allows_env_access("env.print", "PATH"));
        assert!(!has_only.allows_env_access("env.get", "PATH"));

        let get_only = RuntimeContext {
            capabilities: vec![CapabilityGrant {
                id: "env.get".to_string(),
                value: CapabilityValue::PathGlobs(vec!["PATH".to_string()]),
            }],
        };
        assert!(get_only.allows_env_access("env.get", "PATH"));
        assert!(!get_only.allows_env_access("env.has", "PATH"));
        assert!(!get_only.allows_env_access("env.print", "PATH"));
    }

    #[test]
    fn net_capability_checks_are_operation_specific() {
        let fetch_only = RuntimeContext {
            capabilities: vec![CapabilityGrant {
                id: "net.fetch".to_string(),
                value: CapabilityValue::UrlGlobs(vec!["https://example.com/**".to_string()]),
            }],
        };
        assert!(fetch_only.allows_net_fetch("https://example.com/health"));
        assert!(!fetch_only.allows_net_get("https://example.com/health"));

        let get_only = RuntimeContext {
            capabilities: vec![CapabilityGrant {
                id: "net.get".to_string(),
                value: CapabilityValue::UrlGlobs(vec!["https://example.com/**".to_string()]),
            }],
        };
        assert!(get_only.allows_net_get("https://example.com/health"));
        assert!(!get_only.allows_net_fetch("https://example.com/health"));
    }

    #[test]
    fn fs_capability_checks_are_operation_specific() {
        let print_only = RuntimeContext {
            capabilities: vec![CapabilityGrant {
                id: "fs.print".to_string(),
                value: CapabilityValue::PathGlobs(vec!["/data/**".to_string()]),
            }],
        };
        assert!(print_only.allows_fs_print("/data/hello.txt"));
        assert!(!print_only.allows_fs_read("/data/hello.txt"));

        let read_only = RuntimeContext {
            capabilities: vec![CapabilityGrant {
                id: "fs.read".to_string(),
                value: CapabilityValue::PathGlobs(vec!["/data/**".to_string()]),
            }],
        };
        assert!(read_only.allows_fs_read("/data/hello.txt"));
        assert!(!read_only.allows_fs_print("/data/hello.txt"));
    }
}
