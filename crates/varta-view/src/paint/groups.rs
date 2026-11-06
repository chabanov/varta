//! Paint scopes retain source geometry for input while isolating ancestor clips
//! from a group's child draws. Pixel composition is a separate bounded replay.
use super::*;
use std::sync::Arc;

// Pointer keys are valid only while their original allocations stay alive.
// Retaining the source also retains the validated base in its ancestor chain.
pub(super) struct RelativeClip {
    source: Arc<Clip>,
    relative: Option<Arc<Clip>>,
}

pub(super) struct Scope {
    node: u32,
    start: Option<usize>,
    inherited: BoxRect,
    base: Option<Arc<Clip>>,
    pub hidden: bool,
}
impl Surface<'_> {
    fn paint_clip(&self, clip: BoxRect) -> BoxRect {
        let mut paint = snap::rect(clip);
        if clip.width > 0.0 && clip.x + clip.width == self.width as f32 / self.scale {
            paint.width = (clip.x + clip.width - paint.x).max(0.0);
        }
        if clip.height > 0.0 && clip.y + clip.height == self.height as f32 / self.scale {
            paint.height = (clip.y + clip.height - paint.y).max(0.0);
        }
        paint
    }
    pub(super) fn local_clip(&self, clip: BoxRect) -> BoxRect {
        if self.capture.is_some() {
            return clip;
        }
        let Some(scope) = self.groups.last() else {
            return self.paint_clip(clip);
        };
        let base = scope.inherited;
        let left = if clip.x == base.x { 0.0 } else { clip.x };
        let top = if clip.y == base.y { 0.0 } else { clip.y };
        let right = if clip.x + clip.width == base.x + base.width {
            self.width as f32 / self.scale
        } else {
            clip.x + clip.width
        };
        let bottom = if clip.y + clip.height == base.y + base.height {
            self.height as f32 / self.scale
        } else {
            clip.y + clip.height
        };
        self.paint_clip(BoxRect {
            x: left,
            y: top,
            width: (right - left).max(0.0),
            height: (bottom - top).max(0.0),
        })
    }
    pub(super) fn relative_clip(
        &mut self,
        source: Option<Arc<Clip>>,
    ) -> Result<Option<Arc<Clip>>, PaintError> {
        let Some(base) = self.groups.last().and_then(|group| group.base.clone()) else {
            return Ok(source);
        };
        let Some(source) = source else {
            return Err(PaintError::Unsupported(
                self.groups.last().expect("group base exists").node,
            ));
        };
        let key = (Arc::as_ptr(&source) as usize, Arc::as_ptr(&base) as usize);
        if let Some(found) = self.relative_clips.get(&key) {
            debug_assert!(Arc::ptr_eq(&source, &found.source));
            return Ok(found.relative.clone());
        }
        let mut missing = Vec::new();
        let mut current = Some(source.clone());
        while let Some(clip) = current {
            if Arc::ptr_eq(&clip, &base) {
                let mut result = None;
                for source in missing.into_iter().rev() {
                    result = Some(Clip::reparent(&source, result));
                }
                self.relative_clips.insert(
                    key,
                    RelativeClip {
                        source,
                        relative: result.clone(),
                    },
                );
                return Ok(result);
            }
            current = clip.parent.clone();
            missing.push(clip);
        }
        Err(PaintError::Unsupported(
            self.groups.last().expect("group base exists").node,
        ))
    }
    pub(super) fn begin_group(
        &mut self,
        node: u32,
        opacity: f32,
        inherited: BoxRect,
    ) -> Result<(), PaintError> {
        let base = self.active_clip.clone();
        let hidden = opacity == 0.0 || self.groups.last().is_some_and(|scope| scope.hidden);
        let start = if hidden {
            None
        } else {
            // A stateless frame or an earlier ordinary-cache overflow may have
            // already produced pixels. Keep that prefix and capture only the
            // remaining bounded program; replay starts from those same pixels.
            self.isolation_required = true;
            if self.commands.is_none() {
                self.commands = Some(Recording::default());
            }

            let local = self.local_clip(inherited);
            let mut clips = self.relative_clip(base.clone())?;
            if local.width == 0.0 || local.height == 0.0 {
                clips = Some(Clip::new(
                    clips,
                    None,
                    scaled_rect(local, self.scale),
                    Corners::default(),
                ));
            }
            let clip = Rect::from_xywh(local.x, local.y, local.width, local.height)
                .and_then(|r| r.transform(Transform::from_scale(self.scale, self.scale)));
            let bounds = Rect::from_xywh(0.0, 0.0, self.width as f32, self.height as f32)
                .ok_or(PaintError::Dimensions)?;
            let draw = Draw {
                primitive: Primitive::BeginGroup {
                    opacity,
                    additive: false,
                    end: 0,
                },
                color: Color::TRANSPARENT,
                transform: Transform::identity(),
                clip,
                clips,
                bounds,
            };
            let recording = self
                .commands
                .as_mut()
                .ok_or(PaintError::Unsupported(node))?;
            let index = recording.commands.len();
            recording.push(draw)?;
            Some(index)
        };
        self.groups.push(Scope {
            node,
            start,
            inherited,
            base,
            hidden,
        });
        Ok(())
    }
    pub(super) fn end_group(&mut self, node: u32) -> Result<(), PaintError> {
        if self.groups.last().is_none_or(|scope| scope.node != node) {
            return Ok(());
        }
        let scope = self.groups.pop().expect("matching group scope");
        let Some(start) = scope.start else {
            return Ok(());
        };
        let recording = self
            .commands
            .as_mut()
            .ok_or(PaintError::Unsupported(node))?;
        if recording.commands.len() == start + 1 {
            let begin = recording.commands.pop().expect("group begins here");
            recording.bytes -= begin.bytes();
            return Ok(());
        }
        let merge = |old: Option<Rect>, next: Rect| -> Result<Option<Rect>, PaintError> {
            Ok(Some(match old {
                None => next,
                Some(old) => Rect::from_ltrb(
                    old.left().min(next.left()),
                    old.top().min(next.top()),
                    old.right().max(next.right()),
                    old.bottom().max(next.bottom()),
                )
                .ok_or(PaintError::Dimensions)?,
            }))
        };
        let mut raw = None;
        let mut visible = None;
        for draw in &recording.commands[start + 1..] {
            if matches!(
                draw.primitive,
                Primitive::Rect(_)
                    | Primitive::Path(..)
                    | Primitive::Border(_)
                    | Primitive::Image(_)
            ) {
                raw = merge(raw, draw.bounds)?;
                let mut bounds = Some(draw.bounds);
                if let Some(clip) = draw.clip {
                    bounds = bounds.and_then(|b| b.intersect(&clip));
                }
                if let Some(clip) = &draw.clips {
                    bounds = bounds.and_then(|b| clip.bounds.and_then(|clip| b.intersect(&clip)));
                }
                if let Some(bounds) = bounds {
                    visible = merge(visible, bounds)?;
                }
            }
        }
        let bounds = visible.or(raw).unwrap_or(recording.commands[start].bounds);
        let end = recording.commands.len();
        let Primitive::BeginGroup { end: target, .. } = &mut recording.commands[start].primitive
        else {
            unreachable!()
        };
        *target = end;
        recording.commands[start].bounds = bounds;
        let begin = &mut recording.commands[start];
        let before = begin.bytes();
        if let Some(clip) = begin.clip {
            // Remove each redundant axis constraint independently. Keeping an
            // identical fractional edge would multiply its AA coverage twice.
            let left = if bounds.left() >= clip.left() {
                0.0
            } else {
                clip.left()
            };
            let top = if bounds.top() >= clip.top() {
                0.0
            } else {
                clip.top()
            };
            let right = if bounds.right() <= clip.right() {
                self.width as f32
            } else {
                clip.right()
            };
            let bottom = if bounds.bottom() <= clip.bottom() {
                self.height as f32
            } else {
                clip.bottom()
            };
            begin.clip = if left == 0.0
                && top == 0.0
                && right == self.width as f32
                && bottom == self.height as f32
            {
                None
            } else {
                Rect::from_ltrb(left, top, right, bottom)
            };
        }
        if begin
            .clips
            .as_ref()
            .is_some_and(|clip| clip.contains_rect(bounds))
        {
            begin.clips = None;
        }
        recording.bytes = recording
            .bytes
            .saturating_sub(before)
            .saturating_add(begin.bytes());
        let draw = Draw {
            primitive: Primitive::EndGroup,
            color: Color::TRANSPARENT,
            transform: Transform::identity(),
            clip: None,
            clips: None,
            bounds,
        };
        recording.push(draw)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_scope_cache_keeps_source_pointer_keys_alive_after_a_control_scope_ends() {
        let mut surface = Surface {
            glyphs: None,
            pixels: None,
            width: 24,
            height: 24,
            scale: 1.0,
            commands: Some(Recording::default()),
            active_clip: None,
            clip_masks: None,
            failure: None,
            groups: Vec::new(),
            relative_clips: BTreeMap::new(),
            isolation_required: false,
            transforms: 0,
            capture: None,
        };
        let bounds = BoxRect {
            x: 0.0,
            y: 0.0,
            width: 24.0,
            height: 24.0,
        };
        surface
            .push_clip(
                bounds,
                Corners([[12.0; 2]; 4]),
                bounds,
                Corners([[12.0; 2]; 4]),
            )
            .unwrap();
        let base = Arc::downgrade(surface.active_clip.as_ref().unwrap());
        surface.begin_group(2, 0.5, bounds).unwrap();
        let inner = BoxRect {
            x: 1.0,
            y: 1.0,
            width: 22.0,
            height: 22.0,
        };
        surface
            .push_clip(
                inner,
                Corners([[11.0; 2]; 4]),
                inner,
                Corners([[11.0; 2]; 4]),
            )
            .unwrap();
        let source = Arc::downgrade(surface.active_clip.as_ref().unwrap());
        let relative = surface
            .relative_clip(surface.active_clip.clone())
            .unwrap()
            .unwrap();
        assert!(relative.parent.is_none());
        assert_eq!(relative.bounds, Rect::from_xywh(1.0, 1.0, 22.0, 22.0));
        drop(relative);
        surface.active_clip = None;
        surface.end_group(2).unwrap();
        assert!(
            source.upgrade().is_some(),
            "temporary control scope is still a cache key"
        );
        assert!(
            base.upgrade().is_some(),
            "source ownership retains its base key too"
        );
        surface.relative_clips.clear();
        assert!(source.upgrade().is_none());
        assert!(base.upgrade().is_none());
    }
}
