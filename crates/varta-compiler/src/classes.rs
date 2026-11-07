//! Class token extraction from the module graph (K8 T1).
//!
//! A build-time stylesheet generator (Tailwind) needs every class name the
//! program can produce. They are read from the parsed program, never from a
//! filesystem walk: every string literal, template literal text, string
//! literal *type* and TSX attribute or text value of every program module,
//! split into tokens on ASCII whitespace exactly as the HTML `class`
//! attribute is (Infra "split a string on ASCII whitespace"; HTML global
//! attribute `class`, a set of space-separated tokens). Engine preludes and
//! text resources are not program code and are skipped.
//!
//! A class name the program *composes* from parts (`` `btn-${size}` ``,
//! `"bg-" + tone`) in a class position (TSX `class`/`className`,
//! `View.setClass`/`View.bindClass`) is resolved through the checker's types
//! (a Varta extension beyond Tailwind's scanner): when every hole that
//! touches the class name has a finite union of string-literal types, every
//! combination is a token (at most [`MAX_CLASS_COMBINATIONS`] per
//! composition). A hole without such a type is a located diagnostic, which a
//! host that generates styles turns into an error.
use crate::modules::{Graph, engine};
use crate::{CompilerError, ModuleLoader, ModuleSource};
use std::collections::{BTreeMap, BTreeSet};
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitWith};

/// The most distinct tokens one scan collects.
pub const MAX_CLASS_TOKENS: usize = 65_536;
/// The longest token kept, in bytes; longer ones are skipped.
pub const MAX_CLASS_TOKEN_BYTES: usize = 4096;
/// The most class names one composition may enumerate.
pub const MAX_CLASS_COMBINATIONS: usize = 4096;

/// ASCII whitespace (Infra): TAB, LF, FF, CR, SPACE.
fn is_ascii_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{c}' | '\r' | ' ')
}

/// The class tokens and class compositions of a module graph.
#[derive(Debug, Default)]
pub struct ClassScan {
    /// Distinct tokens, sorted.
    pub tokens: BTreeSet<String>,
    /// One located diagnostic per composed class name whose parts have no
    /// finite literal type, in module order.
    pub compositions: Vec<CompilerError>,
    /// The identities of the graph's text resources.
    pub text_resources: BTreeSet<std::path::PathBuf>,
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

/// One part of a composed class expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Part {
    Text(String),
    /// An expression whose value is spliced in.
    Hole(Span),
}

/// A composed class expression in a class position: its parts in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Composition {
    pub parts: Vec<Part>,
}

impl Composition {
    /// Holes that touch non-whitespace text or another hole: they build a
    /// class name together with their neighbours.
    fn touching(&self) -> Vec<usize> {
        let touches = |part: Option<&Part>, at_end: bool| match part {
            Some(Part::Hole(_)) => true,
            Some(Part::Text(text)) => {
                let c = if at_end {
                    text.chars().last()
                } else {
                    text.chars().next()
                };
                c.is_some_and(|c| !is_ascii_whitespace(c))
            }
            None => false,
        };
        // Empty texts separate nothing: drop them first.
        let parts: Vec<&Part> = self
            .parts
            .iter()
            .filter(|p| !matches!(p, Part::Text(t) if t.is_empty()))
            .collect();
        let mut out = Vec::new();
        for (index, part) in parts.iter().enumerate() {
            if let Part::Hole(_) = part {
                let before = index.checked_sub(1).map(|i| parts[i]);
                let after = parts.get(index + 1).copied();
                if touches(before, true) || touches(after, false) {
                    out.push(
                        self.parts
                            .iter()
                            .position(|p| std::ptr::eq(p, *part))
                            .unwrap_or_default(),
                    );
                }
            }
        }
        out
    }
}

/// The key of a hole across the graph: its file and span.
pub(crate) type HoleKey = (String, u32, u32);

/// What the checker found for one watched hole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HoleType {
    /// A finite set of string literals.
    Finite(BTreeSet<String>),
    /// Anything else (`string`, a number, a union with non-literals).
    Open,
}

/// Holes watched during lowering and the types observed for them.
#[derive(Clone, Debug, Default)]
pub(crate) struct HoleTracker {
    pub watched: BTreeSet<HoleKey>,
    pub observed: BTreeMap<HoleKey, HoleType>,
}

impl HoleTracker {
    pub(crate) fn observe(&mut self, key: HoleKey, ty: HoleType) {
        if !self.watched.contains(&key) {
            return;
        }
        let merged = match (self.observed.remove(&key), ty) {
            (None, ty) => ty,
            (Some(HoleType::Finite(mut a)), HoleType::Finite(b)) => {
                a.extend(b);
                HoleType::Finite(a)
            }
            _ => HoleType::Open,
        };
        self.observed.insert(key, merged);
    }
}

