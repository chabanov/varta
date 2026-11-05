//! The embedder's real platform for the guest's two platform effects:
//! `copy_text` writes the system clipboard and an external link activation
//! opens the system browser.
//!
//! Without a sink the view keeps simulating the platform (headless
//! sessions): a granted effect is recorded in the snapshot and reports `ok`
//! unless the scripted [`crate::Platform`] refuses it. With a sink the view
//! records nothing and the sink's answer is the outcome. Either way the sink
//! is reached only after the gesture rule and the permission broker granted
//! the operation, so a denial happens before any effect.
use std::sync::Arc;
use varta_common::view::CommandStatus;

/// The platform refused a granted effect: the guest receives
/// `refused_by_platform`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the platform refused the effect")]
pub struct PlatformRefusal;

/// A native host's clipboard and system browser.
pub trait PlatformEffects: std::fmt::Debug + Send + Sync {
    /// Write `text` (plain text within the value-byte limit) to the system
    /// clipboard.
    fn write_clipboard(&self, text: &str) -> Result<(), PlatformRefusal>;
    /// Hand `url` (an absolute `https:` or `http:` URL a `ui.open_url`
    /// grant matched) to the system browser.
    fn open_url(&self, url: &str) -> Result<(), PlatformRefusal>;
}

/// The installed sink of one view.
pub(crate) type Sink = Option<Arc<dyn PlatformEffects>>;

/// The guest-visible outcome of a sink's answer.
pub(crate) fn status(result: Result<(), PlatformRefusal>) -> CommandStatus {
    match result {
        Ok(()) => CommandStatus::Ok,
        Err(PlatformRefusal) => CommandStatus::RefusedByPlatform,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Access, BatchWriter, Broker, Input, Platform, Target, View};
    use std::sync::Mutex;
    use varta_common::view::{Attribute, EventKind, ResultOperation, Tag};

    /// Records every effect it receives and answers with `refuse`.
    #[derive(Debug, Default)]
    struct Recording {
        refuse: bool,
        clipboard: Mutex<Vec<String>>,
        opened: Mutex<Vec<String>>,
    }
    impl PlatformEffects for Recording {
        fn write_clipboard(&self, text: &str) -> Result<(), PlatformRefusal> {
            self.clipboard.lock().unwrap().push(text.into());
            if self.refuse {
                Err(PlatformRefusal)
            } else {
                Ok(())
            }
        }
        fn open_url(&self, url: &str) -> Result<(), PlatformRefusal> {
            self.opened.lock().unwrap().push(url.into());
            if self.refuse {
                Err(PlatformRefusal)
            } else {
                Ok(())
            }
        }
    }
    struct Decide(Access);
    impl Broker for Decide {
        fn link_requested(&self, _: &str) -> bool {
            true
        }
        fn open_url(&mut self, _: &str) -> Access {
            self.0
        }
        fn clipboard_write(&mut self) -> Access {
            self.0
        }
    }
    const LINK: u32 = 2;
    const URL: &str = "https://docs.test/guide";
    const REQUEST: u32 = 7;
    const TEXT: &str = "copied";

    fn link(view: &mut View, broker: &mut Decide) {
        let mut batch = BatchWriter::new();
        batch
            .create_element(LINK, Tag::A)
            .set_attribute(LINK, Attribute::Href, URL)
            .insert(1, LINK, 0);
        view.commit(&batch.finish(), false, broker).unwrap();
    }
    fn statuses(view: &mut View) -> Vec<(String, u32)> {
        std::iter::from_fn(|| view.next_delivery())
            .filter(|delivery| delivery.record.kind == EventKind::CommandResult)
            .map(|delivery| (delivery.record.key.clone(), delivery.record.status))
            .collect()
    }
    fn copy(view: &mut View, broker: &mut Decide, gesture: bool) {
        let mut batch = BatchWriter::new();
        batch.copy_text(REQUEST, TEXT);
        view.commit(&batch.finish(), gesture, broker).unwrap();
    }

    #[test]
    fn granted_gesture_effects_reach_the_sink_and_report_its_answer() {
        for refuse in [false, true] {
            let sink = Arc::new(Recording {
                refuse,
                ..Recording::default()
            });
            let mut view = View::new(Default::default(), Platform::default())
                .with_platform_effects(sink.clone());
            let mut broker = Decide(Access::Granted);
            link(&mut view, &mut broker);
            view.input(
                &Input::Click {
                    target: Target::Node(LINK),
                },
                &mut broker,
            )
            .unwrap();
            copy(&mut view, &mut broker, true);
            let expected = if refuse {
                CommandStatus::RefusedByPlatform
            } else {
                CommandStatus::Ok
            };
            assert_eq!(
                statuses(&mut view),
                [
                    (ResultOperation::OpenUrl.name().into(), expected.code()),
                    (ResultOperation::CopyText.name().into(), expected.code()),
                ]
            );
            assert_eq!(*sink.opened.lock().unwrap(), [URL]);
            assert_eq!(*sink.clipboard.lock().unwrap(), [TEXT]);
            // A real platform's effects never enter the snapshot.
            let snapshot = view.snapshot();
            assert!(snapshot["clipboard"].is_null());
            assert_eq!(snapshot["opened"], serde_json::json!([]));
        }
    }

    #[test]
    fn denied_and_non_gesture_effects_never_reach_the_sink() {
        let sink = Arc::new(Recording::default());
        let mut view =
            View::new(Default::default(), Platform::default()).with_platform_effects(sink.clone());
        let mut denied = Decide(Access::Denied);
        link(&mut view, &mut denied);
        view.input(
            &Input::Click {
                target: Target::Node(LINK),
            },
            &mut denied,
        )
        .unwrap();
        copy(&mut view, &mut denied, true);
        // A delivered result releases its request id.
        let mut results = statuses(&mut view);
        // Granted, but outside a user-gesture turn.
        copy(&mut view, &mut Decide(Access::Granted), false);
        results.extend(statuses(&mut view));
        assert_eq!(
            results,
            [
                (
                    ResultOperation::OpenUrl.name().into(),
                    CommandStatus::NotAllowed.code()
                ),
                (
                    ResultOperation::CopyText.name().into(),
                    CommandStatus::NotAllowed.code()
                ),
                (
                    ResultOperation::CopyText.name().into(),
                    CommandStatus::InvalidContext.code()
                ),
            ]
        );
        assert!(sink.opened.lock().unwrap().is_empty());
        assert!(sink.clipboard.lock().unwrap().is_empty());
    }
}
