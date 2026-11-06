//! Class token extraction from the module graph (K8 T1).
//!
//! A build-time stylesheet generator (Tailwind) needs every class name the
//! program can produce. They are read from the parsed program, never from a
//! filesystem walk: every string literal, template literal text, string
//! literal *type* (so each member of a finite union of string literals, the
//! type of a `class` expression such as `type Tone = "bg-red-500" |
//! "bg-blue-500"`, is enumerated) and TSX attribute or text value of every
//! program module, split into tokens on ASCII whitespace exactly as the HTML
//! `class` attribute is (HTML §2.3.8 "split a string on ASCII whitespace",
//! §3.2.6 `class`: a set of space-separated tokens). Engine preludes and text
//! resources are not program code and are skipped.
//!
//! A class name the program *composes* from parts (`` `btn-${size}` ``,
//! `"bg-" + tone`) exists only at run time, so no build-time generator can
//! see it; such compositions in a class position (TSX `class`/`className`,
//! `View.setClass`/`View.bindClass`) are reported as located diagnostics,
//! which a host that generates styles turns into errors.
use crate::modules::{Graph, engine};
use crate::{CompilerError, ModuleLoader, ModuleSource};
use std::collections::BTreeSet;
use swc_common::Span;
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitWith};

/// The most distinct tokens one scan collects.
pub const MAX_CLASS_TOKENS: usize = 65_536;
/// The longest token kept, in bytes; longer ones are skipped.
pub const MAX_CLASS_TOKEN_BYTES: usize = 4096;

/// ASCII whitespace (Infra §4.5): TAB, LF, FF, CR, SPACE.
fn is_ascii_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{c}' | '\r' | ' ')
}

/// The class tokens and class compositions of a module graph.
#[derive(Debug, Default)]
pub struct ClassScan {
    /// Distinct tokens, sorted.
    pub tokens: BTreeSet<String>,
    /// One located diagnostic per class name composed at run time, in module
    /// order.
    pub compositions: Vec<CompilerError>,
}

impl ClassScan {
    fn add(&mut self, text: &str) -> Result<(), CompilerError> {
        for token in text.split(is_ascii_whitespace) {
            if token.is_empty() || token.len() > MAX_CLASS_TOKEN_BYTES {
                continue;
            }
            if !self.tokens.contains(token) {
                if self.tokens.len() >= MAX_CLASS_TOKENS {
                    return Err(CompilerError::Module(format!(
                        "the program has more than {MAX_CLASS_TOKENS} distinct class tokens"
                    )));
                }
                self.tokens.insert(token.to_owned());
            }
        }
        Ok(())
    }
}

/// Scan the class tokens of the graph rooted at `entry` (loaded like a
/// compilation: same loader, profile, budgets and TSX desugaring).
pub fn scan_classes_with_profile(
    entry: ModuleSource,
    loader: &dyn ModuleLoader,
    profile: varta_common::language::LanguageProfile,
) -> Result<ClassScan, CompilerError> {
    let graph = Graph::load_with_profile(entry, loader, profile)?;
    let mut scan = ClassScan::default();
    for path in &graph.order {
        if engine::is_prelude(path) {
            continue;
        }
        let unit = &graph.units[path];
        if unit.text {
            continue;
        }
        let mut literals = Literals::default();
        unit.ast.visit_with(&mut literals);
        for text in &literals.texts {
            scan.add(text)?;
        }
        let mut spans = unit.class_compositions.clone();
        let mut positions = ClassPositions::default();
        unit.ast.visit_with(&mut positions);
        spans.extend(positions.compositions);
        spans.sort_by_key(|span| (span.lo, span.hi));
        spans.dedup();
        for span in spans {
            scan.compositions.push(unit.source.locate(
                CompilerError::Type(
                    "a class name composed at run time cannot be seen by the build-time style generator; write each class name as a complete string literal (for example `cond ? \"btn-lg\" : \"btn-sm\"`) or list it with `@source inline(…)`".into(),
                ),
                span,
            ));
        }
    }
    Ok(scan)
}

/// Every string literal value, template text and string literal type.
#[derive(Default)]
struct Literals {
    texts: Vec<String>,
}

impl Visit for Literals {
    fn visit_str(&mut self, text: &ast::Str) {
        if let Some(value) = text.value.as_str() {
            self.texts.push(value.to_owned());
        }
    }
    fn visit_tpl_element(&mut self, element: &ast::TplElement) {
        match element.cooked.as_ref().and_then(|cooked| cooked.as_str()) {
            Some(cooked) => self.texts.push(cooked.to_owned()),
            None => self.texts.push(element.raw.to_string()),
        }
    }
    fn visit_jsx_text(&mut self, text: &ast::JSXText) {
        if let Some(value) = text.value.as_str() {
            self.texts.push(value.to_owned());
        }
    }
}

