//! The Worker's guest as the shared value layer sees it: guest memory, the
//! module's allocation and retention helpers, the exception channel, the
//! turn deadline and the fuel tank, each one `varta_kv_host` import
//! (`varta_common::kv::codec`). The Worker checks every guest range again.
use alloc::{format, string::String, string::ToString};
use varta_common::{json::JsonValue, memory::Memory, value::ValueAbiError};

#[link(wasm_import_module = "varta_kv_host")]
unsafe extern "C" {
    fn memory_size() -> u32;
    fn memory_read(at: u32, into: *mut u8, len: u32) -> u32;
    fn memory_write(at: u32, from: *const u8, len: u32) -> u32;
    fn memory_zero(at: u32, len: u32) -> u32;
    fn allocate(len: u32) -> u32;
    fn retain(handle: u64) -> u64;
    fn deadline_passed() -> u32;
    fn charge(fuel: u64);
    fn channel_ready() -> u32;
    fn channel_throw(cell: u64) -> u32;
    fn result_length(high: i32) -> u32;
}

/// Why a call stopped: a trap message, or the turn deadline.
pub enum Failure {
    Fatal(String),
    Deadline,
}

/// The exception channel's own failure text.
fn channel() -> Failure {
    Failure::Fatal(ValueAbiError("noncanonical JSON exception channel".into()).to_string())
}

fn json(message: &str) -> Failure {
    Failure::Fatal(format!("checked JSON: {message}"))
}

/// Guest memory 0, read through the Worker.
pub struct GuestMemory;

impl Memory for GuestMemory {
    fn size(&self) -> usize {
        // SAFETY: a host import without memory arguments.
        unsafe { memory_size() as usize }
    }
    fn copy_to(&self, at: usize, out: &mut [u8]) -> bool {
        if at
            .checked_add(out.len())
            .is_none_or(|end| end > self.size())
        {
            return false;
        }
        // SAFETY: `out` is a live, writable range of this module's memory.
        unsafe { memory_read(at as u32, out.as_mut_ptr(), out.len() as u32) != 0 }
    }
}

/// The turn deadline, polled through the Worker.
pub struct Turn;

impl varta_kv_codec::Deadline for Turn {
    fn passed(&self) -> bool {
        // SAFETY: a host import without memory arguments.
        unsafe { deadline_passed() != 0 }
    }
}

/// Charge fair fuel; the Worker traps when the tank runs out.
pub fn charge_fuel(fuel: u64) {
    // SAFETY: a host import without memory arguments.
    unsafe { charge(fuel) }
}

/// The guest a decode builds into.
pub struct Guest {
    memory: GuestMemory,
}

impl Guest {
    pub fn new() -> Self {
        Self {
            memory: GuestMemory,
        }
    }
}

impl varta_common::json::build::Heap for Guest {
    type Error = Failure;
    fn allocate(&mut self, len: usize) -> Result<usize, Failure> {
        if varta_kv_codec::Deadline::passed(&Turn) {
            return Err(json("execution deadline"));
        }
        let request = i32::try_from(len).map_err(|_| json("allocation size"))?;
        // SAFETY: a host import without memory arguments.
        let ptr = unsafe { allocate(request as u32) } as usize;
        varta_kv_codec::build::check_block(&self.memory, ptr, len).map_err(json)?;
        Ok(ptr)
    }
    fn write(&mut self, at: usize, bytes: &[u8]) -> Result<(), Failure> {
        // SAFETY: `bytes` is a live range of this module's memory.
        match unsafe { memory_write(at as u32, bytes.as_ptr(), bytes.len() as u32) } {
            0 => Err(json("memory range")),
            _ => Ok(()),
        }
    }
    fn zero(&mut self, at: usize, len: usize) -> Result<(), Failure> {
        // SAFETY: a host import without memory arguments.
        match unsafe { memory_zero(at as u32, len as u32) } {
            0 => Err(json("memory range")),
            _ => Ok(()),
        }
    }
    fn retain(&mut self, handle: u64) -> Result<(), Failure> {
        // SAFETY: a host import without memory arguments.
        if unsafe { retain(handle) } != handle {
            return Err(json("retention helper changed the value"));
        }
        Ok(())
    }
}

impl varta_kv_codec::guest::Guest for Guest {
    type Memory = GuestMemory;
    fn memory(&self) -> &GuestMemory {
        &self.memory
    }
    fn ready(&mut self) -> Result<(), Failure> {
        // SAFETY: a host import without memory arguments.
        match unsafe { channel_ready() } {
            0 => Err(channel()),
            _ => Ok(()),
        }
    }
    fn deadline(&mut self) -> Result<(), Failure> {
        if varta_kv_codec::Deadline::passed(&Turn) {
            return Err(json("execution deadline"));
        }
        Ok(())
    }
    fn charge(&mut self, fuel: u64) -> Result<(), Failure> {
        charge_fuel(fuel);
        Ok(())
    }
    fn throw(&mut self, cell: u64) -> Result<(), Failure> {
        // SAFETY: a host import without memory arguments.
        match unsafe { channel_throw(cell) } {
            0 => Err(channel()),
            _ => Ok(()),
        }
    }
    fn result_length(&mut self, high: i32) -> Result<(), Failure> {
        // SAFETY: a host import without memory arguments.
        match unsafe { result_length(high) } {
            0 => Err(json("missing result length")),
            _ => Ok(()),
        }
    }
    fn dispose(&mut self, value: JsonValue) -> Result<(), Failure> {
        drop(value);
        Ok(())
    }
    fn json_fatal(&self, message: String) -> Failure {
        json(&message)
    }
    fn kv_fatal(&self, message: &'static str) -> Failure {
        Failure::Fatal(format!("kv: {message}"))
    }
    fn abi_fatal(&self, error: ValueAbiError) -> Failure {
        Failure::Fatal(error.to_string())
    }
    fn deadline_error(&self) -> Failure {
        Failure::Deadline
    }
}
