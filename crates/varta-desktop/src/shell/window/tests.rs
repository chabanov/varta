//! Headless tests of the window host: no window or event loop is opened.
use super::ime::translate_ime;
use crate::input_queue::PendingKey;
use crate::pixels::{WHITE_RGB, update_rgb};
use std::sync::Arc;
use winit::event::Ime;

fn headless_preview() -> (super::Host, std::sync::mpsc::Receiver<super::WorkerMessage>) {
    use super::*;
    let (sender, receiver) = mpsc::sync_channel(WORKER_QUEUE_CAPACITY);
    (
        Host {
            accessibility: None,
            accessibility_adapter: None,
            window: None,
            surface: None,
            #[cfg(feature = "gpu")]
            gpu: None,
            #[cfg(feature = "gpu")]
            gpu_events: None,
            dimensions: Arc::new(AtomicU64::new(0)),
            dark: Arc::new(AtomicBool::new(false)),
            scale: Arc::new(AtomicU64::new(1.0f64.to_bits())),
            sender,
            frame: None,
            error: None,
            presented: false,
            started: Instant::now(),
            frame_presented: false,
            frame_ack: Arc::new(AtomicU64::new(0)),
            cursor: None,
            pointer: Arc::new(Mutex::new(PointerState::default())),
            wake: Arc::new(wake::Wake::default()),
            gpu_active: Arc::new(AtomicBool::new(false)),
            pressed: None,
            pending_scroll: 0.0,
            scroll_inflight: None,
            modifiers: ModifiersState::empty(),
            pending_keys: PendingKeys::default(),
            key_inflight: None,
            window_focused: true,
            ime_binding: None,
            ime_composing: false,
            title: String::new(),
            default_title: "Varta test",
            label: "varta-test",
            console: super::Console {
                output: |_| {},
                diagnostic: |_| {},
            },
        },
        receiver,
    )
}
fn present_empty_frame(preview: &mut super::Host) {
    preview.frame = Some(super::PresentedFrame {
        clipboard_pending: false,
        accessibility: None,
        timing: None,
        scene: None,
        image: Arc::new(
            varta_view::View::default()
                .rasterize_boxes(16, 16)
                .unwrap()
                .into_metadata(),
        ),
        rgb: vec![WHITE_RGB; 16 * 16],
        revision: 1,
        dimensions: 16u64 << 32 | 16,
        sequence: 1,
        commits: 0,
        dark: false,
        title: String::new(),
        scale: 1.0f64.to_bits(),
    });
    preview.frame_presented = true;
}
#[test]
fn pointer_motion_is_coalesced_outside_the_activation_fifo_and_leave_clears_press() {
    let (mut preview, receiver) = headless_preview();
    for n in 0..10_000 {
        preview.cursor = Some((n as f32, 5.0));
        preview.update_pointer(Some(true));
    }
    assert_eq!(
        preview.pointer.lock().unwrap().position,
        Some((9999.0, 5.0))
    );
    assert!(receiver.try_recv().is_err());
    preview.cursor = None;
    preview.update_pointer(None);
    assert_eq!(
        *preview.pointer.lock().unwrap(),
        super::PointerState::default()
    );
}

#[test]
fn dirty_rgb_conversion_matches_full_conversion_with_alpha_and_resize() {
    let mut image = varta_view::View::default().rasterize_boxes(4, 4).unwrap();
    let mut rgb = Vec::new();
    update_rgb(&image, &mut rgb, None, true).unwrap();
    for (index, color) in [
        (0, [12, 30, 50, 255]),
        (5, [10, 20, 30, 64]),
        (15, [200, 100, 50, 255]),
    ] {
        image.pixels[index * 4..index * 4 + 4].copy_from_slice(&color);
        update_rgb(
            &image,
            &mut rgb,
            Some(varta_view::BoxRect {
                x: (index % 4) as f32,
                y: (index / 4) as f32,
                width: 1.0,
                height: 1.0,
            }),
            false,
        )
        .unwrap();
        let mut full = Vec::new();
        update_rgb(&image, &mut full, None, true).unwrap();
        assert_eq!(rgb, full);
    }
    assert_eq!(rgb[0], 0x000c_1e32);
    assert_eq!(rgb[5], 0x00c9_d3dd);
    let unchanged = rgb.clone();
    update_rgb(&image, &mut rgb, None, false).unwrap();
    assert_eq!(rgb, unchanged);
    let resized = varta_view::View::default().rasterize_boxes(2, 8).unwrap();
    update_rgb(&resized, &mut rgb, None, true).unwrap();
    assert!(rgb.iter().all(|c| *c == WHITE_RGB));
}

#[test]
fn immediate_typing_waits_for_the_pointer_focus_frame() {
    let (mut preview, receiver) = headless_preview();
    present_empty_frame(&mut preview);
    let frame = preview.frame.as_ref().unwrap();
    let pressed = super::Pressed {
        node: 7,
        revision: frame.revision,
        dimensions: frame.dimensions,
        text_anchor: None,
        select_hit: None,
        dark: frame.dark,
        scale: frame.scale,
    };
    preview
        .send_pointer(
            varta_view::Input::Click {
                target: varta_view::Target::Node(7),
            },
            pressed,
        )
        .unwrap();
    preview
        .enqueue_key(PendingKey::Character {
            key: "Ш".into(),
            text: "Ш".into(),
            modifiers: super::ModifiersState::empty(),
            select_all: false,
            history: None,
        })
        .unwrap();
    assert!(matches!(
        receiver.try_recv().unwrap(),
        super::WorkerMessage::Input {
            input: varta_view::Input::Click { .. },
            ..
        }
    ));
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(
        matches!(preview.pending_keys.pop_front(), Some(PendingKey::Character { text, .. }) if text == "Ш")
    );
}

