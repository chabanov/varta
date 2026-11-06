//! The v38 L0 view protocol (`view_protocol()`): the `view_commit` import,
//! the view capabilities decided by permission broker v1 and the delivery of
//! view event records.
//!
//! [`Session`] owns the instance's headless renderer and the
//! commit-per-turn rule: each turn may commit at most one batch. The records
//! the renderer produces (from a committed batch or from a trusted platform
//! input) leave it at once and become host tasks of the instance
//! (`events::HostEvents::push_view`): every record runs as one event turn of
//! an application instance with fresh per-turn budgets, its gesture flag
//! makes a user-gesture turn (the broker's `gesture`), and coalescible kinds
//! follow the task queue's coalescing rule. The record reaches the guest in
//! its binary form through the synthesized export [`EVENT_EXPORT`]. Platform
//! inputs are host work between turns ([`input`]), never a turn themselves.
use crate::events::HostEvents;
use crate::permission::{Broker as PermissionBroker, ViewPrompts};
use crate::{RuntimeContext, RuntimeError, policy::Policy};
use varta_common::{HostOperation, abi, view};
use varta_view::{
    Access, Broker, Environment, EventRecord, Input, InputError, Platform, View, ViewError,
};
use wasmtime::{Caller, Extern, Instance, Linker, Store};

pub(crate) use view::EVENT_EXPORT;

/// One instance's view state.
#[derive(Default)]
pub(crate) struct Session {
    view: View,
    layout: Option<crate::ViewLayout>,
    /// A batch was committed in the current turn.
    committed: bool,
    /// The platform is simulated (a scripted headless session): it answers
    /// the prompts the broker asks for when the instance has no prompt seam.
    simulated: bool,
    /// An application instance hosts the view and delivers its records; a
    /// bounded execution renders but has no event delivery, so its records
    /// are discarded.
    hosted: bool,
}