fn hole_key(source: &crate::source::Source, span: Span) -> HoleKey {
    (source.file.name.to_string(), span.lo.0, span.hi.0)
}

/// Scan the class tokens of the graph rooted at `entry` (loaded like a
/// compilation: same loader, profile, budgets and TSX desugaring). When a
/// class position composes a name, the program is also type-checked to
/// resolve its holes.
pub fn scan_classes_with_profile(
    entry: ModuleSource,
    loader: &dyn ModuleLoader,
    profile: varta_common::language::LanguageProfile,
) -> Result<ClassScan, CompilerError> {
    let graph = Graph::load_with_profile(entry, loader, profile)?;
    let mut scan = ClassScan::default();
    let mut pending: Vec<(&std::path::Path, Composition)> = Vec::new();
    for path in &graph.order {
        if engine::is_prelude(path) {
            continue;
        }
        let unit = &graph.units[path];
        if unit.text {
            scan.text_resources.insert(path.clone());
            continue;
        }
        let mut literals = Literals::default();
        unit.ast.visit_with(&mut literals);
        for text in &literals.texts {
            scan.add(text)?;
        }
        let mut positions = ClassPositions::default();
        unit.ast.visit_with(&mut positions);
        let mut compositions = unit.class_compositions.clone();
        compositions.extend(positions.compositions);
        for composition in compositions {
            if !composition.touching().is_empty()
                && !pending
                    .iter()
                    .any(|(p, c)| *p == path.as_path() && *c == composition)
            {
                pending.push((path.as_path(), composition));
            }
        }
    }
    if pending.is_empty() {
        return Ok(scan);
    }
    let mut tracker = HoleTracker::default();
    for (path, composition) in &pending {
        let source = &graph.units[*path].source;
        for index in composition.touching() {
            if let Part::Hole(span) = composition.parts[index] {
                tracker.watched.insert(hole_key(source, span));
            }
        }
    }
    let tracker = crate::lower::lower_tracked(&graph, profile, tracker)?;
    for (path, composition) in &pending {
        let source = &graph.units[*path].source;
        match enumerate(composition, source, &tracker) {
            Ok(names) => {
                for name in names {
                    scan.add(&name)?;
                }
            }
            Err(error) => scan.compositions.push(error),
        }
    }
    Ok(scan)
}

/// Every class text a composition can produce, or the located error of its
/// first hole without a finite literal type.
fn enumerate(
    composition: &Composition,
    source: &crate::source::Source,
    tracker: &HoleTracker,
) -> Result<Vec<String>, CompilerError> {
    let touching = composition.touching();
    let mut texts = vec![String::new()];
    for (index, part) in composition.parts.iter().enumerate() {
        match part {
            Part::Text(text) => texts.iter_mut().for_each(|t| t.push_str(text)),
            // A hole separated by whitespace is its own token(s): its
            // literals are scanned on their own.
            Part::Hole(_) if !touching.contains(&index) => {
                texts.iter_mut().for_each(|t| t.push(' '));
            }
            Part::Hole(span) => {
                let values = match tracker.observed.get(&hole_key(source, *span)) {
                    Some(HoleType::Finite(values)) => values,
                    _ => {
                        return Err(source.locate(
                            CompilerError::Type(
                                "a class name is composed at run time from a value whose type is not a finite union of string literals, so the build-time style generator cannot enumerate it; give the value a literal union type (`\"sm\" | \"lg\"`), write complete class names (`cond ? \"btn-lg\" : \"btn-sm\"`) or list them with `@source inline(…)`".into(),
                            ),
                            *span,
                        ));
                    }
                };
                if texts.len().saturating_mul(values.len()) > MAX_CLASS_COMBINATIONS {
                    return Err(source.locate(
                        CompilerError::Type(format!(
                            "this composed class name has more than {MAX_CLASS_COMBINATIONS} combinations; narrow the literal types of its parts"
                        )),
                        *span,
                    ));
                }
                texts = texts
                    .iter()
                    .flat_map(|t| values.iter().map(move |v| format!("{t}{v}")))
                    .collect();
            }
        }
    }
    Ok(texts)
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
        self.texts.push(tpl_text(element));
    }
    fn visit_jsx_text(&mut self, text: &ast::JSXText) {
        if let Some(value) = text.value.as_str() {
            self.texts.push(value.to_owned());
        }
    }
}

