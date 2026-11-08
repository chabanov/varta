use varta_common::session;
#[path = "arena.rs"]
mod arena;
const INPUT_BYTES: usize = session::MAX_POLICY_BYTES;
static mut INPUT: [u8; INPUT_BYTES] = [0; INPUT_BYTES];
#[unsafe(no_mangle)]
pub extern "C" fn session_input() -> usize {
    arena::reset();
    core::ptr::addr_of_mut!(INPUT) as *mut u8 as usize
}
#[unsafe(no_mangle)]
pub extern "C" fn session_capacity() -> usize {
    INPUT_BYTES
}
#[unsafe(no_mangle)]
pub extern "C" fn session_check(len: usize) -> i32 {
    if len > INPUT_BYTES {
        return 0;
    }
    // SAFETY: the private fixed input allocation holds at least len bytes;
    // no reference survives this synchronous single-threaded host call.
    let input =
        unsafe { core::slice::from_raw_parts(core::ptr::addr_of!(INPUT) as *const u8, len) };
    let Ok(text) = core::str::from_utf8(input) else {
        return 0;
    };
    i32::from(session::descriptor(text))
}
#[panic_handler]
fn panic(_: &core::panic::PanicInfo<'_>) -> ! {
    core::arch::wasm32::unreachable()
}