impl Session {
    /// The view of an instance whose embedder renders it headlessly with a
    /// simulated `platform`.
    pub(crate) fn simulated(environment: Environment, platform: Platform) -> Self {
        Self {
            view: View::new(environment, platform),
            simulated: true,
            hosted: true,
            ..Self::default()
        }
    }
    /// The view of an application instance on a host with its own prompt
    /// seam (or none) and, when given, its real clipboard and system
    /// browser for granted guest effects.
    pub(crate) fn hosted(effects: Option<std::sync::Arc<dyn varta_view::PlatformEffects>>) -> Self {
        Self {
            view: match effects {
                Some(effects) => View::default().with_platform_effects(effects),
                None => View::default(),
            },
            hosted: true,
            ..Self::default()
        }
    }
    /// Whether a CSS transition is running.
    pub(crate) fn animating(&self) -> bool {
        self.view.animating()
    }
    /// A new turn starts: its commit allowance.
    pub(crate) fn begin_turn(&mut self) {
        self.committed = false;
    }
    pub(crate) fn snapshot(&self) -> serde_json::Value {
        self.view.snapshot()
    }
    pub(crate) fn snapshot_with_geometry(
        &self,
        request: &mut varta_view::GeometryRequest,
    ) -> Result<serde_json::Value, varta_view::LayoutError> {
        self.view.snapshot_with_geometry(request)
    }
    pub(crate) fn accessibility_tree(
        &self,
    ) -> Result<varta_view::AccessibilityTree, varta_view::AccessibilityError> {
        self.view.accessibility_tree()
    }
    pub(crate) fn clipboard_target(&self) -> Option<varta_view::ClipboardTarget> {
        self.view.clipboard_target()
    }
    pub(crate) fn clipboard_pending(&self) -> bool {
        self.view.clipboard_pending()
    }
    pub(crate) fn title(&self) -> &str {
        self.view.title()
    }
    pub(crate) fn commits(&self) -> usize {
        self.view.commits()
    }
    pub(crate) fn set_environment(
        &mut self,
        environment: Environment,
    ) -> Result<(), varta_view::LayoutError> {
        self.view.set_environment(environment)
    }
    pub(crate) fn set_layout(
        &mut self,
        layout: crate::ViewLayout,
    ) -> Result<(), varta_view::LayoutError> {
        let mut environment = self.view.environment();
        environment.width = f64::from(layout.width);
        if let Some(dark) = layout.dark {
            environment.dark = dark;
        }
        environment.scrollbars = layout.scrollbars;
        if let Some(size) = layout.root_font_size {
            environment.root_font_size = size;
        }
        self.view.set_environment(environment)?;
        self.layout = Some(layout);
        Ok(())
    }
    pub(crate) fn set_layout_size(
        &mut self,
        width: f32,
        height: f32,
    ) -> Result<(), varta_view::LayoutError> {
        let Some(layout) = self.layout.as_mut() else {
            return Err(varta_view::LayoutError::Viewport);
        };
        crate::ViewLayout::validate(width, height, layout.font_size)?;
        let mut environment = self.view.environment();
        environment.width = f64::from(width);
        self.view.set_environment(environment)?;
        layout.width = width;
        layout.height = height;
        Ok(())
    }
    pub(crate) fn cancel_preedit(&mut self) {
        self.view.cancel_preedit();
    }
    pub(crate) fn set_pointer_state(&mut self, hovered: Option<u32>, active: Option<u32>) -> bool {
        self.view.set_pointer_state(hovered, active)
    }
    pub(crate) fn rasterize(
        &self,
        width: u32,
        height: u32,
        font: Option<(&mut varta_view::TextMeasurer, f32)>,
    ) -> Result<varta_view::RasterImage, varta_view::PaintError> {
        match font {
            Some((measurer, size)) => self.view.rasterize_with_text(width, height, measurer, size),
            None => self.view.rasterize_boxes(width, height),
        }
    }
    pub(crate) fn rasterize_scaled(
        &self,
        width: u32,
        height: u32,
        scale: f32,
        font: Option<(&mut varta_view::TextMeasurer, f32)>,
    ) -> Result<varta_view::RasterImage, varta_view::PaintError> {
        self.view.rasterize_scaled(width, height, scale, font)
    }
    pub(crate) fn render(
        &self,
        renderer: &mut varta_view::Renderer,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<varta_view::RasterImage, varta_view::PaintError> {
        renderer.render(&self.view, width, height, scale)
    }
    pub(crate) fn prepare(
        &self,
        renderer: &mut varta_view::Renderer,
        width: u32,
        height: u32,
        scale: f32,
    ) -> Result<varta_view::PreparedFrame, varta_view::PaintError> {
        renderer.prepare(&self.view, width, height, scale)
    }
    /// Whether a committed view can still receive platform inputs (and so
    /// records): it keeps an application instance alive like an open event
    /// source.
    pub(crate) fn attached(&self) -> bool {
        self.view.commits() > 0
    }
    /// Move the renderer's records into the instance's task queue.
    pub(crate) fn queue(&mut self, events: &mut HostEvents) -> Result<(), ViewError> {
        for delivery in self.view.drain() {
            if self.hosted {
                events.push_view(delivery)?;
            } else {
                // Undelivered: the record leaves the renderer as delivered.
                self.view.delivered(&delivery.record);
            }
        }
        Ok(())
    }

    pub(crate) fn scroll_into_view(
        &mut self,
        target: u32,
        width: f32,
        height: f32,
        font: Option<(&mut varta_view::TextMeasurer, f32)>,
    ) -> Result<usize, varta_view::ScrollIntoViewError> {
        self.view.scroll_into_view(target, width, height, font)
    }
}

/// The view capabilities, decided by permission broker v1: the package
/// request within the host ceiling, broker states (revocation included),
/// the audit log and, for `ui.open_url`, the prompt rule.
struct ViewBroker<'a> {
    broker: &'a mut PermissionBroker,
    policy: &'a Policy,
    prompts: ViewPrompts,
}
impl Broker for ViewBroker<'_> {
    fn link_requested(&self, url: &str) -> bool {
        self.policy.link_requested(url)
    }
    fn open_url(&mut self, url: &str) -> Access {
        self.broker.open_url(self.policy, url, self.prompts)
    }
    fn clipboard_write(&mut self) -> Access {
        self.broker.clipboard_write(self.policy)
    }
    fn prompted(&mut self, operation: view::ResultOperation, resource: &str, allowed: bool) {
        let id = match operation {
            view::ResultOperation::OpenUrl => view::OPEN_URL_CAPABILITY,
            view::ResultOperation::CopyText => view::CLIPBOARD_WRITE_CAPABILITY,
        };
        self.broker.prompted(id, resource, allowed);
    }
}

