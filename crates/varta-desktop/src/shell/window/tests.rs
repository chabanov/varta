//! Headless tests of the window host: no window or event loop is opened.
use super::ime::translate_ime;
use crate::input_queue::PendingKey;
use crate::pixels::{WHITE_RGB, update_rgb};
use std::sync::Arc;
use std::time::Instant;
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
            preferences: Arc::new(AtomicU32::new(0)),
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
            #[cfg(feature = "gpu")]
            gpu_required: false,
            #[cfg(feature = "gpu")]
            gpu_failure: None,
            console: super::Console::SILENT,
            prompt: None,
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

#[test]
fn an_explicit_gpu_renderer_needs_a_gpu_build() {
    use crate::shell::{ShellError, args::Renderer};
    assert!(super::check_renderer(Renderer::Cpu).is_ok());
    // `auto` presents with the CPU when there is no GPU backend.
    assert!(super::check_renderer(Renderer::Auto).is_ok());
    let gpu = super::check_renderer(Renderer::Gpu);
    if cfg!(feature = "gpu") {
        assert!(gpu.is_ok());
    } else {
        assert!(matches!(gpu, Err(ShellError::GpuNotCompiled)));
    }
}

#[cfg(feature = "gpu")]
#[test]
fn a_required_gpu_that_fails_to_initialize_is_an_error_and_auto_falls_back() {
    use std::sync::atomic::Ordering;
    const REASON: &str = "no adapter";
    let (mut host, _receiver) = headless_preview();
    host.gpu_active.store(true, Ordering::Release);
    host.gpu_unavailable(REASON.into()).unwrap();
    assert!(!host.gpu_active.load(Ordering::Acquire));
    assert_eq!(host.gpu_failure, None);

    let (mut host, _receiver) = headless_preview();
    host.gpu_required = true;
    host.gpu_active.store(true, Ordering::Release);
    let error = host.gpu_unavailable(REASON.into()).unwrap_err();
    assert!(error.contains(REASON), "{error}");
    assert_eq!(host.gpu_failure.as_deref(), Some(REASON));
}

/// A broker that grants nothing, for a layout-only guest view.
struct NoGrants;
impl varta_view::Broker for NoGrants {
    fn link_requested(&self, _: &str) -> bool {
        false
    }
    fn open_url(&mut self, _: &str) -> varta_view::Access {
        varta_view::Access::Denied
    }
    fn clipboard_write(&mut self) -> varta_view::Access {
        varta_view::Access::Denied
    }
}

/// A presented guest frame whose topmost fixed element covers the whole
/// `size`x`size` viewport, as a guest imitating or covering a prompt would.
fn present_covering_frame(host: &mut super::Host, size: u32) -> u32 {
    use varta_common::view::{CssProperty as P, Tag};
    const COVER: u32 = 2;
    let mut batch = varta_view::BatchWriter::new();
    batch
        .create_element(COVER, Tag::Div)
        .insert(1, COVER, 0)
        .set_style(COVER, P::Position, "fixed")
        .set_style(COVER, P::ZIndex, "2147483647")
        .set_style(COVER, P::Top, "0px")
        .set_style(COVER, P::Left, "0px")
        .set_style(COVER, P::Width, "100%")
        .set_style(COVER, P::Height, "100%")
        .set_style(COVER, P::BackgroundColor, "#f00");
    let mut view = varta_view::View::default();
    view.commit(&batch.finish(), false, &mut NoGrants).unwrap();
    let image = view.rasterize_boxes(size, size).unwrap();
    host.frame = Some(super::PresentedFrame {
        clipboard_pending: false,
        accessibility: None,
        timing: None,
        scene: None,
        rgb: vec![WHITE_RGB; (size * size) as usize],
        image: Arc::new(image.into_metadata()),
        revision: 1,
        dimensions: u64::from(size) << 32 | u64::from(size),
        sequence: 1,
        commits: 1,
        dark: false,
        title: String::new(),
        scale: 1.0f64.to_bits(),
    });
    host.frame_presented = true;
    COVER
}