/// Class compositions in `View.setClass`/`View.bindClass` arguments.
#[derive(Default)]
struct ClassPositions {
    compositions: Vec<Span>,
}

const VIEW_OBJECT: &str = engine::VIEW_OBJECT;
const CLASS_MEMBERS: &[&str] = &["setClass", "bindClass"];
/// The class argument of `View.setClass(node, classes)`.
const CLASS_ARGUMENT: usize = 1;

impl Visit for ClassPositions {
    fn visit_call_expr(&mut self, call: &ast::CallExpr) {
        if let ast::Callee::Expr(callee) = &call.callee
            && let ast::Expr::Member(member) = &**callee
            && let ast::Expr::Ident(object) = &*member.obj
            && object.sym == VIEW_OBJECT
            && let ast::MemberProp::Ident(property) = &member.prop
            && CLASS_MEMBERS.contains(&property.sym.as_ref())
            && let Some(argument) = call.args.get(CLASS_ARGUMENT)
        {
            self.compositions.extend(compositions(&argument.expr));
        }
        call.visit_children_with(self);
    }
}

/// Spans of class-name compositions in a class expression: a template hole
/// or a concatenated operand that touches non-whitespace literal text.
pub(crate) fn compositions(expr: &ast::Expr) -> Vec<Span> {
    let mut finder = Compositions::default();
    expr.visit_with(&mut finder);
    finder.spans
}

#[derive(Default)]
struct Compositions {
    spans: Vec<Span>,
}

fn touches_end(text: &str) -> bool {
    text.chars().last().is_some_and(|c| !is_ascii_whitespace(c))
}

fn touches_start(text: &str) -> bool {
    text.chars().next().is_some_and(|c| !is_ascii_whitespace(c))
}

fn tpl_text(element: &ast::TplElement) -> String {
    element
        .cooked
        .as_ref()
        .and_then(|cooked| cooked.as_str())
        .map_or_else(|| element.raw.to_string(), str::to_owned)
}

/// A string literal operand of `+` (directly, not a nested expression).
fn literal_text(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str().map(str::to_owned),
        ast::Expr::Paren(paren) => literal_text(&paren.expr),
        _ => None,
    }
}

impl Visit for Compositions {
    fn visit_tpl(&mut self, tpl: &ast::Tpl) {
        for (index, hole) in tpl.exprs.iter().enumerate() {
            let before = tpl.quasis.get(index).map(tpl_text).unwrap_or_default();
            let after = tpl.quasis.get(index + 1).map(tpl_text).unwrap_or_default();
            if touches_end(&before) || touches_start(&after) {
                self.spans.push(swc_common::Spanned::span(&**hole));
            }
        }
        tpl.visit_children_with(self);
    }
    fn visit_bin_expr(&mut self, binary: &ast::BinExpr) {
        if binary.op == ast::BinaryOp::Add {
            let left = literal_text(&binary.left);
            let right = literal_text(&binary.right);
            let composed = match (&left, &right) {
                (Some(left), None) => touches_end(left),
                (None, Some(right)) => touches_start(right),
                _ => false,
            };
            if composed {
                self.spans.push(binary.span);
            }
        }
        binary.visit_children_with(self);
    }
    // A nested function is not part of the class value.
    fn visit_function(&mut self, _: &ast::Function) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use crate::source::Source;

    fn spans_of(code: &str) -> usize {
        let source = Source::new(std::path::Path::new("x.ts"), code);
        let module = parse::parse_module(&source, false).unwrap();
        let mut positions = ClassPositions::default();
        module.visit_with(&mut positions);
        positions.compositions.len()
    }

    #[test]
    fn compositions_in_class_positions() {
        assert_eq!(spans_of("View.setClass(n, `btn-${size}`);"), 1);
        assert_eq!(spans_of("View.setClass(n, `${base}-lg p-2`);"), 1);
        assert_eq!(
            spans_of("View.setClass(n, `p-2 ${active ? \"a\" : \"b\"}`);"),
            0
        );
        assert_eq!(spans_of("View.setClass(n, \"bg-\" + tone);"), 1);
        assert_eq!(spans_of("View.setClass(n, \"p-2 \" + extra);"), 0);
        assert_eq!(spans_of("View.bindClass(n, () => `w-${n}`);"), 1);
        assert_eq!(spans_of("const s = `w-${n}`; View.setClass(n, s);"), 0);
    }

    #[test]
    fn tokens_split_on_ascii_whitespace() {
        let mut scan = ClassScan::default();
        scan.add("flex\tp-4\nhover:bg-red-500  w-1/2\u{a0}x")
            .unwrap();
        let tokens: Vec<&str> = scan.tokens.iter().map(String::as_str).collect();
        // U+00A0 is not ASCII whitespace: it stays inside a token.
        assert_eq!(
            tokens,
            vec!["flex", "hover:bg-red-500", "p-4", "w-1/2\u{a0}x"]
        );
    }
}
