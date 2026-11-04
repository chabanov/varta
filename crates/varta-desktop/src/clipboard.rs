//! One clipboard operation at a time, outside the UI and application loops.
//! Only an authorized core task reaches this worker. Tests inject a provider
//! and never read or overwrite the user's system clipboard.
use std::sync::{
    Arc,
    mpsc::{self, Receiver, SyncSender, TryRecvError},
};
use varta_common::view::{DRAFT_FIELD_VALUE_RESERVATION_BYTES, DRAFT_MAX_VALUE_BYTES};
use varta_view::{
    ClipboardData as Data, ClipboardError as Error, ClipboardTask as Task, ClipboardToken,
};

const IO_QUEUE_CAPACITY: usize = 1;

pub(super) struct Completion {
    pub token: ClipboardToken,
    pub result: Result<Data, Error>,
}
trait Provider {
    fn read(&mut self) -> Result<Option<String>, Error>;
    fn write(&mut self, text: &str) -> Result<(), Error>;
}
#[derive(Default)]
struct Native {
    clipboard: Option<arboard::Clipboard>,
}
impl Native {
    fn get(&mut self) -> Result<&mut arboard::Clipboard, Error> {
        if self.clipboard.is_none() {
            self.clipboard = Some(arboard::Clipboard::new().map_err(|_| Error::Unavailable)?);
        }
        Ok(self.clipboard.as_mut().expect("initialized clipboard"))
    }
}
impl Provider for Native {
    fn read(&mut self) -> Result<Option<String>, Error> {
        match self.get()?.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(_) => Err(Error::Failed),
        }
    }
    fn write(&mut self, text: &str) -> Result<(), Error> {
        self.get()?.set_text(text).map_err(|_| Error::Failed)
    }
}
fn within_limit(text: &str, limit: usize) -> bool {
    text.len() <= limit
        && text
            .encode_utf16()
            .take(DRAFT_MAX_VALUE_BYTES / 2 + 1)
            .count()
            <= DRAFT_MAX_VALUE_BYTES / 2
}
fn perform(task: Task, provider: &mut dyn Provider) -> Completion {
    let token = task.token().clone();
    let result = match task {
        Task::Read { max_bytes, .. } => provider.read().and_then(|text| {
            let Some(text) = text else {
                return Ok(Data::Text(None));
            };
            if !within_limit(&text, max_bytes.min(DRAFT_FIELD_VALUE_RESERVATION_BYTES)) {
                return Err(Error::TooLarge);
            }
            // Native SDKs own their transient read buffers. Before retaining a
            // response, release spare capacity and enforce the existing value
            // budget; this is not a claim about total SDK/OS/RSS allocation.
            Ok(Data::Text(Some(text.into_boxed_str().into_string())))
        }),
        Task::Write { text, .. } => {
            if within_limit(&text, DRAFT_FIELD_VALUE_RESERVATION_BYTES) {
                provider.write(&text).map(|()| Data::Written)
            } else {
                Err(Error::TooLarge)
            }
        }
    };
    Completion { token, result }
}
struct Channels {
    requests: SyncSender<Task>,
    completions: Receiver<Completion>,
}
fn spawn<P: Provider + 'static>(
    make_provider: impl FnOnce() -> P + Send + 'static,
    wake: Arc<crate::wake::Wake>,
) -> Result<Channels, Error> {
    let (requests, receiver) = mpsc::sync_channel::<Task>(IO_QUEUE_CAPACITY);
    let (results, completions) = mpsc::sync_channel(IO_QUEUE_CAPACITY);
    std::thread::Builder::new()
        .name("varta-clipboard".into())
        .spawn(move || {
            let mut provider = make_provider();
            while let Ok(task) = receiver.recv() {
                let result = perform(task, &mut provider);
                if results.send(result).is_err() {
                    break;
                }
                wake.notify();
            }
        })
        .map_err(|_| Error::Unavailable)?;
    Ok(Channels {
        requests,
        completions,
    })
}