fn permission_prompt(required: bool) -> varta_common::permission::PermissionPrompt {
    varta_common::permission::PermissionPrompt {
        artifact: "sha256:00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into(),
        capability: "net.request".into(),
        scope: vec!["https://api.test/**".into()],
        purpose: Some("[ Allow ]".into()),
        required,
    }
}

/// A layer that accepts input at once (shown and presented one input
/// delay ago).
fn ready_layer(required: bool) -> crate::shell::consent::PromptLayer {
    let now = Instant::now();
    let shown = now
        .checked_sub(crate::shell::consent::PROMPT_INPUT_DELAY)
        .unwrap_or(now);
    let mut layer =
        crate::shell::consent::PromptLayer::new(&permission_prompt(required), shown).unwrap();
    let (width, height) = layer.logical_size();
    assert!(layer.presented(width, height, shown));
    layer
}

fn mouse(state: winit::event::ElementState) -> winit::event::WindowEvent {
    winit::event::WindowEvent::MouseInput {
        device_id: winit::event::DeviceId::dummy(),
        state,
        button: winit::event::MouseButton::Left,
    }
}
fn cursor(x: f32, y: f32) -> winit::event::WindowEvent {
    winit::event::WindowEvent::CursorMoved {
        device_id: winit::event::DeviceId::dummy(),
        position: winit::dpi::PhysicalPosition::new(f64::from(x), f64::from(y)),
    }
}

/// A physical point inside the button answering `answer`, at factor 1.
fn button_point(
    layer: &crate::shell::consent::PromptLayer,
    answer: varta_common::permission::PromptAnswer,
) -> (f32, f32) {
    let (width, height) = layer.logical_size();
    (0..height)
        .flat_map(|y| (0..width).map(move |x| (x as f32, y as f32)))
        .find(|(x, y)| layer.button_at(*x, *y, width, height) == Some(answer))
        .map(|(x, y)| (x + 4.0, y + 4.0))
        .unwrap()
}

#[test]
fn guest_content_can_neither_cover_nor_hit_an_open_prompt() {
    use varta_common::permission::PromptAnswer;
    use winit::event::{ElementState, Ime, MouseScrollDelta, TouchPhase, WindowEvent};
    let (mut host, receiver) = headless_preview();
    let layer = ready_layer(false);
    let (width, height) = layer.logical_size();
    let cover = present_covering_frame(&mut host, width.max(height));
    // Without a prompt, the guest's topmost element takes every point,
    // including where the prompt's Allow button is drawn.
    let (allow_x, allow_y) = button_point(&layer, PromptAnswer::Allow);
    host.cursor = Some((allow_x, allow_y));
    assert_eq!(host.hit().map(|(node, ..)| node), Some(cover));
    host.pressed = Some(super::Pressed {
        node: cover,
        revision: 1,
        dimensions: host.frame.as_ref().unwrap().dimensions,
        dark: false,
        scale: 1.0f64.to_bits(),
        text_anchor: None,
        select_hit: None,
    });
    let pixels = layer.render(width, height);
    let (answer, answers) = std::sync::mpsc::sync_channel(1);
    let open = host.begin_prompt(layer, answer).unwrap();
    host.prompt = Some(open);
    // A press begun in the application's window never completes, and the
    // guest's pointer leaves.
    assert!(host.pressed.is_none() && host.cursor.is_none());
    assert_eq!(host.pointer.lock().unwrap().position, None);
    // User input of the application's window no longer reaches the guest;
    // window state still does.
    for event in [
        mouse(ElementState::Pressed),
        mouse(ElementState::Released),
        cursor(allow_x, allow_y),
        WindowEvent::MouseWheel {
            device_id: winit::event::DeviceId::dummy(),
            delta: MouseScrollDelta::LineDelta(0.0, 1.0),
            phase: TouchPhase::Moved,
        },
        WindowEvent::Ime(Ime::Commit("x".into())),
    ] {
        assert!(!host.admits(&event), "{event:?}");
    }
    for event in [
        WindowEvent::RedrawRequested,
        WindowEvent::CloseRequested,
        WindowEvent::Focused(true),
        WindowEvent::Resized(winit::dpi::PhysicalSize::new(width, height)),
    ] {
        assert!(host.admits(&event), "{event:?}");
    }
    // The prompt's own pixels come only from the broker's prompt: the
    // guest's covering frame is not an input of the layer.
    assert_eq!(
        host.prompt.as_ref().unwrap().layer.render(width, height),
        pixels
    );
    // A click in the prompt window answers the prompt, never the guest.
    host.prompt_event(cursor(allow_x, allow_y)).unwrap();
    host.prompt_event(mouse(ElementState::Pressed)).unwrap();
    host.prompt_event(mouse(ElementState::Released)).unwrap();
    assert_eq!(answers.try_recv(), Ok(PromptAnswer::Allow));
    assert!(host.prompt.is_none());
    assert!(receiver.try_recv().is_err(), "nothing reached the worker");
    assert!(host.admits(&mouse(ElementState::Pressed)));
}

