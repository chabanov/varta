// Copyright 2026 the Varta Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Decoration edges contribute advance without becoming independent words.
use super::{
    data::{LayoutData, LayoutItemKind},
    ContentWidths,
};
use crate::{analysis::Boundary, style::Brush, InlineBoxKind, OverflowWrap, TextWrapMode};

impl<B: Brush> LayoutData<B> {
    pub(crate) fn inline_edge_widths(&self) -> ContentWidths {
        let mut min = 0.0_f32;
        let mut max = 0.0_f32;
        let mut word = 0.0;
        let mut line = 0.0;
        let mut opening = 0.0;
        let mut whitespace = 0.0;
        let mut wrap = TextWrapMode::Wrap;
        // An in-flow (atomic) box and its closing edges form a word; the
        // next text, opening edge or box starts another.
        let mut atom = false;
        for item in &self.items {
            if atom
                && !(item.kind == LayoutItemKind::InlineBox
                    && self.inline_boxes[item.index].kind == InlineBoxKind::InlineEnd)
            {
                atom = false;
                min = min.max(word);
                word = 0.0;
            }
            match item.kind {
                LayoutItemKind::TextRun => {
                    let run = &self.runs[item.index];
                    for cluster in &self.clusters[run.cluster_range.clone()] {
                        let style = &self.styles[cluster.style_index as usize];
                        let boundary = cluster.info.boundary();
                        if boundary == Boundary::Mandatory
                            || (wrap == TextWrapMode::Wrap
                                && (boundary == Boundary::Line
                                    || style.overflow_wrap == OverflowWrap::Anywhere))
                        {
                            min = min.max(word - whitespace);
                            word = 0.0;
                            if boundary == Boundary::Mandatory {
                                max = max.max(line - opening - whitespace);
                                line = opening;
                            }
                        }
                        word += opening + cluster.advance;
                        opening = 0.0;
                        line += cluster.advance;
                        whitespace = if cluster.info.whitespace().is_space_or_nbsp() {
                            cluster.advance
                        } else {
                            0.0
                        };
                        wrap = style.text_wrap_mode;
                    }
                }
                LayoutItemKind::InlineBox => {
                    let edge = &self.inline_boxes[item.index];
                    match edge.kind {
                        InlineBoxKind::InlineStart => {
                            opening += edge.width;
                            line += edge.width;
                        }
                        InlineBoxKind::InlineEnd => {
                            word += opening + edge.width;
                            opening = 0.0;
                            line += edge.width;
                            whitespace = 0.0;
                        }
                        InlineBoxKind::InFlow => {
                            line += edge.width;
                            if wrap == TextWrapMode::Wrap {
                                // The opening edges right before it join it.
                                min = min.max(word - whitespace);
                                word = opening + edge.width;
                                atom = true;
                            } else {
                                word += opening + edge.width;
                            }
                            opening = 0.0;
                            whitespace = 0.0;
                        }
                        _ => {}
                    }
                }
            }
        }
        // A final empty inline with opening width is visible content, rather
        // than trailing collapsible text. Negative ends must be included before
        // committing an unbreakable width, not after an earlier maximum.
        let trailing = if opening != 0.0 { 0.0 } else { whitespace };
        ContentWidths {
            min: min.max(word + opening - trailing),
            max: max.max(line - trailing),
        }
    }
}
