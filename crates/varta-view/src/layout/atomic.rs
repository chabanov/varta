//! Bottom-up inline-block/inline-flex preparation. Each subtree is moved into
//! its parent once; no recursive layout entry or per-atom shaping allowance.
use super::{BoxGeometry, BoxRect, Environment, LayoutError, ROOT_NODE, TextGeometry, Tree, Units};
use crate::{TextMeasurer, layout::replaced::AtomBox};
use std::collections::BTreeMap;
use taffy::MaybeResolve;
use varta_common::view::CssProperty as P;

#[derive(Clone, Copy)]
pub(crate) struct Info {
    pub image: AtomBox,
    pub baseline: Option<f32>,
}
struct Prepared {
    info: Info,
    data: TextGeometry,
}
pub(crate) fn is_atomic(tree: &Tree, env: &Environment, id: u32) -> bool {
    tree.computed_property(id, env, P::Display)
        .is_some_and(|v| matches!(v.as_str(), "inline-block" | "inline-flex"))
        && !crate::inline::blockified(tree, env, id)
}
pub(super) fn layout(
    tree: &Tree,
    env: &Environment,
    (width, height): (f32, f32),
    (measurer, size): (&mut TextMeasurer, f32),
) -> Result<TextGeometry, LayoutError> {
    let env = &Environment {
        width: f64::from(width),
        height: f64::from(height),
        font: varta_style::font::FontBase {
            medium: f64::from(size),
            fixed_monospace: measurer.fixed_monospace(),
        },
        ..*env
    };
    let mut order = Vec::new();
    let mut pending = vec![(ROOT_NODE, 0)];
    while let Some((id, depth)) = pending.pop() {
        if depth > super::MAX_LAYOUT_DEPTH || order.len() >= super::MAX_LAYOUT_NODES {
            return Err(LayoutError::Limit);
        }
        measurer
            .charge_inline_work(1)
            .map_err(|_| LayoutError::Limit)?;
        if tree.computed_property(id, env, P::Display).as_deref() == Some("none")
            || tree.closed_dialog(id)
        {
            continue;
        }
        order.push(id);
        pending.extend(tree.nodes[&id].children.iter().map(|id| (*id, depth + 1)));
    }
    let mut prepared = BTreeMap::<u32, Prepared>::new();
    for root in order
        .into_iter()
        .rev()
        .filter(|id| *id != ROOT_NODE && is_atomic(tree, env, *id))
        .chain([ROOT_NODE])
    {
        measurer
            .charge_inline_work(prepared.len())
            .map_err(|_| LayoutError::Limit)?;
        if root != ROOT_NODE
            && (tree
                .computed_property(root, env, P::Position)
                .is_some_and(|v| v != "static")
                || crate::transform::effective(&tree.computed_read(root, env)).is_some())
        {
            return Err(LayoutError::Unsupported(root));
        }
        let available = if root == ROOT_NODE {
            width
        } else {
            available(tree, env, root, width)?
        };
        let boxes = prepared.iter().map(|(id, p)| (*id, p.info)).collect();
        let mut data = super::geometry_and_text_core(
            tree,
            env,
            available,
            height,
            measurer,
            size,
            (root, &boxes),
        )?;
        // Only direct prepared atoms occur in this pass's geometry. Descendant
        // contexts were consumed by their own parent on an earlier iteration.
        let children: Vec<_> = prepared
            .keys()
            .filter(|id| data.0.contains_key(id))
            .copied()
            .collect();
        for id in children {
            let child = prepared.remove(&id).ok_or(LayoutError::Limit)?;
            let target = data.0[&id];
            merge(&mut data, id, child.data, target)?;
        }
        if root == ROOT_NODE {
            return Ok(data);
        }
        let bounds = data.0[&root];
        let properties = super::layout_properties(tree, env, root)?;
        let units = Units::new(env).ok_or(LayoutError::RootFont)?;
        let mut margin = [0.0; 4];
        for (slot, property) in
            margin
                .iter_mut()
                .zip([P::MarginTop, P::MarginRight, P::MarginBottom, P::MarginLeft])
        {
            if let Some(value) = properties.get(&property) {
                *slot = units
                    .margin(value)
                    .ok_or(LayoutError::Unsupported(root))?
                    .maybe_resolve(Some(available), crate::calc::resolve)
                    .unwrap_or(0.0);
            }
        }
        let inset = [
            bounds.content.y - bounds.rect.y,
            bounds.rect.x + bounds.rect.width - bounds.content.x - bounds.content.width,
            bounds.rect.y + bounds.rect.height - bounds.content.y - bounds.content.height,
            bounds.content.x - bounds.rect.x,
        ];
        let baseline = baseline(tree, env, root, &data, measurer, size)?;
        prepared.insert(
            root,
            Prepared {
                info: Info {
                    image: AtomBox {
                        size: (bounds.rect.width, bounds.rect.height),
                        margin,
                        inset,
                    },
                    baseline,
                },
                data,
            },
        );
    }
    Err(LayoutError::Limit)
}

