//! Overlay scrollbars as Chrome 156 shows them on macOS when the system
//! shows scroll bars only while scrolling: the state machine of
//! `ui::OverlayScrollbarAnimatorMac` as Blink's `MacScrollbarImplV2` drives
//! it, on the view's animation clock ([`crate::View::set_animation_time`]).
//!
//! A scroll that changes a container's offset (wheel, programmatic,
//! scroll-into-view or assistive) shows its thumb at once; the thumb starts
//! to fade [`FADE_OUT_DELAY_MS`] after the last scroll and is gone
//! [`ANIMATION_MS`] later. A pointer entering the bar's frame while the
//! thumb shows cancels the fade, fades the track in and widens the thumb
//! box from [`THUMB_BOX`] to [`THUMB_BOX_EXPANDED`] pixels, each over
//! [`ANIMATION_MS`] with ease-in-out; the thumb stays widened until it has
//! faded out, and the fade-out timer runs again once the pointer leaves.
//! Every state is a pure function of the event times and the clock, so a
//! headless render at a given clock is deterministic.
use crate::{NodeIdentity, transition::Timing};
use std::{collections::BTreeMap, sync::Arc};

/// Delay from the last scroll (or the pointer leaving the bar) to the start
/// of the fade-out (`OverlayScrollbarAnimatorMac::kFadeOutDelay`).
pub(crate) const FADE_OUT_DELAY_MS: f64 = 500.0;
/// The duration of the fade-out, the track fade-in and the thumb expansion
/// (`OverlayScrollbarAnimatorMac::kAnimationDuration`), each ease-in-out.
pub(crate) const ANIMATION_MS: f64 = 250.0;
/// The thumb box width before and after the pointer enters the bar
/// (`MacScrollbarImplV2`, `scrollbar-width: auto`), in CSS pixels.
pub(crate) const THUMB_BOX: i32 = 12;
pub(crate) const THUMB_BOX_EXPANDED: i32 = 16;

/// What one overlay bar paints at the current clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Visual {
    /// The thumb's opacity (the knob alpha).
    pub thumb_alpha: f32,
    /// The track's opacity; zero while the pointer has not entered the bar.
    pub track_alpha: f32,
    /// The thumb box width in whole CSS pixels, at the bar's outer edge.
    pub thumb_box: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Track {
    Hidden,
    /// Fading in since this clock time.
    FadingIn(f64),
    Shown,
}

/// One scroll container's shown overlay bar; a container with none has a
/// hidden bar.
#[derive(Debug, Clone, PartialEq)]
struct Bar {
    node: NodeIdentity,
    /// Whether the pointer is over the bar's frame (`mouse_in_track_`).
    mouse_in_track: bool,
    /// The armed fade-out timer: the fade-out starts at this clock time.
    fade: Option<f64>,
    /// When the thumb box started to widen.
    expand: Option<f64>,
    track: Track,
}

/// Ease-in-out progress of an animation that started at `start`.
fn progress(start: f64, now: f64) -> f64 {
    Timing::EASE_IN_OUT.output(((now - start) / ANIMATION_MS).clamp(0.0, 1.0))
}

impl Bar {
    fn new(node: NodeIdentity) -> Self {
        Self {
            node,
            mouse_in_track: false,
            fade: None,
            expand: None,
            track: Track::Hidden,
        }
    }
    fn fading(&self, now: f64) -> bool {
        self.fade.is_some_and(|start| now >= start)
    }
    fn track_alpha(&self, now: f64) -> f64 {
        match self.track {
            Track::Hidden => 0.0,
            Track::FadingIn(start) => progress(start, now),
            Track::Shown => 1.0,
        }
    }
    /// The bar at `now`, or `None` once it has faded out.
    fn visual(&self, now: f64) -> Option<Visual> {
        let (thumb_alpha, track_alpha) = match self.fade.filter(|start| now >= *start) {
            Some(start) => {
                if now >= start + ANIMATION_MS {
                    return None;
                }
                let fade = progress(start, now);
                // The fade-out stops a running track fade-in; a track that
                // had appeared then fades with the thumb.
                let track = if self.track_alpha(start) == 0.0 {
                    0.0
                } else {
                    1.0 - fade
                };
                (1.0 - fade, track)
            }
            None => (1.0, self.track_alpha(now)),
        };
        // The animator keeps an integer width (`int thumb_width_`).
        let thumb_box = self.expand.map_or(THUMB_BOX, |start| {
            let expanded = progress(start, now);
            ((1.0 - expanded) * f64::from(THUMB_BOX) + expanded * f64::from(THUMB_BOX_EXPANDED))
                as i32
        });
        Some(Visual {
            thumb_alpha: thumb_alpha as f32,
            track_alpha: track_alpha as f32,
            thumb_box,
        })
    }
    /// `FadeOutAnimationCancel`: the thumb is opaque again, and so is a
    /// track that had appeared.
    fn cancel_fade(&mut self, now: f64) {
        if !self.fading(now) {
            return;
        }
        let track = self
            .visual(now)
            .map_or(0.0, |visual| f64::from(visual.track_alpha));
        self.track = if track > 0.0 {
            Track::Shown
        } else {
            Track::Hidden
        };
        self.fade = None;
    }
    /// `FadeOutTimerUpdate`: no fade while the pointer is over the bar,
    /// otherwise the timer restarts.
    fn arm(&mut self, now: f64) {
        self.fade = (!self.mouse_in_track).then_some(now + FADE_OUT_DELAY_MS);
    }
    /// `MouseDidEnter` on a shown bar. Returns whether the state changed.
    fn enter(&mut self, now: f64) -> bool {
        if self.mouse_in_track {
            return false;
        }
        self.mouse_in_track = true;
        self.cancel_fade(now);
        self.arm(now);
        self.track = match self.track {
            Track::Hidden => Track::FadingIn(now),
            Track::FadingIn(start) if now >= start + ANIMATION_MS => Track::Shown,
            track => track,
        };
        // The thumb widens once and stays wide until it has faded out.
        self.expand.get_or_insert(now);
        true
    }
    fn animating(&self, now: f64) -> bool {
        self.fade.is_some()
            || self.expand.is_some_and(|start| now < start + ANIMATION_MS)
            || matches!(self.track, Track::FadingIn(start) if now < start + ANIMATION_MS)
    }
}

