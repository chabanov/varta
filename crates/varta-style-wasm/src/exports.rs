//! The exported calls of the style session (`varta_style::session`).
use alloc::{vec, vec::Vec};
use core::cell::UnsafeCell;
use varta_common::view::Tag;
use varta_common::view::stylesheet::REM_PIXELS;
use varta_style::session::{self, Session, status};

use crate::fonts;

/// The one session of this instance, its fonts and its input and output
/// buffers.
struct State {
    session: Option<Session>,
    fonts: Option<fonts::Fonts>,
    input: Vec<u8>,
    output: Vec<u8>,
}

struct Cell(UnsafeCell<State>);
// SAFETY: wasm32 code here is single-threaded and no export re-enters
// another, so at most one mutable borrow of the cell exists at a time.
unsafe impl Sync for Cell {}

static STATE: Cell = Cell(UnsafeCell::new(State {
    session: None,
    fonts: None,
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
    let input = core::str::from_utf8(&state.input).ok();
    let session = state.session.get_or_insert_with(new_session);
    f(session, input)
}

/// A session with the bridge's measurements for the browser's fonts.
fn new_session() -> Session {
    let mut session = Session::new(session::environment(
        (1024.0, 768.0),
        REM_PIXELS,
        REM_PIXELS,
        0,
    ));
    session.set_fonts(alloc::sync::Arc::new(fonts::Fonts::new().set));
    session
}

/// The state's session.
fn session_of(state: &mut State) -> &mut Session {
    state.session.get_or_insert_with(new_session)
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
pub extern "C" fn view_engine_create(id: u32, tag: u32) -> u32 {
    // `TEXT_NODE` creates a text node; anything else is an element tag code.
    let tag = if tag == TEXT_NODE {
        None
    } else {
        match u16::try_from(tag).ok().and_then(Tag::from_code) {
            Some(tag) => Some(tag),
            None => return session::status::MISUSE,
        }
    };
    with(|session, _| session.create(id, tag))
}

/// The tag argument of [`view_engine_create`] that creates a text node.
const TEXT_NODE: u32 = u32::MAX;

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

/// Set an attribute: the input holds its name (`name_len` UTF-8 bytes)
/// followed by its value.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_attribute(id: u32, name_len: u32) -> u32 {
    with(|session, text| {
        let Some((name, value)) = text.and_then(|text| text.split_at_checked(name_len as usize))
        else {
            return status::MISUSE;
        };
        session.set_attribute(id, name, value)
    })
}

/// Remove the attribute the input names.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_unattribute(id: u32) -> u32 {
    with(|session, text| match text {
        Some(name) => session.remove_attribute(id, name),
        None => status::MISUSE,
    })
}

/// A form control's value or a text node's data: the input.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_value(id: u32) -> u32 {
    with(|session, text| match text {
        Some(value) => session.set_value(id, value),
        None => status::MISUSE,
    })
}

/// The element state bits of `mask` (`varta_style::ElementState`).
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_states(id: u32, bits: u32, mask: u32) -> u32 {
    with(|session, _| session.set_states(id, bits, mask))
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
pub extern "C" fn view_engine_environment(
    width: f64,
    height: f64,
    root: f64,
    medium: f64,
    flags: u32,
) {
    with(|session, _| {
        session.set_environment(session::environment((width, height), root, medium, flags));
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn view_engine_state(focus_visible: u32, hovered: u32, active: u32) {
    with(|session, _| session.set_state(focus_visible, hovered, active));
}

/// The focused element (`:focus`, `:focus-within`), or 0.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_focus(focus: u32) {
    with(|session, _| session.set_focus(focus));
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

/// The committed computed values of pseudo-element `kind` (its index in
/// `PseudoElement::ALL`) of element `id` as output bytes; returns their
/// length (0 for an unknown kind).
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_computed_pseudo(id: u32, kind: u32) -> u32 {
    let Some(kind) = usize::try_from(kind)
        .ok()
        .and_then(|index| varta_style::PseudoElement::ALL.get(index).copied())
    else {
        return 0;
    };
    let bytes = with(|session, _| session::encode_computed(&session.computed_pseudo(id, kind)));
    let state = state();
    state.output = bytes;
    u32::try_from(state.output.len()).unwrap_or(0)
}

/// The pseudo-elements the committed sheets select: bit `i` for
/// `PseudoElement::ALL[i]`.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_pseudo_elements() -> u32 {
    with(|session, _| session.selected_pseudo_elements())
}

/// Add a packaged face: the input holds the family name (`family_len`
/// UTF-8 bytes) followed by the font file; `generics` has the bit of each
/// generic family it stands for (`Generic::ALL` order). A file the engine
/// cannot read is measured by the bridge under its family name.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_face(family_len: u32, generics: u32) -> u32 {
    let state = state();
    let Some((family, data)) = state.input.split_at_checked(family_len as usize) else {
        return status::MISUSE;
    };
    let Ok(family) = core::str::from_utf8(family) else {
        return status::MISUSE;
    };
    let fonts = state.fonts.get_or_insert_with(fonts::Fonts::new);
    let aliases = fonts::generics(generics);
    let added = match fonts.set.add_file(family, &aliases, data) {
        Ok(_) => Ok(()),
        Err(varta_style::fonts::FaceError::Unreadable) => {
            fonts.set.add_host_family(family, &aliases)
        }
        Err(error) => Err(error),
    };
    if added.is_err() {
        return status::INVALID;
    }
    publish_fonts(state);
    status::OK
}

/// Set the default family of text without `font-family` to the input's
/// UTF-8 name, or with an empty input to the browser's default font.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_default_family() -> u32 {
    let state = state();
    let Ok(family) = core::str::from_utf8(&state.input) else {
        return status::MISUSE;
    };
    let family = (!family.is_empty()).then(|| alloc::string::String::from(family));
    state
        .fonts
        .get_or_insert_with(fonts::Fonts::new)
        .set
        .set_default_family(family);
    publish_fonts(state);
    status::OK
}

/// Give the session the current font set.
fn publish_fonts(state: &mut State) {
    let set = state
        .fonts
        .get_or_insert_with(fonts::Fonts::new)
        .set
        .clone();
    session_of(state).set_fonts(alloc::sync::Arc::new(set));
}

/// Admitted keyframes only: the browser consumes the engine's CSSOM payload.
#[unsafe(no_mangle)]
pub extern "C" fn view_engine_animation_css() -> u32 {
    let state = state();
    state.output = session_of(state).animation_css().into_bytes();
    state.output.len() as u32
}
