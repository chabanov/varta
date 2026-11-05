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