#[test]
fn one_prompt_at_a_time_and_closing_it_dismisses() {
    use varta_common::permission::{MAX_OPEN_PROMPTS, PromptAnswer};
    use winit::event::{ElementState, WindowEvent};
    assert_eq!(MAX_OPEN_PROMPTS, 1);
    let (mut host, _receiver) = headless_preview();
    let (first, first_answers) = std::sync::mpsc::sync_channel(1);
    host.prompt = host.begin_prompt(ready_layer(true), first);
    assert!(host.prompt.is_some());
    // A second request while one is open is dismissed at once.
    let (second, second_answers) = std::sync::mpsc::sync_channel(1);
    assert!(host.begin_prompt(ready_layer(false), second).is_none());
    assert_eq!(second_answers.try_recv(), Ok(PromptAnswer::Dismiss));
    assert!(first_answers.try_recv().is_err());
    // The prompt window's close button dismisses.
    host.prompt_event(WindowEvent::CloseRequested).unwrap();
    assert_eq!(first_answers.try_recv(), Ok(PromptAnswer::Dismiss));
    assert!(host.prompt.is_none());
    // A click within the input delay after the prompt appears never
    // answers it.
    let (third, third_answers) = std::sync::mpsc::sync_channel(1);
    let fresh =
        crate::shell::consent::PromptLayer::new(&permission_prompt(false), Instant::now()).unwrap();
    let (x, y) = button_point(&fresh, PromptAnswer::Allow);
    host.prompt = host.begin_prompt(fresh, third);
    host.prompt_event(cursor(x, y)).unwrap();
    host.prompt_event(mouse(ElementState::Pressed)).unwrap();
    host.prompt_event(mouse(ElementState::Released)).unwrap();
    assert!(third_answers.try_recv().is_err());
    assert!(host.prompt.is_some());
}

#[test]
fn prompt_keys_move_focus_activate_and_cancel() {
    use crate::shell::consent::PromptKey;
    use winit::keyboard::{Key, ModifiersState, NamedKey};
    let none = ModifiersState::empty();
    for (key, modifiers, expected) in [
        (Key::Named(NamedKey::Tab), none, Some(PromptKey::Next)),
        (
            Key::Named(NamedKey::Tab),
            ModifiersState::SHIFT,
            Some(PromptKey::Previous),
        ),
        (
            Key::Named(NamedKey::ArrowRight),
            none,
            Some(PromptKey::Next),
        ),
        (
            Key::Named(NamedKey::ArrowLeft),
            none,
            Some(PromptKey::Previous),
        ),
        (Key::Named(NamedKey::Enter), none, Some(PromptKey::Activate)),
        (Key::Character(" ".into()), none, Some(PromptKey::Activate)),
        (Key::Named(NamedKey::Escape), none, Some(PromptKey::Cancel)),
        (
            Key::Named(NamedKey::PageDown),
            none,
            Some(PromptKey::NextPage),
        ),
        (
            Key::Named(NamedKey::PageUp),
            none,
            Some(PromptKey::PreviousPage),
        ),
        (Key::Character("a".into()), none, None),
    ] {
        assert_eq!(
            super::prompt::prompt_key(&key, modifiers),
            expected,
            "{key:?}"
        );
    }
}

