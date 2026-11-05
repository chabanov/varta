//! Platform IME events mapped to bounded pending keys for the bound field.
use crate::input_queue::PendingKey;
use winit::event::Ime;

fn valid_ime_text(text: &str) -> bool {
    let max_units = varta_common::view::DRAFT_MAX_VALUE_BYTES / 2;
    text.len() <= max_units * 3
        && !text.chars().any(char::is_control)
        && text.encode_utf16().take(max_units + 1).count() <= max_units
}

pub(super) fn translate_ime(
    ime: Ime,
    focused: bool,
    binding: (u32, usize),
    composing: &mut bool,
) -> Option<PendingKey> {
    let (node, commits) = binding;
    let cancel = || PendingKey::ImePreedit {
        node,
        commits,
        text: String::new(),
        cursor: None,
    };
    match ime {
        Ime::Enabled => None,
        Ime::Disabled => {
            *composing = false;
            Some(cancel())
        }
        Ime::Preedit(_, _) | Ime::Commit(_) if !focused => None,
        Ime::Preedit(text, cursor) => {
            if !valid_ime_text(&text)
                || cursor.is_some_and(|(start, end)| {
                    start > end
                        || end > text.len()
                        || !text.is_char_boundary(start)
                        || !text.is_char_boundary(end)
                })
            {
                *composing = false;
                return Some(cancel());
            }
            *composing = !text.is_empty();
            Some(PendingKey::ImePreedit {
                node,
                commits,
                text,
                cursor,
            })
        }
        Ime::Commit(text) => {
            *composing = false;
            if !valid_ime_text(&text) {
                return Some(cancel());
            }
            Some(PendingKey::ImeCommit {
                node,
                commits,
                text,
            })
        }
    }
}