pub(super) struct Worker {
    channels: Option<Channels>,
    pending: Option<ClipboardToken>,
    wake: Arc<crate::wake::Wake>,
}
impl Worker {
    pub fn new(wake: Arc<crate::wake::Wake>) -> Self {
        Self {
            channels: None,
            pending: None,
            wake,
        }
    }
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
    pub fn submit(&mut self, task: Task) -> Result<(), Completion> {
        let token = task.token().clone();
        if self.is_pending() {
            return Err(Completion {
                token,
                result: Err(Error::Failed),
            });
        }
        if self.channels.is_none() {
            match spawn(Native::default, self.wake.clone()) {
                Ok(channels) => self.channels = Some(channels),
                Err(error) => {
                    return Err(Completion {
                        token,
                        result: Err(error),
                    });
                }
            }
        }
        let channels = self
            .channels
            .as_ref()
            .expect("initialized clipboard worker");
        if channels.requests.try_send(task).is_err() {
            self.channels = None;
            return Err(Completion {
                token,
                result: Err(Error::Unavailable),
            });
        }
        self.pending = Some(token);
        Ok(())
    }
    pub fn try_complete(&mut self) -> Option<Completion> {
        match self.channels.as_ref()?.completions.try_recv() {
            Ok(result) => {
                self.pending = None;
                Some(result)
            }
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                self.channels = None;
                self.pending.take().map(|token| Completion {
                    token,
                    result: Err(Error::Unavailable),
                })
            }
        }
    }
}