/// The overlay bars of one view: the clock, every shown bar and the scroll
/// container whose bar frame the pointer is over.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct OverlayScrollbars {
    now: f64,
    bars: BTreeMap<u32, Bar>,
    hovered: Option<NodeIdentity>,
}

impl OverlayScrollbars {
    /// Move the clock to `now` (never back) and drop faded bars. Returns
    /// whether any bar paints differently.
    pub(crate) fn advance(&mut self, now: f64) -> bool {
        if !now.is_finite() || now <= self.now {
            return false;
        }
        let before = self.now;
        self.now = now;
        let mut changed = false;
        self.bars.retain(|_, bar| {
            let visual = bar.visual(now);
            changed |= bar.visual(before) != visual;
            visual.is_some()
        });
        changed
    }

    /// Whether a fade is pending or an animation runs: the host keeps
    /// advancing the clock, and presents a frame when [`Self::advance`]
    /// reports a change.
    pub(crate) fn animating(&self) -> bool {
        self.bars.values().any(|bar| bar.animating(self.now))
    }

    fn prune(&mut self) {
        let now = self.now;
        self.bars.retain(|_, bar| bar.visual(now).is_some());
    }

    fn bar_mut(&mut self, node: &NodeIdentity) -> Option<&mut Bar> {
        self.bars.get_mut(&node.id).filter(|bar| bar.node == *node)
    }

    /// A scroll changed `node`'s offset (`DidScroll`): show its thumb, or
    /// keep it shown and restart the fade-out timer.
    pub(crate) fn scrolled(&mut self, node: NodeIdentity) {
        let now = self.now;
        self.prune();
        if let Some(bar) = self.bar_mut(&node) {
            bar.cancel_fade(now);
            bar.arm(now);
            return;
        }
        let hovered = self.hovered.as_ref() == Some(&node);
        let mut bar = Bar::new(node);
        bar.arm(now);
        // The bar that appears under a resting pointer takes it at once, as
        // Blink's hover update after the scroll does.
        if hovered {
            bar.enter(now);
        }
        self.bars.insert(bar.node.id, bar);
    }

    /// The pointer moved over `node`'s bar frame, or off every bar
    /// (`MouseDidEnter`/`MouseDidExit`; a hidden bar ignores the pointer).
    /// Returns whether a bar's state changed.
    pub(crate) fn hover(&mut self, node: Option<NodeIdentity>) -> bool {
        let now = self.now;
        self.prune();
        if self.hovered == node {
            return false;
        }
        let mut changed = false;
        if let Some(previous) = self.hovered.take()
            && let Some(bar) = self.bar_mut(&previous)
            && bar.mouse_in_track
        {
            bar.mouse_in_track = false;
            bar.arm(now);
            changed = true;
        }
        if let Some(bar) = node.as_ref().and_then(|node| self.bar_mut(node)) {
            changed |= bar.enter(now);
        }
        self.hovered = node;
        changed
    }