/// Definite normal-flow containing widths. Reject unresolved flex/grid item
/// distribution instead of measuring an atomic subtree at the viewport width.
fn available(tree: &Tree, env: &Environment, root: u32, viewport: f32) -> Result<f32, LayoutError> {
    let units = Units::new(env).ok_or(LayoutError::RootFont)?;
    let mut width = viewport;
    let mut parent_flex = false;
    let mut unknown = false;
    for id in tree
        .path(root)
        .into_iter()
        .skip(1)
        .rev()
        .filter(|id| *id != ROOT_NODE)
    {
        let properties = super::layout_properties(tree, env, id)?;
        let basis = width;
        let display = properties
            .get(&P::Display)
            .map(String::as_str)
            .unwrap_or("block");
        if display == "inline"
            || (crate::inline::inline_tag(
                tree.element(id)
                    .map_or(varta_common::view::Tag::Div, |e| e.tag),
            ) && !properties.contains_key(&P::Display))
        {
            continue;
        }
        let specified = properties.get(&P::Width).filter(|v| v.as_str() != "auto");
        if let Some(value) = specified {
            if unknown && value.contains('%') {
                return Err(LayoutError::Unsupported(root));
            }
            width = units
                .margin(value)
                .and_then(|v| v.maybe_resolve(Some(basis), crate::calc::resolve))
                .ok_or(LayoutError::Unsupported(root))?;
            unknown = false;
        } else if parent_flex {
            unknown = true;
        }
        if unknown {
            parent_flex = matches!(display, "flex" | "inline-flex" | "grid");
            continue;
        }
        let mut sides = 0.0;
        for property in [P::PaddingLeft, P::PaddingRight] {
            if let Some(value) = properties.get(&property) {
                sides += units
                    .margin(value)
                    .and_then(|v| v.maybe_resolve(Some(basis), crate::calc::resolve))
                    .ok_or(LayoutError::Unsupported(root))?;
            }
        }
        let borders = super::border_widths(tree, env, id)?;
        sides += borders[1] + borders[3];
        if specified.is_none()
            || properties
                .get(&P::BoxSizing)
                .is_some_and(|v| v == "border-box")
        {
            width -= sides;
        }
        if specified.is_none() {
            for property in [P::MarginLeft, P::MarginRight] {
                if let Some(value) = properties.get(&property) {
                    width -= units
                        .margin(value)
                        .and_then(|v| v.maybe_resolve(Some(basis), crate::calc::resolve))
                        .unwrap_or(0.0);
                }
            }
        }
        for (property, minimum) in [(P::MaxWidth, false), (P::MinWidth, true)] {
            if let Some(value) = properties
                .get(&property)
                .filter(|v| !matches!(v.as_str(), "auto" | "none"))
            {
                let mut limit = units
                    .margin(value)
                    .and_then(|v| v.maybe_resolve(Some(basis), crate::calc::resolve))
                    .ok_or(LayoutError::Unsupported(root))?;
                if properties
                    .get(&P::BoxSizing)
                    .is_some_and(|v| v == "border-box")
                {
                    limit = (limit - sides).max(0.0);
                }
                width = if minimum {
                    width.max(limit)
                } else {
                    width.min(limit)
                };
            }
        }
        parent_flex = matches!(display, "flex" | "inline-flex" | "grid");
    }
    if unknown || !width.is_finite() || width <= 0.0 {
        Err(LayoutError::Unsupported(root))
    } else {
        Ok(width)
    }
}