fn fatal(error: ViewError) -> wasmtime::Error {
    wasmtime::Error::new(error)
}

pub(crate) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        varta_common::abi::HOST_MODULE,
        HostOperation::ViewCommit.name(),
        |mut caller: Caller<'_, RuntimeContext>,
         ptr: i32,
         len: i32|
         -> Result<i32, wasmtime::Error> {
            caller.data_mut().tick()?;
            let bytes = batch(&mut caller, ptr, len)?;
            // A commit is a style change event at the instance's time.
            sync_clock(caller.data_mut());
            let RuntimeContext {
                view,
                broker,
                policy,
                events,
                ..
            } = caller.data_mut();
            if view.committed {
                return Err(fatal(ViewError::SecondCommit));
            }
            let gesture = broker.gesture;
            // Guest code is running: nothing may prompt now.
            let simulated = view.simulated;
            let mut decisions = ViewBroker {
                broker,
                policy,
                prompts: ViewPrompts {
                    seam: false,
                    simulated,
                },
            };
            match view.layout.as_mut() {
                Some(layout) => view.view.commit_with_layout(
                    &bytes,
                    gesture,
                    &mut decisions,
                    layout.width,
                    layout.height,
                    layout.font.as_mut().map(|font| (font, layout.font_size)),
                ),
                None => view.view.commit(&bytes, gesture, &mut decisions),
            }
            .map_err(fatal)?;
            view.committed = true;
            view.queue(events).map_err(fatal)?;
            Ok(abi::HostStatus::Ok as i32)
        },
    )?;
    Ok(())
}

/// Copy the batch out of guest memory: bounds-checked and length-capped
/// before any decoding.
fn batch(
    caller: &mut Caller<'_, RuntimeContext>,
    ptr: i32,
    len: i32,
) -> Result<Vec<u8>, wasmtime::Error> {
    let invalid = |violation| {
        fatal(ViewError::Invalid {
            operation: None,
            violation,
        })
    };
    let (Ok(start), Ok(length)) = (usize::try_from(ptr), usize::try_from(len)) else {
        return Err(invalid(varta_view::Violation::Truncated));
    };
    if length > view::DRAFT_MAX_BATCH_BYTES {
        return Err(invalid(varta_view::Violation::BatchTooLarge));
    }
    let Some(Extern::Memory(memory)) = caller.get_export(varta_common::abi::MEMORY_EXPORT) else {
        return Err(wasmtime::Error::msg(
            "view_commit requires an exported memory",
        ));
    };
    memory
        .data(&*caller)
        .get(start..start.saturating_add(length))
        .map(<[u8]>::to_vec)
        .ok_or_else(|| invalid(varta_view::Violation::Truncated))
}

/// Perform one trusted platform input between turns (host work, no guest
/// code): the renderer updates host-owned state and its records become host
/// tasks. A prompt it needs may use the instance's prompt seam.
/// Move the view's animation clock to the instance's time line: the virtual
/// loop time of a scripted (simulated) session, so its `wait` steps move
/// transitions deterministically, otherwise the wall clock since the
/// instance started. Returns whether a running transition's value may have
/// changed.
pub(crate) fn sync_clock(context: &mut RuntimeContext) -> bool {
    let now = if context.view.simulated {
        context.scheduler.loop_time()
    } else {
        context.started.elapsed().as_secs_f64() * MILLISECONDS_PER_SECOND
    };
    context.view.view.set_animation_time(now)
}

const MILLISECONDS_PER_SECOND: f64 = 1000.0;

pub(crate) fn input(context: &mut RuntimeContext, input: &Input) -> Result<(), RuntimeError> {
    sync_clock(context);
    let RuntimeContext {
        view,
        broker,
        policy,
        events,
        ..
    } = context;
    let simulated = view.simulated;
    let mut decisions = ViewBroker {
        broker,
        policy,
        prompts: ViewPrompts {
            seam: true,
            simulated,
        },
    };
    view.view
        .input(input, &mut decisions)
        .map_err(RuntimeError::ViewInput)?;
    view.queue(events).map_err(RuntimeError::View)
}

