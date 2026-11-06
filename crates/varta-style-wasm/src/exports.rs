//! The exported calls of the style session (`varta_style::session`).
use alloc::{vec, vec::Vec};
use core::cell::UnsafeCell;
use varta_common::view::stylesheet::REM_PIXELS;
use varta_style::session::{self, Session, status};

/// The one session of this instance and its input and output buffers.
struct State {
    session: Option<Session>,
    input: Vec<u8>,
    output: Vec<u8>,
}

struct Cell(UnsafeCell<State>);
// SAFETY: wasm32 code here is single-threaded and no export re-enters
// another, so at most one mutable borrow of the cell exists at a time.
unsafe impl Sync for Cell {}

static STATE: Cell = Cell(UnsafeCell::new(State {
    session: None,
    input: Vec::new(),
    output: Vec::new(),
}));

/// The instance state for the duration of one export.
fn state() -> &'static mut State {
    // SAFETY: see `Cell`: exports run one at a time and each takes this
    // borrow once, for its own duration.
    unsafe { &mut *STATE.0.get() }
}

/// Run `f` on the session and the current input text (`None` when the
/// input is not UTF-8), with one borrow of the state.
fn with<R>(f: impl FnOnce(&mut Session, Option<&str>) -> R) -> R {
    let state = state();
    let session = state
        .session
        .get_or_insert_with(|| Session::new(session::environment(1024.0, 768.0, REM_PIXELS, 0)));
    f(session, core::str::from_utf8(&state.input).ok())
}

/// Reserve a `len`-byte input buffer; returns its address.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_input(len: u32) -> u32 {
    let state = state();
    state.input = vec![0u8; len as usize];
    state.input.as_mut_ptr() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_output() -> u32 {
    state().output.as_ptr() as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_output_length() -> u32 {
    u32::try_from(state().output.len()).unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_begin() {
    with(|session, _| session.begin());
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_commit() -> u32 {
    with(|session, _| session.commit())
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_abort() {
    with(|session, _| session.abort());
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_create(id: u32, element: u32) -> u32 {
    with(|session, _| session.create(id, element != 0))
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_insert(parent: u32, child: u32, before: u32) -> u32 {
    with(|session, _| session.insert(parent, child, before))
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_remove(id: u32) -> u32 {
    with(|session, _| session.remove(id))
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_class(id: u32) -> u32 {
    with(|session, text| match text {
        Some(text) => session.set_class(id, text),
        None => status::MISUSE,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_style(id: u32, property: u32) -> u32 {
    with(|session, text| match (text, u16::try_from(property)) {
        (Some(text), Ok(property)) => session.set_style(id, property, text),
        _ => status::MISUSE,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_unstyle(id: u32, property: u32) -> u32 {
    with(|session, _| match u16::try_from(property) {
        Ok(property) => session.remove_style(id, property),
        Err(_) => status::MISUSE,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_flags(id: u32, disabled: u32, checked: u32) -> u32 {
    with(|session, _| session.set_flags(id, disabled != 0, checked != 0))
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_sheet(id: u32) -> u32 {
    with(|session, text| match text {
        Some(text) => session.stylesheet(id, text),
        None => status::MISUSE,
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_charge() -> u32 {
    with(|session, _| session.charge())
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_environment(width: f64, height: f64, root: f64, flags: u32) {
    with(|session, _| session.set_environment(session::environment(width, height, root, flags)));
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_state(focus: u32, hovered: u32, active: u32) {
    with(|session, _| session.set_state(focus, hovered, active));
}

/// The committed computed values of `id` as output bytes
/// (`session::encode_computed`); returns their length.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_computed(id: u32) -> u32 {
    let bytes = with(|session, _| session::encode_computed(&session.computed(id)));
    let state = state();
    state.output = bytes;
    u32::try_from(state.output.len()).unwrap_or(0)
}