    /// The bar of the element `id` with identity `token` at the clock.
    pub(crate) fn visual(&self, id: u32, token: &Arc<()>) -> Option<Visual> {
        self.bars
            .get(&id)
            .filter(|bar| Arc::ptr_eq(&bar.node.token, token))?
            .visual(self.now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u32) -> NodeIdentity {
        NodeIdentity {
            id,
            token: Arc::new(()),
        }
    }

    fn at(bars: &mut OverlayScrollbars, now: f64, node: &NodeIdentity) -> Option<Visual> {
        bars.advance(now);
        bars.visual(node.id, &node.token)
    }

    #[test]
    fn a_scroll_shows_the_thumb_until_the_delay_and_fade_end() {
        let scroller = node(2);
        let mut bars = OverlayScrollbars::default();
        assert_eq!(bars.visual(2, &scroller.token), None);
        bars.advance(1000.0);
        bars.scrolled(scroller.clone());
        let shown = Visual {
            thumb_alpha: 1.0,
            track_alpha: 0.0,
            thumb_box: THUMB_BOX,
        };
        assert_eq!(bars.visual(2, &scroller.token), Some(shown));
        assert!(bars.animating());
        // Nothing changes during the delay: no frame is needed.
        assert!(!bars.advance(1499.0));
        assert_eq!(at(&mut bars, 1500.0, &scroller), Some(shown));
        // Half way through the ease-in-out fade the thumb is half visible.
        let half = at(&mut bars, 1625.0, &scroller).unwrap();
        assert!((half.thumb_alpha - 0.5).abs() < 1e-6, "{half:?}");
        assert!(bars.advance(1700.0));
        assert_eq!(at(&mut bars, 1750.0, &scroller), None);
        assert!(!bars.animating());
        // Another element with the same id never inherits the bar.
        bars.scrolled(scroller.clone());
        assert_eq!(bars.visual(2, &node(2).token), None);
    }

    #[test]
    fn scrolling_again_restarts_the_timer_and_cancels_a_fade() {
        let scroller = node(2);
        let mut bars = OverlayScrollbars::default();
        bars.scrolled(scroller.clone());
        bars.advance(400.0);
        bars.scrolled(scroller.clone());
        assert_eq!(at(&mut bars, 899.0, &scroller).unwrap().thumb_alpha, 1.0);
        assert!(at(&mut bars, 1000.0, &scroller).unwrap().thumb_alpha < 1.0);
        // A scroll during the fade makes the thumb opaque at once.
        bars.scrolled(scroller.clone());
        assert_eq!(bars.visual(2, &scroller.token).unwrap().thumb_alpha, 1.0);
        assert_eq!(at(&mut bars, 1499.0, &scroller).unwrap().thumb_alpha, 1.0);
        assert_eq!(at(&mut bars, 1750.0, &scroller), None);
    }

    #[test]
    fn hovering_expands_the_thumb_shows_the_track_and_holds_the_fade() {
        let scroller = node(2);
        let mut bars = OverlayScrollbars::default();
        // A hidden bar ignores the pointer.
        assert!(!bars.hover(Some(scroller.clone())));
        assert!(!bars.hover(None));
        bars.scrolled(scroller.clone());
        bars.advance(100.0);
        assert!(bars.hover(Some(scroller.clone())));
        let start = bars.visual(2, &scroller.token).unwrap();
        assert_eq!((start.thumb_box, start.track_alpha), (THUMB_BOX, 0.0));
        let half = at(&mut bars, 225.0, &scroller).unwrap();
        // Ease-in-out at half time is half way: 14 whole pixels.
        assert_eq!(half.thumb_box, 14);
        assert!((half.track_alpha - 0.5).abs() < 1e-6, "{half:?}");
        let full = Visual {
            thumb_alpha: 1.0,
            track_alpha: 1.0,
            thumb_box: THUMB_BOX_EXPANDED,
        };
        assert_eq!(at(&mut bars, 350.0, &scroller), Some(full));
        // No fade while the pointer stays, and nothing animates.
        assert_eq!(at(&mut bars, 5000.0, &scroller), Some(full));
        assert!(!bars.animating());
        // Leaving restarts the timer; the thumb fades out still wide.
        assert!(bars.hover(None));
        assert_eq!(at(&mut bars, 5499.0, &scroller), Some(full));
        let fading = at(&mut bars, 5625.0, &scroller).unwrap();
        assert_eq!(fading.thumb_box, THUMB_BOX_EXPANDED);
        assert!((fading.track_alpha - 0.5).abs() < 1e-6, "{fading:?}");
        assert_eq!(at(&mut bars, 5750.0, &scroller), None);
        // Shown again, it starts narrow.
        bars.scrolled(scroller.clone());
        assert_eq!(
            bars.visual(2, &scroller.token).unwrap().thumb_box,
            THUMB_BOX
        );
    }

    #[test]
    fn entering_during_the_fade_restores_the_thumb() {
        let scroller = node(2);
        let mut bars = OverlayScrollbars::default();
        bars.scrolled(scroller.clone());
        assert!(at(&mut bars, 600.0, &scroller).unwrap().thumb_alpha < 1.0);
        bars.hover(Some(scroller.clone()));
        let entered = bars.visual(2, &scroller.token).unwrap();
        assert_eq!((entered.thumb_alpha, entered.track_alpha), (1.0, 0.0));
        assert_eq!(at(&mut bars, 850.0, &scroller).unwrap().track_alpha, 1.0);
    }

    #[test]
    fn a_scroll_under_a_resting_pointer_takes_it_at_once() {
        let scroller = node(2);
        let mut bars = OverlayScrollbars::default();
        bars.hover(Some(scroller.clone()));
        bars.scrolled(scroller.clone());
        // Narrow at first, then widening; no fade while the pointer rests.
        assert_eq!(
            bars.visual(2, &scroller.token).unwrap().thumb_box,
            THUMB_BOX
        );
        let later = at(&mut bars, 2000.0, &scroller).unwrap();
        assert_eq!(
            (later.thumb_box, later.track_alpha),
            (THUMB_BOX_EXPANDED, 1.0)
        );
    }
}