#[test]
fn a_paged_prompt_answers_allow_only_after_every_page_was_shown() {
    use crate::shell::consent::{PROMPT_INPUT_DELAY, PromptKey, PromptLayer};
    use varta_common::permission::PromptAnswer;
    use winit::event::ElementState;
    let (mut host, _receiver) = headless_preview();
    let now = Instant::now();
    let shown = now.checked_sub(PROMPT_INPUT_DELAY * 2).unwrap_or(now);
    let url = format!("https://\u{43f}.test/{}", "\u{441}/".repeat(200));
    let request = varta_common::permission::PermissionPrompt {
        capability: varta_common::view::OPEN_URL_CAPABILITY.into(),
        scope: vec![url],
        purpose: None,
        ..permission_prompt(false)
    };
    let layer = PromptLayer::new(&request, shown).unwrap();
    let (_, pages) = layer.page();
    assert!(pages > 1);
    let (x, y) = button_point(&layer, PromptAnswer::Allow);
    let (answer, answers) = std::sync::mpsc::sync_channel(1);
    host.prompt = host.begin_prompt(layer, answer);
    // A click on Allow before every page was shown answers nothing and
    // leaves the prompt open.
    let click = |host: &mut super::Host| {
        host.prompt_event(cursor(x, y)).unwrap();
        host.prompt_event(mouse(ElementState::Pressed)).unwrap();
        host.prompt_event(mouse(ElementState::Released)).unwrap();
    };
    click(&mut host);
    assert!(answers.try_recv().is_err());
    assert!(host.prompt.is_some());
    // Every page selected but only the last one presented: still no.
    let turned = now.checked_sub(PROMPT_INPUT_DELAY).unwrap_or(now);
    let (width, height) = host.prompt.as_ref().unwrap().layer.logical_size();
    for _ in 1..pages {
        let layer = &mut host.prompt.as_mut().unwrap().layer;
        assert_eq!(layer.key(PromptKey::NextPage, turned), None);
    }
    host.prompt_presented(width, height, turned);
    click(&mut host);
    assert!(answers.try_recv().is_err());
    // Every page presented, the last one an input delay ago: Allow answers.
    for _ in 0..pages {
        let layer = &mut host.prompt.as_mut().unwrap().layer;
        assert_eq!(layer.key(PromptKey::NextPage, turned), None);
        host.prompt_presented(width, height, turned);
    }
    assert_eq!(
        host.prompt.as_ref().unwrap().layer.page(),
        (pages - 1, pages)
    );
    click(&mut host);
    assert_eq!(answers.try_recv(), Ok(PromptAnswer::Allow));
    assert!(host.prompt.is_none());
}

/// Host diagnostics of the tests that record them.
static DIAGNOSTICS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
const RECORDING: super::Console = super::Console {
    label: "varta-test",
    output: |_| {},
    diagnostic: |line| DIAGNOSTICS.lock().unwrap().push(line.to_owned()),
};
/// The recorded diagnostics that mention `text`.
fn diagnostics(text: &str) -> Vec<String> {
    DIAGNOSTICS
        .lock()
        .unwrap()
        .iter()
        .filter(|line| line.contains(text))
        .cloned()
        .collect()
}

