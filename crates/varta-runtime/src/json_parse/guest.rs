//! The native instance as the shared value layer sees it
//! (`varta_kv_codec::guest::Guest`): Wasmtime memory, the module's
//! allocation and retention helpers (looked up at the first allocation, as
//! [`heap::Writer`] does), the exception channel and fair fuel. The v38 KV
//! decode and checked JSON's recoverable errors build through it.
use super::*;
use heap::Writer;
use varta_common::exceptions as channel;

pub(crate) struct Host<'a, 'b> {
    caller: &'a mut Caller<'b, RuntimeContext>,
    memory: wasmtime::Memory,
    writer: Option<Writer>,
}

impl<'a, 'b> Host<'a, 'b> {
    pub(crate) fn new(
        caller: &'a mut Caller<'b, RuntimeContext>,
        memory: wasmtime::Memory,
    ) -> Self {
        Self {
            caller,
            memory,
            writer: None,
        }
    }
    /// Look the allocation and retention helpers up once, at the first
    /// allocation.
    fn ensure_writer(&mut self) -> Result<(), wasmtime::Error> {
        if self.writer.is_none() {
            self.writer = Some(Writer::new(self.caller, self.memory)?);
        }
        Ok(())
    }
    /// The module's bounds.
    pub(crate) fn module(&self) -> ModuleBounds {
        self.caller.data().module
    }
}

fn global(caller: &mut Caller<'_, RuntimeContext>, name: &str) -> Option<wasmtime::Global> {
    match caller.get_export(name) {
        Some(Extern::Global(global)) => Some(global),
        _ => None,
    }
}

impl varta_common::json::build::Heap for Host<'_, '_> {
    type Error = wasmtime::Error;
    fn allocate(&mut self, len: usize) -> Result<usize, wasmtime::Error> {
        self.ensure_writer()?;
        let Self { caller, writer, .. } = self;
        writer
            .as_ref()
            .ok_or_else(|| error("missing allocation helper"))?
            .block(caller, len)
    }
    fn write(&mut self, at: usize, bytes: &[u8]) -> Result<(), wasmtime::Error> {
        self.memory
            .write(&mut *self.caller, at, bytes)
            .map_err(wasmtime::Error::new)
    }
    fn zero(&mut self, at: usize, len: usize) -> Result<(), wasmtime::Error> {
        let data = self.memory.data_mut(&mut *self.caller);
        at.checked_add(len)
            .and_then(|end| data.get_mut(at..end))
            .ok_or_else(|| error("memory range"))?
            .fill(0);
        Ok(())
    }
    fn retain(&mut self, handle: u64) -> Result<(), wasmtime::Error> {
        self.ensure_writer()?;
        let Self { caller, writer, .. } = self;
        writer
            .as_ref()
            .ok_or_else(|| error("missing retention helper"))?
            .retain_handle(caller, handle)
    }
}

impl varta_kv_codec::guest::Guest for Host<'_, '_> {
    type Memory = [u8];
    fn memory(&self) -> &[u8] {
        self.memory.data(&*self.caller)
    }
    fn ready(&mut self) -> Result<(), wasmtime::Error> {
        exceptions::ready(self.caller)
    }
    fn deadline(&mut self) -> Result<(), wasmtime::Error> {
        exceptions::deadline(self.caller)
    }
    fn charge(&mut self, fuel: u64) -> Result<(), wasmtime::Error> {
        crate::fair_fuel::charge(self.caller, fuel)
    }
    fn throw(&mut self, cell: u64) -> Result<(), wasmtime::Error> {
        let invalid =
            || wasmtime::Error::new(ValueAbiError("noncanonical JSON exception channel".into()));
        let value = global(self.caller, channel::VALUE).ok_or_else(invalid)?;
        let pending = global(self.caller, channel::PENDING).ok_or_else(invalid)?;
        let length = global(self.caller, abi::RESULT_LEN).ok_or_else(invalid)?;
        length.set(&mut *self.caller, Val::I32(0))?;
        self.memory
            .write(&mut *self.caller, channel::ROOT, &cell.to_le_bytes())?;
        value.set(&mut *self.caller, Val::I64(cell as i64))?;
        pending.set(&mut *self.caller, Val::I32(1))?;
        Ok(())
    }
    fn result_length(&mut self, high: i32) -> Result<(), wasmtime::Error> {
        let Some(length) = global(self.caller, abi::RESULT_LEN) else {
            return Err(error("missing result length"));
        };
        length.set(&mut *self.caller, Val::I32(high))
    }
    fn dispose(&mut self, value: JsonValue) -> Result<(), wasmtime::Error> {
        // A deep tree is dropped on the JSON host stack, never by host
        // recursion here.
        if nesting(&value) > DataLimits::LEGACY.json_depth {
            crate::json::deep(move || drop(value))?;
        }
        Ok(())
    }
    fn json_fatal(&self, message: String) -> wasmtime::Error {
        error(message)
    }
    fn kv_fatal(&self, message: &'static str) -> wasmtime::Error {
        wasmtime::Error::msg(format!("kv: {message}"))
    }
    fn abi_fatal(&self, error: ValueAbiError) -> wasmtime::Error {
        wasmtime::Error::new(error)
    }
    fn deadline_error(&self) -> wasmtime::Error {
        self.caller.data().deadline_error()
    }
}

impl varta_kv_codec::build::AsyncHeap for Host<'_, '_> {
    async fn allocate_async(&mut self, len: usize) -> Result<usize, Self::Error> {
        self.ensure_writer()?;
        let Self { caller, writer, .. } = self;
        writer
            .as_ref()
            .ok_or_else(|| error("missing allocation helper"))?
            .block_async(caller, len)
            .await
    }
    async fn retain_async(&mut self, handle: u64) -> Result<(), Self::Error> {
        self.ensure_writer()?;
        let Self { caller, writer, .. } = self;
        writer
            .as_ref()
            .ok_or_else(|| error("missing retention helper"))?
            .retain_handle_async(caller, handle)
            .await
    }
}