/// Compositions in `View.setClass`/`View.bindClass` arguments.
#[derive(Default)]
struct ClassPositions {
    compositions: Vec<Composition>,
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

/// The compositions in a class expression: template literals and `+`
/// concatenation chains (outermost chain only), with nested functions
/// excluded except an arrow's expression body (a `bindClass` getter).
pub(crate) fn compositions(expr: &ast::Expr) -> Vec<Composition> {
    let mut finder = Compositions::default();
    expr.visit_with(&mut finder);
    finder
        .found
        .into_iter()
        .filter(|c| c.parts.iter().any(|p| matches!(p, Part::Hole(_))))
        .collect()
}

#[derive(Default)]
struct Compositions {
    found: Vec<Composition>,
}

fn tpl_text(element: &ast::TplElement) -> String {
    element
        .cooked
        .as_ref()
        .and_then(|cooked| cooked.as_str())
        .map_or_else(|| element.raw.to_string(), str::to_owned)
}

fn is_add(expr: &ast::Expr) -> bool {
    matches!(expr, ast::Expr::Bin(binary) if binary.op == ast::BinaryOp::Add)
}

/// Flatten an expression into parts: literals are text, templates and `+`
/// chains are spliced, anything else is a hole (returned for visiting).
fn flatten<'a>(expr: &'a ast::Expr, parts: &mut Vec<Part>, holes: &mut Vec<&'a ast::Expr>) {
    match expr {
        ast::Expr::Paren(paren) => flatten(&paren.expr, parts, holes),
        ast::Expr::Lit(ast::Lit::Str(text)) => {
            parts.push(Part::Text(
                text.value.as_str().unwrap_or_default().to_owned(),
            ));
        }
        ast::Expr::Tpl(tpl) => {
            for (index, quasi) in tpl.quasis.iter().enumerate() {
                if let Some(hole) = index.checked_sub(1).and_then(|i| tpl.exprs.get(i)) {
                    flatten(hole, parts, holes);
                }
                parts.push(Part::Text(tpl_text(quasi)));
            }
        }
        ast::Expr::Bin(binary) if binary.op == ast::BinaryOp::Add => {
            flatten(&binary.left, parts, holes);
            flatten(&binary.right, parts, holes);
        }
        other => {
            parts.push(Part::Hole(other.span()));
            holes.push(other);
        }
    }
}

impl Compositions {
    fn composed(&mut self, expr: &ast::Expr) {
        let mut parts = Vec::new();
        let mut holes = Vec::new();
        flatten(expr, &mut parts, &mut holes);
        self.found.push(Composition { parts });
        for hole in holes {
            hole.visit_with(self);
        }
    }
}

impl Visit for Compositions {
    fn visit_expr(&mut self, expr: &ast::Expr) {
        match expr {
            ast::Expr::Tpl(_) => self.composed(expr),
            _ if is_add(expr) => self.composed(expr),
            _ => expr.visit_children_with(self),
        }
    }
    // A nested function is not part of the class value.
    fn visit_function(&mut self, _: &ast::Function) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;
    use crate::source::Source;

    fn touching_of(code: &str) -> Vec<usize> {
        let source = Source::new(std::path::Path::new("x.ts"), code);
        let module = parse::parse_module(&source, false).unwrap();
        let mut positions = ClassPositions::default();
        module.visit_with(&mut positions);
        positions
            .compositions
            .iter()
            .map(|c| c.touching().len())
            .collect()
    }

    #[test]
    fn compositions_in_class_positions() {
        assert_eq!(touching_of("View.setClass(n, `btn-${size}`);"), vec![1]);
        assert_eq!(touching_of("View.setClass(n, `${base}-lg p-2`);"), vec![1]);
        assert_eq!(
            touching_of("View.setClass(n, `p-2 ${active ? \"a\" : \"b\"}`);"),
            vec![0]
        );
        assert_eq!(touching_of("View.setClass(n, \"bg-\" + tone);"), vec![1]);
        assert_eq!(
            touching_of("View.setClass(n, \"bg-\" + tone + \"-500\");"),
            vec![1]
        );
        assert_eq!(touching_of("View.setClass(n, \"p-2 \" + extra);"), vec![0]);
        assert_eq!(touching_of("View.setClass(n, `${a}${b}`);"), vec![2]);
        assert_eq!(touching_of("View.bindClass(n, () => `w-${n}`);"), vec![1]);
        assert_eq!(
            touching_of("const s = `w-${n}`; View.setClass(n, s);"),
            Vec::<usize>::new()
        );
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
