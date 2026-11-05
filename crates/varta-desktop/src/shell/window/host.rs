//! Window-side state transitions: presentation, accessibility publication
//! and the bounded FIFO of input sent to the worker.
use super::{Host, Pressed, WorkerMessage, accessibility, timing};
use crate::input_queue::PendingKey;
use std::num::NonZeroU32;
use std::sync::{atomic::Ordering, mpsc};
use std::time::Instant;
use varta_view::{Input, SelectHit, Target};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    window::{CursorIcon, Window},
};

impl Host {
    pub(super) fn publish_accessibility(&mut self) {
        if !self.frame_presented {
            return;
        }
        let (Some(bridge), Some(adapter), Some(frame)) = (
            &self.accessibility,
            &mut self.accessibility_adapter,
            &self.frame,
        ) else {
            return;
        };
        let Some(tree) = &frame.accessibility else {
            return;
        };
        if let Some(update) = bridge.publish(
            frame.sequence,
            tree,
            &frame.image,
            accessibility::Version {
                revision: frame.revision,
                dimensions: frame.dimensions,
                dark: frame.dark,
                scale: frame.scale,
            },
            &frame.title,
        ) {
            adapter.update_if_active(|| update);
        }
    }
    pub(super) fn flush_accessibility(&mut self) -> Result<(), String> {
        if !self.frame_presented
            || self.key_inflight.is_some()
            || self.scroll_inflight.is_some()
            || !self.pending_keys.is_empty()
        {
            return Ok(());
        }
        let (Some(bridge), Some(frame)) = (&self.accessibility, &self.frame) else {
            return Ok(());
        };
        let version = accessibility::Version {
            revision: frame.revision,
            dimensions: frame.dimensions,
            dark: frame.dark,
            scale: frame.scale,
        };
        if let Some(pending) = bridge.take_current(version) {
            match self.sender.try_send(WorkerMessage::Accessibility(pending)) {
                Ok(()) => {
                    self.key_inflight = Some((frame.revision, frame.dimensions));
                    self.wake.notify();
                }
                Err(mpsc::TrySendError::Full(_)) => {}
                Err(mpsc::TrySendError::Disconnected(_)) => {
                    return Err("application stopped".into());
                }
            }
        }
        Ok(())
    }
    /// The GPU backend did not initialize. With an explicit `gpu` renderer
    /// that is an error (the window closes and [`super::run`] returns
    /// `ShellError::GpuUnavailable`); with `auto` the CPU presents instead.
    #[cfg(feature = "gpu")]
    pub(super) fn gpu_unavailable(&mut self, reason: String) -> Result<(), String> {
        if self.gpu_required {
            let error = format!("GPU renderer unavailable: {reason}");
            self.gpu_failure = Some(reason);
            return Err(error);
        }
        self.console
            .report(format_args!("GPU unavailable ({reason}), using CPU"));
        self.gpu_active.store(false, Ordering::Release);
        Ok(())
    }
    #[cfg(feature = "gpu")]
    pub(super) fn switch_to_cpu(&mut self) -> Result<(), String> {
        self.console.report("switching GPU to CPU fallback");
        self.gpu.take();
        self.gpu_active.store(false, Ordering::Release);
        let window = self.window.as_ref().ok_or("window unavailable")?.clone();
        let context = softbuffer::Context::new(window.clone()).map_err(|e| e.to_string())?;
        self.surface = Some(softbuffer::Surface::new(&context, window).map_err(|e| e.to_string())?);
        self.wake.notify();
        Ok(())
    }
    pub(super) fn update_pointer(&self, down: Option<bool>) {
        let mut state = self.pointer.lock().unwrap_or_else(|e| e.into_inner());
        let before = (state.position, state.down);
        state.position = self.cursor;
        if let Some(down) = down {
            state.down = down;
        }
        if state.position.is_none() {
            state.down = false;
        }
        if before != (state.position, state.down) {
            state.trace = timing::InputStamp::now("pointer-state");
            self.wake.notify();
        }
    }
    pub(super) fn update_cursor(&self) {
        if let Some(window) = &self.window {
            let hint = self
                .cursor
                .and_then(|(x, y)| self.frame.as_ref().map(|f| f.image.cursor_at(x, y)))
                .unwrap_or_default();
            window.set_cursor(match hint {
                varta_view::PointerCursor::Default => CursorIcon::Default,
                varta_view::PointerCursor::Pointer => CursorIcon::Pointer,
                varta_view::PointerCursor::Text => CursorIcon::Text,
                varta_view::PointerCursor::NotAllowed => CursorIcon::NotAllowed,
            });
        }
    }
    pub(super) fn size_changed(&self, window: &Window) {
        let size = window.inner_size();
        self.dimensions.store(
            (u64::from(size.width) << 32) | u64::from(size.height),
            Ordering::Release,
        );
        self.wake.notify();
    }
    pub(super) fn draw(&mut self) -> Result<(), String> {
        #[cfg(feature = "gpu")]
        if self.gpu.is_some() && !self.gpu_active.load(Ordering::Acquire) {
            self.switch_to_cpu()?;
        }
        let (Some(window), Some(frame)) = (&self.window, &self.frame) else {
            return Ok(());
        };
        let size = window.inner_size();
        if size.width != frame.image.width
            || size.height != frame.image.height
            || frame.dark != self.dark.load(Ordering::Acquire)
            || frame.scale != self.scale.load(Ordering::Acquire)
        {
            self.frame_presented = false;
            self.frame_ack.store(frame.sequence, Ordering::Release);
            self.wake.notify();
            return Ok(());
        }
        let Some(width) = NonZeroU32::new(size.width) else {
            return Ok(());
        };
        let Some(height) = NonZeroU32::new(size.height) else {
            return Ok(());
        };
        let copy_started = Instant::now();
        let first_present = self.frame_ack.load(Ordering::Acquire) < frame.sequence;
        #[cfg(feature = "gpu")]
        if let (Some(gpu), Some(scene)) = (&mut self.gpu, &frame.scene) {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                gpu.draw(scene, size.width, size.height)
            }));
            match result {
                Ok(Ok(true)) => {}
                Ok(Ok(false)) => {
                    return Ok(());
                }
                _ => {
                    let sequence = frame.sequence;
                    self.switch_to_cpu()?;
                    self.frame_presented = false;
                    self.frame_ack.store(sequence, Ordering::Release);
                    self.wake.notify();
                    return Ok(());
                }
            }
        } else {
            if frame.scene.is_some() {
                self.frame_presented = false;
                self.frame_ack.store(frame.sequence, Ordering::Release);
                self.wake.notify();
                return Ok(());
            }
            let surface = self.surface.as_mut().ok_or("CPU surface unavailable")?;
            surface.resize(width, height).map_err(|e| e.to_string())?;
            let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
            if buffer.len() != frame.rgb.len() {
                return Err("frame presentation dimensions differ".into());
            }
            buffer.copy_from_slice(&frame.rgb);
            buffer.present().map_err(|e| e.to_string())?;
        }
        #[cfg(not(feature = "gpu"))]
        {
            if frame.scene.is_some() {
                return Err("GPU scene requires the GPU backend".into());
            }
            let surface = self.surface.as_mut().ok_or("CPU surface unavailable")?;
            surface.resize(width, height).map_err(|e| e.to_string())?;
            let mut buffer = surface.buffer_mut().map_err(|e| e.to_string())?;
            if buffer.len() != frame.rgb.len() {
                return Err("frame presentation dimensions differ".into());
            }
            buffer.copy_from_slice(&frame.rgb);
            buffer.present().map_err(|e| e.to_string())?;
        }
        if first_present && let Some(timing) = frame.timing {
            (self.console.diagnostic)(&timing.report(
                frame.sequence,
                if frame.scene.is_some() { "gpu" } else { "cpu" },
                copy_started,
                Instant::now(),
            ));
        }
        let title = if frame.title.is_empty() {
            self.default_title
        } else {
            &frame.title
        };
        if self.title != title {
            window.set_title(title);
            self.title = title.into();
        }
        self.frame_presented = true;
        self.frame_ack.store(frame.sequence, Ordering::Release);
        self.wake.notify();
        self.update_cursor();
        let binding = frame.image.editable().map(|node| (node, frame.commits));
        if self.ime_binding != binding {
            if self.ime_binding.is_some() {
                window.set_ime_allowed(false);
            }
            self.ime_binding = binding;
            self.ime_composing = false;
            self.pending_keys.retain(|pending| match pending {
                PendingKey::ImePreedit { node, commits, .. }
                | PendingKey::ImeCommit { node, commits, .. } => Some((*node, *commits)) == binding,
                _ => true,
            });
        }
        window.set_ime_allowed(self.window_focused && binding.is_some());
        if let Some(caret) = frame.image.ime_cursor_area() {
            window.set_ime_cursor_area(
                PhysicalPosition::new(caret.x as f64, caret.y as f64),
                PhysicalSize::new(caret.width.max(1.0) as f64, caret.height.max(1.0) as f64),
            );
        }
        if !self.presented {
            self.presented = true;
            self.console.report("first frame presented");
            if timing::InputStamp::now("startup").is_some() {
                (self.console.diagnostic)(&format!(
                    "varta-ui-startup: {{\"boundary\":\"host-entry-to-first-present-return\",\"backend\":\"{}\",\"total_ms\":{:.3}}}",
                    if frame.scene.is_some() { "gpu" } else { "cpu" },
                    self.started.elapsed().as_secs_f64() * 1000.0,
                ));
            }
        }
        if self.scroll_inflight.is_some_and(|(revision, dimensions)| {
            revision != frame.revision || dimensions != frame.dimensions
        }) {
            self.scroll_inflight = None;
        }
        if self.key_inflight.is_some_and(|(revision, dimensions)| {
            revision != frame.revision || dimensions != frame.dimensions
        }) {
            self.key_inflight = None;
        }
        self.publish_accessibility();
        self.flush_keys()?;
        self.flush_accessibility()?;
        self.flush_scroll()
    }
    pub(super) fn hit(&self) -> Option<(u32, u64, u64)> {
        let (x, y) = self.cursor?;
        let frame = self.frame.as_ref().filter(|_| self.frame_presented)?;
        Some((
            frame
                .image
                .select_hit(x, y)
                .map(SelectHit::target)
                .or_else(|| frame.image.hit_test(x, y))?,
            frame.revision,
            frame.dimensions,
        ))
    }
    pub(super) fn flush_scroll(&mut self) -> Result<(), String> {
        if self.scroll_inflight.is_some()
            || self.key_inflight.is_some()
            || !self.pending_keys.is_empty()
            || !self.frame_presented
            || self.pending_scroll == 0.0
        {
            return Ok(());
        }
        let Some((x, y)) = self.cursor else {
            self.pending_scroll = 0.0;
            return Ok(());
        };
        let Some(frame) = &self.frame else {
            return Ok(());
        };
        let delta = std::mem::take(&mut self.pending_scroll);
        if let Some(menu) = frame.image.select_menu() {
            let rect = menu.viewport;
            if x < rect.x || y < rect.y || x >= rect.x + rect.width || y >= rect.y + rect.height {
                return Ok(());
            }
            let rows = (delta.abs() / menu.row_height).ceil() as usize;
            let first = if delta < 0.0 {
                menu.first.saturating_sub(rows)
            } else {
                menu.first.saturating_add(rows).min(menu.max_first)
            };
            if first != menu.first {
                self.sender
                    .try_send(WorkerMessage::Input {
                        trace: timing::InputStamp::now("select-scroll"),
                        input: Input::SelectScroll {
                            target: Target::Node(menu.target),
                            first,
                        },
                        revision: frame.revision,
                        dimensions: frame.dimensions,
                        dark: frame.dark,
                        scale: frame.scale,
                    })
                    .map_err(|_| {
                        "native input queue is full or the application stopped".to_owned()
                    })?;
                self.wake.notify();
                self.scroll_inflight = Some((frame.revision, frame.dimensions));
            }
            return Ok(());
        }
        if let Some((node, top, remaining)) = frame.image.scroll_target(x, y, delta) {
            let metrics = frame
                .image
                .scroll_metrics(node)
                .ok_or("scroll target is missing frame metrics")?;
            self.sender
                .try_send(WorkerMessage::Input {
                    trace: timing::InputStamp::now("scroll"),
                    input: Input::ScrollMeasured {
                        target: Target::Node(node),
                        top,
                        scroll_height: f64::from(metrics.viewport.height) + f64::from(metrics.max),
                        client_height: f64::from(metrics.viewport.height),
                    },
                    revision: frame.revision,
                    dimensions: frame.dimensions,
                    dark: frame.dark,
                    scale: frame.scale,
                })
                .map_err(|_| "native input queue is full or the application stopped".to_owned())?;
            self.wake.notify();
            self.pending_scroll = remaining;
            self.scroll_inflight = Some((frame.revision, frame.dimensions));
        }
        Ok(())
    }
    pub(super) fn enqueue_key(&mut self, pending: PendingKey) -> Result<(), String> {
        self.pending_keys.push(pending).map_err(str::to_owned)?;
        self.flush_keys()
    }
    pub(super) fn send_pointer(&mut self, input: Input, pressed: Pressed) -> Result<(), String> {
        self.sender
            .try_send(WorkerMessage::Input {
                input,
                trace: timing::InputStamp::now("pointer"),
                revision: pressed.revision,
                dimensions: pressed.dimensions,
                dark: pressed.dark,
                scale: pressed.scale,
            })
            .map_err(|_| "native input queue is full or the application stopped".to_owned())?;
        self.wake.notify();
        // A click can change focus. Keep immediately following characters in
        // FIFO order until its resulting frame supplies the new focused target.
        self.key_inflight = Some((pressed.revision, pressed.dimensions));
        Ok(())
    }
    pub(super) fn flush_keys(&mut self) -> Result<(), String> {
        if self.key_inflight.is_some() || self.scroll_inflight.is_some() || !self.frame_presented {
            return Ok(());
        }
        let Some(frame) = &self.frame else {
            return Ok(());
        };
        if frame.clipboard_pending {
            return Ok(());
        }
        while let Some(pending) = self.pending_keys.pop_front() {
            let input = match pending {
                PendingKey::Clipboard(key) => {
                    self.sender
                        .try_send(WorkerMessage::Clipboard {
                            key,
                            trace: timing::InputStamp::now("clipboard"),
                            version: accessibility::Version {
                                revision: frame.revision,
                                dimensions: frame.dimensions,
                                dark: frame.dark,
                                scale: frame.scale,
                            },
                        })
                        .map_err(|_| {
                            "native input queue is full or the application stopped".to_owned()
                        })?;
                    self.wake.notify();
                    self.key_inflight = Some((frame.revision, frame.dimensions));
                    break;
                }
                PendingKey::SelectClose(node) => {
                    if frame
                        .image
                        .select_menu()
                        .is_none_or(|menu| menu.target != node)
                    {
                        continue;
                    }
                    Input::SelectClose {
                        target: Target::Node(node),
                    }
                }
                PendingKey::Character {
                    key,
                    text,
                    modifiers,
                    select_all,
                    history,
                } => {
                    let Some(target) = frame.image.focused() else {
                        continue;
                    };
                    Input::TextKey {
                        target: Target::Node(target),
                        key,
                        text,
                        shift: modifiers.shift_key(),
                        ctrl: modifiers.control_key(),
                        alt: modifiers.alt_key(),
                        meta: modifiers.super_key(),
                        select_all,
                        history,
                    }
                }
                PendingKey::Key(key, modifiers) => {
                    let target = frame
                        .image
                        .focused()
                        .or_else(|| (key == "Tab").then(varta_view::View::root));
                    let Some(target) = target else {
                        continue;
                    };
                    if let Some(movement) = frame.image.text_navigation(target, key).filter(|_| {
                        !modifiers.control_key() && !modifiers.alt_key() && !modifiers.super_key()
                    }) {
                        Input::TextNavigate {
                            target: Target::Node(target),
                            key: key.into(),
                            position: movement.position,
                            preferred_x: movement.preferred_x,
                            upstream: movement.upstream,
                            shift: modifiers.shift_key(),
                        }
                    } else {
                        Input::Key {
                            target: Target::Node(target),
                            key: key.into(),
                            shift: modifiers.shift_key(),
                            ctrl: modifiers.control_key(),
                            alt: modifiers.alt_key(),
                            meta: modifiers.super_key(),
                            composing: false,
                        }
                    }
                }
                PendingKey::ImePreedit {
                    node,
                    commits,
                    text,
                    cursor,
                } => {
                    if frame.image.editable() != Some(node) || frame.commits != commits {
                        continue;
                    }
                    Input::ImePreedit {
                        target: Target::Node(node),
                        text,
                        cursor,
                    }
                }
                PendingKey::ImeCommit {
                    node,
                    commits,
                    text,
                } => {
                    if frame.image.editable() != Some(node) || frame.commits != commits {
                        continue;
                    }
                    Input::ImeCommit {
                        target: Target::Node(node),
                        text,
                    }
                }
            };
            self.sender
                .try_send(WorkerMessage::Input {
                    input,
                    trace: timing::InputStamp::now("key"),
                    revision: frame.revision,
                    dimensions: frame.dimensions,
                    dark: frame.dark,
                    scale: frame.scale,
                })
                .map_err(|_| "native input queue is full or the application stopped".to_owned())?;
            self.wake.notify();
            self.key_inflight = Some((frame.revision, frame.dimensions));
            break;
        }
        Ok(())
    }
}