fn baseline(
    tree: &Tree,
    env: &Environment,
    root: u32,
    data: &TextGeometry,
    measurer: &mut TextMeasurer,
    size: f32,
) -> Result<Option<f32>, LayoutError> {
    let properties = tree.computed_read(root, env);
    if crate::style::overflow(&properties)
        .is_none_or(|axes| axes.iter().any(|v| *v != crate::style::Overflow::Visible))
    {
        return Ok(None);
    }
    let flex = properties
        .get(&P::Display)
        .is_some_and(|v| v == "inline-flex");
    let mut baseline = None;
    for id in tree.subtree_order(root) {
        if tree
            .path(id)
            .into_iter()
            .take_while(|id| *id != root)
            .any(|id| {
                tree.computed_property(id, env, P::Position)
                    .is_some_and(|p| matches!(p.as_str(), "absolute" | "fixed"))
            })
        {
            continue;
        }
        if let Some(frame) = data.2.get(&id) {
            let index = if flex {
                0
            } else {
                frame.shaped.layout.len().saturating_sub(1)
            };
            if let Some(line) = frame.shaped.layout.lines().nth(index) {
                baseline = Some(
                    frame.bounds.rect.y + line.metrics().baseline + frame.shaped.offsets[index]
                        - data.0[&root].rect.y,
                );
                if flex {
                    break;
                }
            }
        } else if let Some(shape) = data.1.get(&id).filter(|s| s.rich.is_none()) {
            let rect = data.0[&id].rect;
            let (_, layout) = measurer
                .shape_styled(
                    &shape.text,
                    Some(rect.width.max(0.01)),
                    &shape.families,
                    shape.options,
                )
                .map_err(super::LayoutError::text(id))?;
            let parent = tree.nodes[&id].parent.unwrap_or(root);
            let above = crate::inline::strut(tree, env, parent, (measurer, size))?.0;
            if let (Some(first), Some(last)) = (layout.lines().next(), layout.lines().last()) {
                baseline = Some(
                    rect.y
                        + above
                        + if flex {
                            0.0
                        } else {
                            last.metrics().baseline - first.metrics().baseline
                        }
                        - data.0[&root].rect.y,
                );
                if flex {
                    break;
                }
            }
        }
    }
    Ok(baseline)
}
fn shifted(mut r: BoxRect, delta: (f32, f32)) -> BoxRect {
    r.x += delta.0;
    r.y += delta.1;
    r
}
fn clip(old: BoxRect, base: BoxRect, target: BoxRect, delta: (f32, f32)) -> BoxRect {
    let left = if old.x == base.x {
        target.x
    } else {
        old.x + delta.0
    };
    let top = if old.y == base.y {
        target.y
    } else {
        old.y + delta.1
    };
    let right = if old.x + old.width == base.x + base.width {
        target.x + target.width
    } else {
        old.x + old.width + delta.0
    };
    let bottom = if old.y + old.height == base.y + base.height {
        target.y + target.height
    } else {
        old.y + old.height + delta.1
    };
    super::intersect(
        target,
        BoxRect {
            x: left,
            y: top,
            width: (right - left).max(0.0),
            height: (bottom - top).max(0.0),
        },
    )
}
fn merge(
    parent: &mut TextGeometry,
    root: u32,
    mut child: TextGeometry,
    target: BoxGeometry,
) -> Result<(), LayoutError> {
    let source = child.0[&root];
    let delta = (target.rect.x - source.rect.x, target.rect.y - source.rect.y);
    let map = |mut g: BoxGeometry| {
        if let Some(t) = &mut g.transform {
            t.x += delta.0 - t.scale_x * delta.0 - t.skew_x * delta.1;
            t.y += delta.1 - t.skew_y * delta.0 - t.scale_y * delta.1;
        }
        g.rect = shifted(g.rect, delta);
        g.content = shifted(g.content, delta);
        g.clip = clip(g.clip, source.clip, target.clip, delta);
        if let Some(s) = &mut g.scrollbars {
            s.vertical = s.vertical.map(|r| shifted(r, delta));
            s.horizontal = s.horizontal.map(|r| shifted(r, delta));
        }
        if let Some(s) = &mut g.scroll {
            s.viewport = shifted(s.viewport, delta);
        }
        if let Some(s) = &mut g.programmatic_scroll {
            s.viewport = shifted(s.viewport, delta);
        }
        g
    };
    for (id, g) in child.0 {
        parent.0.insert(id, map(g));
    }
    for frame in child.2.values_mut() {
        frame.bounds = map(frame.bounds);
        for r in frame.fragments.values_mut().flatten() {
            *r = shifted(*r, delta);
        }
    }
    parent.1.extend(child.1);
    parent.2.extend(child.2);
    let fragments: usize = parent
        .2
        .values()
        .flat_map(|f| f.fragments.values())
        .map(Vec::len)
        .sum();
    if parent.0.len() > super::MAX_LAYOUT_NODES || fragments > crate::inline::MAX_FRAGMENT_COUNT {
        return Err(LayoutError::Limit);
    }
    Ok(())
}