/// Assistive input uses the same broker, event queue and later guest turns.
pub(crate) fn accessibility_input(
    context: &mut RuntimeContext,
    identity: &varta_view::NodeIdentity,
    action: &varta_view::AccessibilityAction,
    frame: Option<&varta_view::FrameMetadata>,
) -> Result<bool, RuntimeError> {
    sync_clock(context);
    let RuntimeContext {
        view,
        broker,
        policy,
        events,
        ..
    } = context;
    let mut decisions = ViewBroker {
        broker,
        policy,
        prompts: ViewPrompts {
            seam: true,
            simulated: view.simulated,
        },
    };
    let accepted = match frame {
        Some(frame) => {
            view.view
                .accessibility_input_from_frame(identity, action, frame, &mut decisions)
        }
        None => view
            .view
            .accessibility_input(identity, action, &mut decisions),
    }
    .map_err(RuntimeError::ViewInput)?;
    if accepted {
        view.queue(events).map_err(RuntimeError::View)?;
    }
    Ok(accepted)
}

/// Run a view record's turn: the renderer learns it was delivered, the
/// record is written at the host result buffer and the guest export runs.
pub(crate) fn deliver(
    instance: &Instance,
    store: &mut Store<RuntimeContext>,
    record: &EventRecord,
) -> Result<(), RuntimeError> {
    store.data_mut().view.view.delivered(record);
    let record = record.encode();
    let Some(export) = instance.get_func(&mut *store, EVENT_EXPORT) else {
        return Err(
            wasmtime::Error::msg("view event record without the guest event export").into(),
        );
    };
    let export = export.typed::<i32, ()>(&*store)?;
    let memory = instance
        .get_memory(&mut *store, varta_common::abi::MEMORY_EXPORT)
        .ok_or_else(|| wasmtime::Error::msg("view events require an exported memory"))?;
    let desired = abi::RESULT_BASE + record.len();
    if desired > memory.data_size(&*store) {
        let pages = (desired - memory.data_size(&*store)).div_ceil(abi::WASM_PAGE_BYTES);
        memory.grow(&mut *store, pages as u64)?;
    }
    memory
        .write(&mut *store, abi::RESULT_BASE, &record)
        .map_err(wasmtime::Error::new)?;
    store.data().check_budget()?;
    export.call(&mut *store, record.len() as i32)?;
    Ok(())
}

/// Map view failures inside guest calls to their typed errors.
pub(crate) fn typed(source: wasmtime::Error) -> Result<RuntimeError, wasmtime::Error> {
    match source.downcast::<ViewError>() {
        Ok(error) => Ok(RuntimeError::View(error)),
        Err(source) => Err(source),
    }
}

impl From<InputError> for RuntimeError {
    fn from(error: InputError) -> Self {
        Self::ViewInput(error)
    }
}

/// Platform clipboard defaults are trusted host input, not a guest clipboard
/// import. Queue keydown for its usual gesture turn before returning I/O work.
pub(crate) fn begin_clipboard(
    context: &mut RuntimeContext,
    target: Option<&varta_view::ClipboardTarget>,
    key: &varta_view::ClipboardKey,
) -> Result<Option<varta_view::ClipboardTask>, RuntimeError> {
    let task = context
        .view
        .view
        .begin_clipboard(target, key)
        .map_err(RuntimeError::ViewInput)?;
    context
        .view
        .queue(&mut context.events)
        .map_err(RuntimeError::View)?;
    Ok(task)
}

/// An external completion cannot create a new user-gesture turn. Input records
/// retain their normal coalescing, capacity and delivery rules.
pub(crate) fn complete_clipboard(
    context: &mut RuntimeContext,
    token: &varta_view::ClipboardToken,
    result: Result<varta_view::ClipboardData, varta_view::ClipboardError>,
) -> Result<bool, RuntimeError> {
    let accepted = context
        .view
        .view
        .complete_clipboard(token, result)
        .map_err(RuntimeError::ViewInput)?;
    context
        .view
        .queue(&mut context.events)
        .map_err(RuntimeError::View)?;
    Ok(accepted)
}