/// Interpret physical shortcut keys independently of the logical keyboard
/// layout, retaining the original key/modifiers in the guest keydown record.
pub(super) fn shortcut(
    physical: winit::keyboard::PhysicalKey,
    logical: &winit::keyboard::Key,
    modifiers: winit::keyboard::ModifiersState,
) -> Option<varta_view::ClipboardKey> {
    use winit::keyboard::{Key, NamedKey};
    const INSERT_KEY: &str = "Insert";
    let action = shortcut_action(physical, modifiers, cfg!(target_os = "macos"))?;
    let key = match logical {
        Key::Character(key)
            if !key.is_empty()
                && key.len() <= crate::input_queue::MAX_KEY_TEXT_BYTES
                && !key.chars().any(char::is_control) =>
        {
            key.to_string()
        }
        Key::Named(NamedKey::Insert) => INSERT_KEY.into(),
        Key::Named(NamedKey::Delete) => varta_common::view::Key::Delete.dom_key().into(),
        _ => return None,
    };
    Some(varta_view::ClipboardKey {
        action,
        key,
        shift: modifiers.shift_key(),
        ctrl: modifiers.control_key(),
        alt: modifiers.alt_key(),
        meta: modifiers.super_key(),
    })
}
fn shortcut_action(
    physical: winit::keyboard::PhysicalKey,
    modifiers: winit::keyboard::ModifiersState,
    macos: bool,
) -> Option<varta_view::ClipboardAction> {
    use varta_view::ClipboardAction as A;
    use winit::keyboard::{KeyCode, PhysicalKey};
    if modifiers.alt_key() {
        return None;
    }
    let command = if macos {
        modifiers.super_key() && !modifiers.control_key()
    } else {
        modifiers.control_key() && !modifiers.super_key()
    };
    if command {
        return match physical {
            PhysicalKey::Code(KeyCode::KeyC) => Some(A::Copy),
            PhysicalKey::Code(KeyCode::KeyX) => Some(A::Cut),
            PhysicalKey::Code(KeyCode::KeyV) => Some(A::Paste),
            PhysicalKey::Code(KeyCode::Insert) if !macos && !modifiers.shift_key() => Some(A::Copy),
            _ => None,
        };
    }
    if !macos && modifiers.shift_key() && !modifiers.control_key() && !modifiers.super_key() {
        return match physical {
            PhysicalKey::Code(KeyCode::Insert) => Some(A::Paste),
            PhysicalKey::Code(KeyCode::Delete) => Some(A::Cut),
            _ => None,
        };
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use varta_common::view::Tag;
    use varta_view::{
        Access, BatchWriter, Broker, ClipboardAction, ClipboardKey, Input, Target, View,
    };
    struct Deny;
    impl Broker for Deny {
        fn link_requested(&self, _: &str) -> bool {
            false
        }
        fn open_url(&mut self, _: &str) -> Access {
            Access::Denied
        }
        fn clipboard_write(&mut self) -> Access {
            Access::Denied
        }
    }
    fn task(action: ClipboardAction) -> Task {
        let mut v = View::default();
        let mut b = BatchWriter::new();
        b.create_element(2, Tag::Input)
            .set_value(2, "source", 0)
            .insert(1, 2, 0);
        v.commit(&b.finish(), false, &mut Deny).unwrap();
        v.input(
            &Input::TextSelect {
                target: Target::Node(2),
                anchor: 0,
                focus: 6,
                activate: false,
            },
            &mut Deny,
        )
        .unwrap();
        v.begin_clipboard(
            v.clipboard_target().as_ref(),
            &ClipboardKey {
                action,
                key: "v".into(),
                shift: false,
                ctrl: false,
                alt: false,
                meta: true,
            },
        )
        .unwrap()
        .unwrap()
    }
    struct Mock {
        read: Result<Option<String>, Error>,
        writes: Vec<String>,
        fail_write: bool,
    }
    impl Provider for Mock {
        fn read(&mut self) -> Result<Option<String>, Error> {
            std::mem::replace(&mut self.read, Ok(None))
        }
        fn write(&mut self, text: &str) -> Result<(), Error> {
            if self.fail_write {
                Err(Error::Failed)
            } else {
                self.writes.push(text.into());
                Ok(())
            }
        }
    }
    fn mock(read: Result<Option<String>, Error>) -> Mock {
        Mock {
            read,
            writes: Vec::new(),
            fail_write: false,
        }
    }
    #[test]
    fn read_responses_release_capacity_and_refuse_complete_overflow() {
        let mut text = String::with_capacity(1024 * 1024);
        text.push_str("ї😀");
        let mut m = mock(Ok(Some(text)));
        let result = perform(task(ClipboardAction::Paste), &mut m);
        let Ok(Data::Text(Some(text))) = result.result else {
            panic!("bounded read")
        };
        assert_eq!(text, "ї😀");
        assert_eq!(text.capacity(), text.len());
        for text in [
            "A".repeat(DRAFT_MAX_VALUE_BYTES / 2 + 1),
            "界".repeat(DRAFT_MAX_VALUE_BYTES / 2 + 1),
        ] {
            let mut m = mock(Ok(Some(text)));
            assert!(matches!(
                perform(task(ClipboardAction::Paste), &mut m).result,
                Err(Error::TooLarge)
            ));
        }
    }
    #[test]
    fn writes_are_validated_before_the_provider_and_failures_are_normal_results() {
        let task = task(ClipboardAction::Copy);
        let token = task.token().clone();
        let mut m = mock(Ok(None));
        assert!(matches!(
            perform(
                Task::Write {
                    token,
                    text: "A".repeat(DRAFT_MAX_VALUE_BYTES / 2 + 1)
                },
                &mut m
            )
            .result,
            Err(Error::TooLarge)
        ));
        assert!(m.writes.is_empty());
        m.fail_write = true;
        assert!(matches!(perform(task, &mut m).result, Err(Error::Failed)));
        assert!(m.writes.is_empty());
    }
    #[test]
    fn unavailable_text_does_not_become_an_empty_replacement() {
        let mut m = mock(Ok(None));
        assert!(matches!(
            perform(task(ClipboardAction::Paste), &mut m).result,
            Ok(Data::Text(None))
        ));
        let mut m = mock(Err(Error::Unavailable));
        assert!(matches!(
            perform(task(ClipboardAction::Paste), &mut m).result,
            Err(Error::Unavailable)
        ));
    }
    #[test]
    fn platform_shortcuts_use_physical_keys_and_do_not_capture_altgr_or_plain_typing() {
        use varta_view::ClipboardAction as A;
        use winit::keyboard::{KeyCode as K, ModifiersState as M, PhysicalKey as P};
        for (macos, modifier) in [(true, M::SUPER), (false, M::CONTROL)] {
            for (code, action) in [(K::KeyC, A::Copy), (K::KeyX, A::Cut), (K::KeyV, A::Paste)] {
                assert_eq!(
                    shortcut_action(P::Code(code), modifier, macos),
                    Some(action)
                );
                assert_eq!(
                    shortcut_action(P::Code(code), modifier | M::ALT, macos),
                    None
                );
                assert_eq!(shortcut_action(P::Code(code), M::empty(), macos), None);
            }
        }
        assert_eq!(
            shortcut_action(P::Code(K::Insert), M::CONTROL, false),
            Some(A::Copy)
        );
        assert_eq!(
            shortcut_action(P::Code(K::Insert), M::SHIFT, false),
            Some(A::Paste)
        );
        assert_eq!(
            shortcut_action(P::Code(K::Delete), M::SHIFT, false),
            Some(A::Cut)
        );
        assert_eq!(shortcut_action(P::Code(K::Insert), M::SHIFT, true), None);
    }
    #[test]
    fn shortcuts_retain_original_logical_keys_without_accepting_control_text() {
        use winit::keyboard::{Key, KeyCode, ModifiersState, PhysicalKey};
        let modifier = if cfg!(target_os = "macos") {
            ModifiersState::SUPER
        } else {
            ModifiersState::CONTROL
        };
        let key = shortcut(
            PhysicalKey::Code(KeyCode::KeyC),
            &Key::Character("с".into()),
            modifier,
        )
        .unwrap();
        assert_eq!(key.key, "с");
        assert_eq!(key.action, ClipboardAction::Copy);
        for invalid in ["", "\0", "\n"] {
            assert!(
                shortcut(
                    PhysicalKey::Code(KeyCode::KeyV),
                    &Key::Character(invalid.into()),
                    modifier
                )
                .is_none()
            );
        }
    }
    #[test]
    fn disconnected_io_releases_the_pending_operation_for_a_later_shortcut() {
        let mut worker = Worker::new(Arc::default());
        let task = task(ClipboardAction::Paste);
        let (requests, _) = mpsc::sync_channel(1);
        let (results, completions) = mpsc::sync_channel(1);
        drop(results);
        worker.channels = Some(Channels {
            requests,
            completions,
        });
        worker.pending = Some(task.token().clone());
        let result = worker.try_complete().unwrap();
        assert!(matches!(result.result, Err(Error::Unavailable)));
        assert!(!worker.is_pending());
        assert!(worker.channels.is_none());
    }
    #[test]
    fn worker_is_lazy_and_only_one_operation_can_be_in_flight() {
        let wake = Arc::default();
        let mut worker = Worker::new(wake);
        assert!(worker.channels.is_none());
        assert!(!worker.is_pending());
        let wake = Arc::default();
        worker.channels = Some(spawn(|| mock(Ok(Some("bounded".into()))), wake).unwrap());
        assert!(worker.submit(task(ClipboardAction::Paste)).is_ok());
        assert!(worker.submit(task(ClipboardAction::Copy)).is_err());
        // Receive deterministically without polling or reading the OS clipboard.
        let result = worker
            .channels
            .as_ref()
            .unwrap()
            .completions
            .recv()
            .unwrap();
        assert!(matches!(result.result,Ok(Data::Text(Some(ref text))) if text=="bounded"));
    }
}
