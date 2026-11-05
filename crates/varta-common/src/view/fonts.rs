//! Explicit-font limits shared by every renderer and by the artifact's
//! resource section: the native text shaper (`varta-view`) loads at most
//! these faces and bytes, and packaging (`varta-artifact`) refuses a section
//! that exceeds them, so a packaged artifact always loads in the shell.

/// At most this many faces in one font collection.
pub const MAX_FONT_FACES: usize = 16;
/// The largest single face.
pub const MAX_FONT_FACE_BYTES: usize = 4 * 1024 * 1024;
/// The largest total of font bytes in one collection.
pub const MAX_FONT_BYTES: usize = 16 * 1024 * 1024;
/// The longest family name, or generic/named alias, a renderer registers
/// (bytes). Packaged family names are shorter still.
pub const MAX_FONT_FAMILY_BYTES: usize = 256;
/// At most this many aliases (generic or named families) map to one face.
pub const MAX_FONT_ALIASES: usize = 16;

/// Elements whose browser user-agent family is the generic `monospace`
/// (`pre`, `code`; Chrome's other monospace elements are outside the view
/// subset). The native user-agent table gives exactly these rows that
/// family, and with packaged faces the browser host maps them to the
/// family that declares `monospace`, as the native shaper resolves them.
pub const UA_MONOSPACE_TAGS: &[super::Tag] = &[super::Tag::Pre, super::Tag::Code];
/// Form controls whose browser user-agent style sets a font of its own (a
/// control font family; `textarea` the generic `monospace`). The native
/// renderer gives them the inherited family, so with packaged faces the
/// browser host makes their family inherit too.
pub const CONTROL_FONT_TAGS: &[super::Tag] = &[
    super::Tag::Button,
    super::Tag::Input,
    super::Tag::Textarea,
    super::Tag::Select,
];