#[test]
fn clipboard_fifo_waits_for_io_completion_before_following_keys() {
    let (mut preview, receiver) = headless_preview();
    present_empty_frame(&mut preview);
    preview
        .enqueue_key(PendingKey::Clipboard(varta_view::ClipboardKey {
            action: varta_view::ClipboardAction::Paste,
            key: "v".into(),
            shift: false,
            ctrl: false,
            alt: false,
            meta: true,
        }))
        .unwrap();
    assert!(matches!(
        receiver.try_recv().unwrap(),
        super::WorkerMessage::Clipboard { .. }
    ));
    preview
        .enqueue_key(PendingKey::Key("Tab", super::ModifiersState::empty()))
        .unwrap();
    preview.key_inflight = None;
    preview.frame.as_mut().unwrap().clipboard_pending = true;
    preview.flush_keys().unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    preview.frame.as_mut().unwrap().clipboard_pending = false;
    preview.frame.as_mut().unwrap().revision = 2;
    preview.flush_keys().unwrap();
    assert!(
        matches!(receiver.try_recv().unwrap(),super::WorkerMessage::Input { input:varta_view::Input::Key { key,.. },revision:2,.. } if key=="Tab")
    );
    assert!(preview.pending_keys.is_empty());
}

#[test]
fn blocked_preview_compacts_spare_capacity_without_losing_preedit_text() {
    let (mut preview, receiver) = headless_preview();
    let mut text = String::with_capacity(crate::input_queue::MAX_PENDING_TEXT_BYTES);
    text.push_str("old");
    let pending = translate_ime(
        Ime::Preedit(text, None),
        true,
        (7, 3),
        &mut preview.ime_composing,
    )
    .unwrap();
    preview.enqueue_key(pending).unwrap();
    let mut text = String::with_capacity(crate::input_queue::MAX_PENDING_TEXT_BYTES + 1);
    text.push_str("new");
    let pending = translate_ime(
        Ime::Preedit(text, None),
        true,
        (7, 3),
        &mut preview.ime_composing,
    )
    .unwrap();
    preview.enqueue_key(pending).unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(
        matches!(preview.pending_keys.pop_front(),Some(PendingKey::ImePreedit{text,..}) if text=="new" && text.capacity()==3)
    );
}
#[test]
fn preview_keeps_fifo_before_first_frame_and_sends_one_key_until_the_next_frame() {
    let (mut preview, receiver) = headless_preview();
    preview
        .enqueue_key(PendingKey::Key("Tab", super::ModifiersState::SHIFT))
        .unwrap();
    preview
        .enqueue_key(PendingKey::Key("Enter", super::ModifiersState::empty()))
        .unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    present_empty_frame(&mut preview);
    preview.flush_keys().unwrap();
    preview.flush_keys().unwrap();
    let super::WorkerMessage::Input { input, .. } = receiver.try_recv().unwrap() else {
        panic!("expected a keyboard input")
    };
    assert!(matches!(input,varta_view::Input::Key{key,shift:true,..} if key=="Tab"));
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    assert!(matches!(
        preview.pending_keys.pop_front(),
        Some(PendingKey::Key("Enter", _))
    ));
}
#[test]
fn stale_ime_binding_discard_releases_capacity_before_new_input_is_queued() {
    let (mut preview, receiver) = headless_preview();
    let text = "界".repeat(varta_common::view::DRAFT_MAX_VALUE_BYTES / 2);
    let count = crate::input_queue::MAX_PENDING_TEXT_BYTES / text.len();
    for _ in 0..count {
        preview
            .enqueue_key(PendingKey::ImeCommit {
                node: 7,
                commits: 3,
                text: text.clone(),
            })
            .unwrap();
    }
    present_empty_frame(&mut preview);
    preview.flush_keys().unwrap();
    assert!(preview.pending_keys.is_empty());
    assert!(matches!(
        receiver.try_recv(),
        Err(std::sync::mpsc::TryRecvError::Empty)
    ));
    preview.frame_presented = false;
    preview
        .enqueue_key(PendingKey::ImeCommit {
            node: 8,
            commits: 4,
            text,
        })
        .unwrap();
}

#[test]
fn ime_ignores_late_input_after_window_focus_loss() {
    let mut composing = false;
    assert!(translate_ime(Ime::Commit("好".into()), false, (7, 3), &mut composing).is_none());
    assert!(
        translate_ime(
            Ime::Preedit("好".into(), Some((3, 3))),
            false,
            (7, 3),
            &mut composing
        )
        .is_none()
    );
    assert!(!composing);
    assert!(matches!(
        translate_ime(Ime::Disabled, false, (7, 3), &mut composing),
        Some(PendingKey::ImePreedit { node: 7, commits: 3, text, cursor: None }) if text.is_empty()
    ));
}

#[test]
fn invalid_ime_input_cancels_composition_and_releases_keyboard() {
    let mut composing = false;
    assert!(matches!(
        translate_ime(
            Ime::Preedit("好".into(), Some((3, 3))),
            true,
            (7, 3),
            &mut composing
        ),
        Some(PendingKey::ImePreedit { .. })
    ));
    assert!(composing);
    assert!(matches!(
        translate_ime(Ime::Preedit("好".into(), Some((1, 1))), true, (7, 3), &mut composing),
        Some(PendingKey::ImePreedit { text, cursor: None, .. }) if text.is_empty()
    ));
    assert!(!composing);
    composing = true;
    assert!(matches!(
        translate_ime(Ime::Commit("\n".into()), true, (7, 3), &mut composing),
        Some(PendingKey::ImePreedit { text, cursor: None, .. }) if text.is_empty()
    ));
    assert!(!composing);
}