#[test]
fn a_request_that_cannot_be_shown_in_full_is_denied_and_reported_by_the_window() {
    use crate::shell::consent::MAX_RESOURCE_LINES;
    use varta_common::permission::PromptAnswer;
    let (mut host, _receiver) = headless_preview();
    host.console = RECORDING;
    // A link URL at the network URL limit needs more lines than a prompt
    // shows.
    let url = format!(
        "https://a.test/{}",
        "p".repeat(varta_common::net::MAX_URL_BYTES - "https://a.test/".len())
    );
    let capability = varta_common::view::OPEN_URL_CAPABILITY;
    let (answer, answers) = std::sync::mpsc::sync_channel(1);
    let request = super::prompt::PromptRequest {
        prompt: varta_common::permission::PermissionPrompt {
            capability: capability.into(),
            scope: vec![url],
            purpose: None,
            ..permission_prompt(false)
        },
        answer,
    };
    assert!(host.admit_prompt(request, Instant::now()).is_none());
    assert_eq!(answers.try_recv(), Ok(PromptAnswer::Deny));
    assert!(host.prompt.is_none());
    let reported = diagnostics(capability);
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(
        reported[0]
            .starts_with("varta-test: denied the ui.open_url permission request without asking")
    );
    assert!(reported[0].contains(&format!("at most {MAX_RESOURCE_LINES}")));
    // A request that fits opens the prompt state.
    let (answer, answers) = std::sync::mpsc::sync_channel(1);
    let request = super::prompt::PromptRequest {
        prompt: permission_prompt(false),
        answer,
    };
    assert!(host.admit_prompt(request, Instant::now()).is_some());
    assert!(answers.try_recv().is_err());
}

#[test]
fn a_clipped_prompt_frame_never_counts_as_shown_and_is_reported_once() {
    use crate::shell::consent::{PROMPT_INPUT_DELAY, PromptLayer};
    use varta_common::permission::PromptAnswer;
    let (mut host, _receiver) = headless_preview();
    host.console = RECORDING;
    let now = Instant::now();
    let shown = now.checked_sub(PROMPT_INPUT_DELAY * 2).unwrap_or(now);
    let request = varta_common::permission::PermissionPrompt {
        capability: "kv.read".into(),
        ..permission_prompt(false)
    };
    let layer = PromptLayer::new(&request, shown).unwrap();
    let (width, height) = layer.logical_size();
    let (answer, answers) = std::sync::mpsc::sync_channel(1);
    host.prompt = host.begin_prompt(layer, answer);
    let short = height - 1;
    for _ in 0..2 {
        host.prompt_presented(width, short, shown);
    }
    let reported = diagnostics(&format!("its window has {width}x{short}"));
    assert_eq!(reported.len(), 1, "{reported:?}");
    let layer = &host.prompt.as_ref().unwrap().layer;
    assert!(!layer.allow_ready(now));
    // Keyboard Allow is out of reach; Escape still dismisses.
    let layer = &mut host.prompt.as_mut().unwrap().layer;
    for _ in 0..4 {
        layer.key(crate::shell::consent::PromptKey::Next, now);
        assert_ne!(layer.focused(), Some(PromptAnswer::Allow));
    }
    assert_eq!(
        layer.key(crate::shell::consent::PromptKey::Cancel, now),
        Some(PromptAnswer::Dismiss)
    );
    // Presented whole, the page counts.
    host.prompt_presented(width, height, shown);
    assert!(host.prompt.as_ref().unwrap().layer.allow_ready(now));
    assert!(answers.try_recv().is_err());
}

#[test]
fn the_guest_cannot_title_its_window_like_a_prompt() {
    use super::prompt::{PROMPT_TITLE, guest_title};
    assert_eq!(guest_title("Notes"), "Notes");
    for spoof in [
        PROMPT_TITLE.to_owned(),
        PROMPT_TITLE.to_ascii_uppercase(),
        format!("  {PROMPT_TITLE} "),
        format!("Notes - {}", PROMPT_TITLE.to_ascii_lowercase()),
    ] {
        assert_eq!(guest_title(&spoof), "", "{spoof}");
    }
}

#[test]
fn system_preferences_round_trip_through_the_worker_word() {
    use varta_common::view::stylesheet::media::{Contrast, Preferences};
    for reduced_motion in [false, true] {
        for contrast in [
            Contrast::NoPreference,
            Contrast::More,
            Contrast::Less,
            Contrast::Custom,
        ] {
            let preferences = Preferences {
                reduced_motion,
                contrast,
                ..Preferences::default()
            };
            assert_eq!(
                super::preferences_of(super::preferences_bits(preferences)),
                preferences
            );
        }
    }
    // Off the main thread nothing is read: the defaults of a desktop.
    let read = std::thread::spawn(super::system_preferences)
        .join()
        .unwrap();
    assert_eq!(read, Preferences::default());
}
