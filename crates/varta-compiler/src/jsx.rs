//! TSX templates of profile v38 (`tsx()`), compiled ahead of time.
//!
//! Each outermost intrinsic JSX element becomes one generated module function
//! (a *static template*): it creates the element tree with L0 operations
//! through the view prelude's `View.*` members and binds every *dynamic hole*
//! to a parameter. The JSX expression itself becomes a call of that function
//! whose arguments are the holes, in source order:
//!
//! - a child or attribute expression `{expr}` is a getter `() => T` (Solid's
//!   model: the expression is re-evaluated by the reactive runner L1 installs
//!   with `View.setEffect`; without one it runs once). Text and string
//!   attributes take `` () => `${expr}` ``, boolean attributes and `checked`
//!   take `() => expr`;
//! - `on<Event>[:filter]={handler}` takes the handler,
//!   `(event: ViewEvent) => void` (a parameterless arrow gets an unused
//!   event parameter); `ref={fn}` takes `(node: number) => void`;
//! - a component `<Name a={x} />` is the call `Name({ a: x })`, a node id.
//!
//! Intrinsic elements, attributes, CSS properties, event kinds and key
//! filters are checked against the `varta_common::view` registry here,
//! static values against their grammars; hole types are checked by the
//! ordinary type checker against the generated parameter types, so the
//! typed `JSX` namespace of `varta.d.ts` (generated from the same tables) is
//! the contract editors see. The generated code is ordinary checked
//! TypeScript; its nodes carry the span of their JSX element.
use crate::CompilerError;
use crate::source::Source;
use std::fmt::Write as _;
use std::path::Path;
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_visit::{VisitMut, VisitMutWith};
use varta_common::view::{
    ANY_KEY, Attribute, AttributeValue, CssProperty, EventFlag, EventKind, Key, ListenOption,
    NO_NODE, Tag, attributes, css,
};

/// The file extension of TSX modules.
const TSX_EXTENSION: &str = "tsx";
/// Generated names start with this prefix, which program code may not use.
const PREFIX: &str = "__vartaJsx";
/// The object of the view prelude members.
const VIEW: &str = crate::modules::engine::VIEW_OBJECT;

pub(crate) fn is_tsx(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == TSX_EXTENSION)
}

/// Event attribute names and their kinds.
pub(crate) const EVENTS: [(&str, EventKind); 10] = [
    ("onClick", EventKind::Click),
    ("onInput", EventKind::Input),
    ("onChange", EventKind::Change),
    ("onSubmit", EventKind::Submit),
    ("onKeyDown", EventKind::Keydown),
    ("onFocusIn", EventKind::Focusin),
    ("onFocusOut", EventKind::Focusout),
    ("onScroll", EventKind::Scroll),
    ("onCancel", EventKind::Cancel),
    ("onClose", EventKind::Close),
];
/// Modifier names of a key filter and their event flags.
const MODIFIERS: [(&str, &str, EventFlag); 4] = [
    ("shift", "noShift", EventFlag::Shift),
    ("ctrl", "noCtrl", EventFlag::Ctrl),
    ("alt", "noAlt", EventFlag::Alt),
    ("meta", "noMeta", EventFlag::Meta),
];
const PREVENT_DEFAULT: &str = "preventDefault";
const STOP_PROPAGATION: &str = "stopPropagation";
const NOT_COMPOSING: &str = "notComposing";
/// Attribute-like names with their own operations.
const CLASS: &str = "class";
const STYLE: &str = "style";
const VALUE: &str = "value";
const CHECKED: &str = "checked";
const REF: &str = "ref";

fn error(message: impl Into<String>) -> CompilerError {
    CompilerError::Type(message.into())
}

/// Desugar every JSX expression of a `.tsx` module into generated template
/// functions and their calls.
pub(crate) fn desugar(module: &mut ast::Module, source: &Source) -> Result<(), CompilerError> {
    let mut reserved = Reserved(None);
    swc_ecma_visit::VisitWith::visit_with(&*module, &mut reserved);
    if let Some(span) = reserved.0 {
        return Err(source.locate(
            error(format!(
                "names starting with `{PREFIX}` are reserved for TSX templates"
            )),
            span,
        ));
    }
    let mut desugar = Desugar {
        source,
        templates: Vec::new(),
        error: None,
    };
    module.visit_mut_with(&mut desugar);
    if let Some(error) = desugar.error {
        return Err(error);
    }
    for (text, span) in desugar.templates {
        let mut generated = parse_snippet(source, &text)?;
        generated.visit_mut_with(&mut Respan(span));
        module.body.extend(generated.body);
    }
    Ok(())
}

struct Reserved(Option<Span>);
impl swc_ecma_visit::Visit for Reserved {
    fn visit_ident(&mut self, ident: &ast::Ident) {
        if self.0.is_none() && ident.sym.starts_with(PREFIX) {
            self.0 = Some(ident.span);
        }
    }
}

/// Every span of generated code becomes the span of its JSX element.
struct Respan(Span);
impl VisitMut for Respan {
    fn visit_mut_span(&mut self, span: &mut Span) {
        *span = self.0;
    }
}

/// Replace hole placeholders with the program's expressions.
struct Fill {
    holes: Vec<Option<Box<ast::Expr>>>,
}
impl VisitMut for Fill {
    fn visit_mut_expr(&mut self, expr: &mut ast::Expr) {
        if let ast::Expr::Ident(ident) = expr
            && let Some(index) = ident
                .sym
                .strip_prefix(HOLE)
                .and_then(|index| index.parse::<usize>().ok())
            && let Some(hole) = self.holes.get_mut(index).and_then(Option::take)
        {
            *expr = *hole;
            return;
        }
        expr.visit_mut_children_with(self);
    }
}
const HOLE: &str = "__vartaJsxHole";
const EVENT_PARAMETER: &str = "__vartaJsxEvent";

fn parse_snippet(source: &Source, text: &str) -> Result<ast::Module, CompilerError> {
    let path = std::path::PathBuf::from(source.file.name.to_string());
    crate::parse::parse_module(&Source::new(&path, text), false).map_err(|error| {
        CompilerError::Internal(format!("generated TSX template does not parse: {error}"))
    })
}

struct Desugar<'a> {
    source: &'a Source,
    /// Generated template functions and the span of their element.
    templates: Vec<(String, Span)>,
    error: Option<CompilerError>,
}

impl VisitMut for Desugar<'_> {
    fn visit_mut_expr(&mut self, expr: &mut ast::Expr) {
        if self.error.is_some() {
            return;
        }
        let result = match expr {
            ast::Expr::JSXElement(element) => self.element_expr(element),
            ast::Expr::JSXFragment(fragment) => Err(self.source.locate(
                error("JSX fragments are not supported: wrap the children in one element"),
                fragment.span,
            )),
            _ => {
                expr.visit_mut_children_with(self);
                return;
            }
        };
        match result {
            Ok(replacement) => *expr = replacement,
            Err(error) => self.error = Some(error),
        }
    }
}

/// One template under construction.
struct Template {
    body: String,
    /// Generated parameters: `(name, type)`.
    params: Vec<(String, &'static str)>,
    /// Call arguments, as text with hole placeholders.
    args: Vec<String>,
    holes: Vec<Option<Box<ast::Expr>>>,
    nodes: usize,
}
impl Template {
    fn node(&mut self) -> String {
        let name = format!("{PREFIX}Node{}", self.nodes);
        self.nodes += 1;
        let _ = writeln!(self.body, "  const {name}: number = {VIEW}.node();");
        name
    }
    /// A hole: a generated parameter of `ty` whose call argument is
    /// `argument` with the program expression at `{}`.
    fn hole(&mut self, expr: Box<ast::Expr>, ty: &'static str, argument: &str) -> String {
        let index = self.holes.len();
        self.holes.push(Some(expr));
        let param = format!("{PREFIX}Param{}", self.params.len());
        self.params.push((param.clone(), ty));
        self.args
            .push(argument.replace("{}", &format!("{HOLE}{index}")));
        param
    }
    fn line(&mut self, text: String) {
        self.body.push_str("  ");
        self.body.push_str(&text);
        self.body.push_str(";\n");
    }
}

/// A JavaScript string literal.
fn literal(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() || c == '\u{2028}' || c == '\u{2029}' => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// JSX text: lines trimmed (except the outer edges of the first and last
/// line), empty lines dropped, joined by one space (React's rule).
fn jsx_text(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.len().saturating_sub(1);
    let mut parts = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let mut line = line.trim_end_matches('\r');
        if index > 0 {
            line = line.trim_start_matches([' ', '\t']);
        }
        if index < last {
            line = line.trim_end_matches([' ', '\t']);
        }
        if !line.is_empty() {
            parts.push(line);
        }
    }
    parts.join(" ")
}

/// The text of a string literal (`None` for one with a lone surrogate).
fn string(text: &ast::Str) -> Option<String> {
    text.value.as_str().map(str::to_owned)
}

fn element_name(name: &ast::JSXElementName) -> Option<String> {
    match name {
        ast::JSXElementName::Ident(ident) => Some(ident.sym.to_string()),
        _ => None,
    }
}

fn attr_name(attr: &ast::JSXAttr) -> (String, Option<String>) {
    match &attr.name {
        ast::JSXAttrName::Ident(name) => (name.sym.to_string(), None),
        ast::JSXAttrName::JSXNamespacedName(name) => {
            (name.ns.sym.to_string(), Some(name.name.sym.to_string()))
        }
    }
}

/// A component name: an identifier starting with an uppercase letter.
fn is_component(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_uppercase())
}

/// The static value of an attribute: a string literal, or an expression
/// container holding a string literal or a substitution-free template.
fn static_value(value: &ast::JSXAttrValue) -> Option<String> {
    match value {
        ast::JSXAttrValue::Str(text) => string(text),
        ast::JSXAttrValue::JSXExprContainer(container) => match &container.expr {
            ast::JSXExpr::Expr(expr) => match &**expr {
                ast::Expr::Lit(ast::Lit::Str(text)) => string(text),
                ast::Expr::Tpl(template) if template.exprs.is_empty() => template
                    .quasis
                    .first()
                    .and_then(|quasi| quasi.cooked.as_ref())
                    .and_then(|cooked| cooked.as_str().map(str::to_owned)),
                _ => None,
            },
            ast::JSXExpr::JSXEmptyExpr(_) => None,
        },
        _ => None,
    }
}

/// The program expression of an attribute or child container.
fn expression(value: &mut ast::JSXAttrValue) -> Option<Box<ast::Expr>> {
    match value {
        ast::JSXAttrValue::JSXExprContainer(container) => match &mut container.expr {
            ast::JSXExpr::Expr(expr) => Some(std::mem::replace(
                expr,
                Box::new(ast::Expr::Invalid(ast::Invalid {
                    span: container.span,
                })),
            )),
            ast::JSXExpr::JSXEmptyExpr(_) => None,
        },
        _ => None,
    }
}

/// A listener filter parsed from `on<Event>:<part>-<part>`.
#[derive(Default)]
struct Filter {
    options: u32,
    key: u16,
    required: u32,
    forbidden: u32,
}

fn filter(kind: EventKind, parts: Option<&str>) -> Result<Filter, String> {
    let mut filter = Filter {
        key: ANY_KEY,
        ..Filter::default()
    };
    let Some(parts) = parts else {
        return Ok(filter);
    };
    for part in parts.split('-') {
        let keyed = kind == EventKind::Keydown;
        if part == PREVENT_DEFAULT {
            filter.options |= ListenOption::PreventDefault as u32;
        } else if part == STOP_PROPAGATION {
            filter.options |= ListenOption::StopPropagation as u32;
        } else if keyed && part == NOT_COMPOSING {
            filter.options |= ListenOption::NotComposing as u32;
        } else if let Some((_, _, flag)) = MODIFIERS.iter().find(|(held, ..)| *held == part)
            && keyed
        {
            filter.required |= *flag as u32;
        } else if let Some((_, _, flag)) = MODIFIERS.iter().find(|(_, free, _)| *free == part)
            && keyed
        {
            filter.forbidden |= *flag as u32;
        } else if let Some(key) = Key::ALL.iter().find(|key| key.name() == part)
            && keyed
            && filter.key == ANY_KEY
        {
            filter.key = key.code();
        } else {
            return Err(format!(
                "unknown `{}` filter `{part}`{}",
                kind.name(),
                if keyed {
                    ""
                } else {
                    " (keys, modifiers and notComposing filter only onKeyDown)"
                }
            ));
        }
    }
    if filter.required & filter.forbidden != 0 {
        return Err("a modifier cannot be both required and forbidden".into());
    }
    Ok(filter)
}

fn admits(attribute: Attribute, tag: Tag) -> bool {
    match attribute.tags() {
        varta_common::view::Tags::All => true,
        varta_common::view::Tags::Focusable => false,
        varta_common::view::Tags::Only(tags) => tags.contains(&tag),
    }
}

impl Desugar<'_> {
    fn fail(&self, message: impl Into<String>, span: Span) -> CompilerError {
        self.source.locate(error(message), span)
    }

    /// The replacement of an outermost JSX element: a component call, or the
    /// call of its generated template.
    fn element_expr(&mut self, element: &mut ast::JSXElement) -> Result<ast::Expr, CompilerError> {
        let span = element.span;
        let mut template = Template {
            body: String::new(),
            params: Vec::new(),
            args: Vec::new(),
            holes: Vec::new(),
            nodes: 0,
        };
        let root = self.child(element, &mut template)?;
        let call = match root {
            Child::Node(root) => {
                let name = format!("{PREFIX}{}", self.templates.len());
                let params: Vec<String> = template
                    .params
                    .iter()
                    .map(|(name, ty)| format!("{name}: {ty}"))
                    .collect();
                self.templates.push((
                    format!(
                        "function {name}({}): number {{\n{}  return {root};\n}}\n",
                        params.join(", "),
                        template.body
                    ),
                    span,
                ));
                format!("{name}({})", template.args.join(", "))
            }
            Child::Component(text) => text,
        };
        let mut module = parse_snippet(self.source, &format!("const {PREFIX}Value = {call};"))?;
        module.visit_mut_with(&mut Respan(span));
        let mut fill = Fill {
            holes: template.holes,
        };
        let ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(var))) = module
            .body
            .pop()
            .ok_or_else(|| CompilerError::Internal("empty TSX call".into()))?
        else {
            return Err(CompilerError::Internal("unexpected TSX call".into()));
        };
        let mut expr = var
            .decls
            .into_iter()
            .next()
            .and_then(|declarator| declarator.init)
            .ok_or_else(|| CompilerError::Internal("missing TSX call".into()))?;
        expr.visit_mut_with(&mut fill);
        Ok(*expr)
    }

    /// A hole expression, with nested JSX desugared first.
    fn take(&mut self, mut expr: Box<ast::Expr>) -> Result<Box<ast::Expr>, CompilerError> {
        self.visit_mut_expr(&mut expr);
        match self.error.take() {
            Some(error) => Err(error),
            None => Ok(expr),
        }
    }

    fn child(
        &mut self,
        element: &mut ast::JSXElement,
        template: &mut Template,
    ) -> Result<Child, CompilerError> {
        let span = element.opening.span;
        let name = element_name(&element.opening.name)
            .ok_or_else(|| self.fail("JSX element names must be identifiers", span))?;
        if is_component(&name) {
            return self
                .component(&name, element, template)
                .map(Child::Component);
        }
        let tag = Tag::ALL
            .iter()
            .copied()
            .find(|tag| tag.name() == name)
            .ok_or_else(|| {
                self.fail(
                    format!("unknown intrinsic element `<{name}>` (not in the L0 view subset)"),
                    span,
                )
            })?;
        let node = template.node();
        template.line(format!("{VIEW}.createElement({node}, {})", tag.code()));
        let mut refs = Vec::new();
        for attr in &mut element.opening.attrs {
            let ast::JSXAttrOrSpread::JSXAttr(attr) = attr else {
                return Err(self.fail("spread attributes are not supported", span));
            };
            if let Some(param) = self.attribute(tag, &node, attr, template)? {
                refs.push(param);
            }
        }
        for child in &mut element.children {
            match child {
                ast::JSXElementChild::JSXText(text) => {
                    let text = text.value.as_str().map(jsx_text).unwrap_or_default();
                    if !text.is_empty() {
                        let id = template.node();
                        template.line(format!("{VIEW}.createText({id}, {})", literal(&text)));
                        template.line(format!("{VIEW}.insert({node}, {id}, {NO_NODE})"));
                    }
                }
                ast::JSXElementChild::JSXExprContainer(container) => {
                    let ast::JSXExpr::Expr(expr) = &mut container.expr else {
                        continue;
                    };
                    let expr = std::mem::replace(
                        expr,
                        Box::new(ast::Expr::Invalid(ast::Invalid {
                            span: container.span,
                        })),
                    );
                    if let ast::Expr::Lit(ast::Lit::Str(text)) = &*expr {
                        let id = template.node();
                        let text = string(text).unwrap_or_default();
                        template.line(format!("{VIEW}.createText({id}, {})", literal(&text)));
                        template.line(format!("{VIEW}.insert({node}, {id}, {NO_NODE})"));
                        continue;
                    }
                    let expr = self.take(expr)?;
                    let param = template.hole(expr, "() => string", "(): string => `${{}}`");
                    let id = template.node();
                    template.line(format!("{VIEW}.createText({id}, \"\")"));
                    template.line(format!("{VIEW}.bindText({id}, {param})"));
                    template.line(format!("{VIEW}.insert({node}, {id}, {NO_NODE})"));
                }
                ast::JSXElementChild::JSXElement(child) => match self.child(child, template)? {
                    Child::Node(id) => {
                        template.line(format!("{VIEW}.insert({node}, {id}, {NO_NODE})"));
                    }
                    Child::Component(call) => {
                        // The component runs at the call site, in source order.
                        let param = format!("{PREFIX}Param{}", template.params.len());
                        template.params.push((param.clone(), "number"));
                        template.args.push(call);
                        template.line(format!("{VIEW}.insert({node}, {param}, {NO_NODE})"));
                    }
                },
                ast::JSXElementChild::JSXFragment(fragment) => {
                    return Err(self.fail(
                        "JSX fragments are not supported: wrap the children in one element",
                        fragment.span,
                    ));
                }
                ast::JSXElementChild::JSXSpreadChild(spread) => {
                    return Err(self.fail("spread children are not supported", spread.span));
                }
            }
        }
        for param in refs {
            template.line(format!("{param}({node})"));
        }
        Ok(Child::Node(node))
    }

    /// `Name(props)`: attributes become a props object literal; component
    /// children are not supported.
    fn component(
        &mut self,
        name: &str,
        element: &mut ast::JSXElement,
        template: &mut Template,
    ) -> Result<String, CompilerError> {
        let span = element.opening.span;
        if element.children.iter().any(|child| match child {
            ast::JSXElementChild::JSXText(text) => text
                .value
                .as_str()
                .is_some_and(|text| !jsx_text(text).is_empty()),
            _ => true,
        }) {
            return Err(self.fail(
                format!("children of component `<{name}>` are not supported"),
                span,
            ));
        }
        let mut props = Vec::new();
        for attr in &mut element.opening.attrs {
            let ast::JSXAttrOrSpread::JSXAttr(attr) = attr else {
                return Err(self.fail("spread attributes are not supported", span));
            };
            let (prop, namespace) = attr_name(attr);
            if namespace.is_some() || !prop.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(self.fail(format!("invalid component prop `{prop}`"), attr.span));
            }
            let value = match attr.value.as_mut() {
                None => "true".to_owned(),
                Some(ast::JSXAttrValue::Str(text)) => literal(&string(text).unwrap_or_default()),
                Some(value) => {
                    let expr = expression(value).ok_or_else(|| {
                        self.fail(format!("prop `{prop}` needs a value"), attr.span)
                    })?;
                    let expr = self.take(expr)?;
                    let index = template.holes.len();
                    template.holes.push(Some(expr));
                    format!("{HOLE}{index}")
                }
            };
            props.push(format!("{prop}: {value}"));
        }
        Ok(if props.is_empty() {
            format!("{name}()")
        } else {
            format!("{name}({{ {} }})", props.join(", "))
        })
    }

    /// One attribute of an intrinsic element; returns the parameter of a
    /// `ref`, called once the element is complete.
    fn attribute(
        &mut self,
        tag: Tag,
        node: &str,
        attr: &mut ast::JSXAttr,
        template: &mut Template,
    ) -> Result<Option<String>, CompilerError> {
        let span = attr.span;
        let (name, namespace) = attr_name(attr);
        let fixed = attr
            .value
            .as_ref()
            .map_or(Some(String::new()), static_value);
        if let Some((_, kind)) = EVENTS.iter().find(|(event, _)| *event == name) {
            let filter = filter(*kind, namespace.as_deref()).map_err(|m| self.fail(m, span))?;
            let expr = attr
                .value
                .as_mut()
                .and_then(expression)
                .ok_or_else(|| self.fail(format!("`{name}` needs a handler"), span))?;
            let expr = self.take(expr)?;
            let expr = with_event_parameter(self.source, expr)?;
            let param = template.hole(expr, "(event: ViewEvent) => void", "{}");
            template.line(format!(
                "{VIEW}.on({node}, {}, {param}, {}, {}, {}, {})",
                kind.code(),
                filter.options,
                filter.key,
                filter.required,
                filter.forbidden
            ));
            return Ok(None);
        }
        if name == STYLE
            && let Some(property) = namespace
        {
            let property = CssProperty::ALL
                .iter()
                .copied()
                .find(|p| p.name() == property)
                .ok_or_else(|| self.fail(format!("unknown CSS property `{property}`"), span))?;
            return self
                .style(tag, node, property, attr, fixed, template)
                .map(|()| None);
        }
        if namespace.is_some() {
            return Err(self.fail(format!("unknown namespaced attribute `{name}:`"), span));
        }
        match name.as_str() {
            REF => {
                let expr = attr
                    .value
                    .as_mut()
                    .and_then(expression)
                    .ok_or_else(|| self.fail("`ref` needs a function", span))?;
                let expr = self.take(expr)?;
                return Ok(Some(template.hole(expr, "(node: number) => void", "{}")));
            }
            CLASS => {
                return self
                    .dynamic_string(
                        attr,
                        fixed,
                        template,
                        |value| {
                            value
                                .split_ascii_whitespace()
                                .all(css::ident)
                                .then(|| format!("{VIEW}.setClass({node}, {})", literal(value)))
                        },
                        &format!("{VIEW}.bindClass({node}, {{}})"),
                    )
                    .map(|()| None);
            }
            STYLE => {
                let Some(text) = fixed else {
                    return Err(self.fail(
                        "`style` takes a static declaration list; use `style:<property>={...}` for dynamic values",
                        span,
                    ));
                };
                for declaration in text.split(';').filter(|d| !d.trim().is_empty()) {
                    let (property, value) = declaration.split_once(':').ok_or_else(|| {
                        self.fail(format!("invalid style declaration `{declaration}`"), span)
                    })?;
                    let property = CssProperty::ALL
                        .iter()
                        .copied()
                        .find(|p| p.name() == property.trim())
                        .ok_or_else(|| {
                            self.fail(format!("unknown CSS property `{}`", property.trim()), span)
                        })?;
                    self.static_style(tag, node, property, value.trim(), span, template)?;
                }
                return Ok(None);
            }
            VALUE if matches!(tag, Tag::Input | Tag::Textarea | Tag::Select) => {
                return self
                    .dynamic_string(
                        attr,
                        fixed,
                        template,
                        |value| Some(format!("{VIEW}.setValue({node}, {})", literal(value))),
                        &format!("{VIEW}.bindValue({node}, {{}})"),
                    )
                    .map(|()| None);
            }
            CHECKED if tag == Tag::Input => {
                return self
                    .flag(
                        attr,
                        template,
                        format!("{VIEW}.setChecked({node}, true)"),
                        &format!("{VIEW}.bindChecked({node}, {{}})"),
                    )
                    .map(|()| None);
            }
            "className" | "htmlFor" => {
                return Err(self.fail(
                    format!(
                        "use `{}` instead of `{name}`",
                        if name == "className" { CLASS } else { "for" }
                    ),
                    span,
                ));
            }
            _ => {}
        }
        let attribute = Attribute::ALL
            .iter()
            .copied()
            .find(|a| a.name() == name)
            .ok_or_else(|| {
                self.fail(
                    format!("unknown attribute `{name}` (not in the L0 view subset)"),
                    span,
                )
            })?;
        if !admits(attribute, tag) {
            return Err(self.fail(
                format!("attribute `{name}` is not allowed on `<{}>`", tag.name()),
                span,
            ));
        }
        let code = attribute.code();
        if attribute.value() == AttributeValue::Boolean {
            return self
                .flag(
                    attr,
                    template,
                    format!("{VIEW}.setAttribute({node}, {code}, \"\")"),
                    &format!("{VIEW}.bindFlag({node}, {code}, {{}})"),
                )
                .map(|()| None);
        }
        let grammar = attribute.value();
        self.dynamic_string(
            attr,
            fixed,
            template,
            |value| {
                let valid = if grammar == AttributeValue::Href && attributes::external(value) {
                    true
                } else {
                    attributes::valid(grammar, tag, value)
                };
                valid.then(|| format!("{VIEW}.setAttribute({node}, {code}, {})", literal(value)))
            },
            &format!("{VIEW}.bindAttribute({node}, {code}, {{}})"),
        )
        .map(|()| None)
    }

    /// A string-valued attribute: static (checked by `statement`, which
    /// returns `None` for an invalid value) or a getter hole bound by `bind`.
    fn dynamic_string(
        &mut self,
        attr: &mut ast::JSXAttr,
        fixed: Option<String>,
        template: &mut Template,
        statement: impl Fn(&str) -> Option<String>,
        bind: &str,
    ) -> Result<(), CompilerError> {
        let span = attr.span;
        let (name, _) = attr_name(attr);
        if let Some(value) = fixed {
            if attr.value.is_none() {
                return Err(self.fail(format!("attribute `{name}` needs a value"), span));
            }
            let line = statement(&value).ok_or_else(|| {
                self.fail(
                    format!("invalid value `{value}` for attribute `{name}`"),
                    span,
                )
            })?;
            template.line(line);
            return Ok(());
        }
        let expr = attr
            .value
            .as_mut()
            .and_then(expression)
            .ok_or_else(|| self.fail(format!("attribute `{name}` needs a value"), span))?;
        let expr = self.take(expr)?;
        let param = template.hole(expr, "() => string", "(): string => `${{}}`");
        template.line(bind.replace("{}", &param));
        Ok(())
    }

    /// A boolean attribute or `checked`: present (`<x disabled />`), or a
    /// getter `() => boolean`.
    fn flag(
        &mut self,
        attr: &mut ast::JSXAttr,
        template: &mut Template,
        present: String,
        bind: &str,
    ) -> Result<(), CompilerError> {
        let span = attr.span;
        let (name, _) = attr_name(attr);
        match attr.value.as_mut() {
            None => {
                template.line(present);
                Ok(())
            }
            Some(ast::JSXAttrValue::Str(_)) => Err(self.fail(
                format!("boolean attribute `{name}` takes no string value; use `{name}` or `{name}={{expr}}`"),
                span,
            )),
            Some(value) => {
                let expr = expression(value)
                    .ok_or_else(|| self.fail(format!("attribute `{name}` needs a value"), span))?;
                let expr = self.take(expr)?;
                let param = template.hole(expr, "() => boolean", "(): boolean => {}");
                template.line(bind.replace("{}", &param));
                Ok(())
            }
        }
    }

    fn style(
        &mut self,
        tag: Tag,
        node: &str,
        property: CssProperty,
        attr: &mut ast::JSXAttr,
        fixed: Option<String>,
        template: &mut Template,
    ) -> Result<(), CompilerError> {
        let span = attr.span;
        if let Some(value) = fixed {
            return self.static_style(tag, node, property, &value, span, template);
        }
        let code = property.code();
        self.dynamic_string(
            attr,
            None,
            template,
            |_| None,
            &format!("{VIEW}.bindStyle({node}, {code}, {{}})"),
        )
    }

    fn static_style(
        &self,
        tag: Tag,
        node: &str,
        property: CssProperty,
        value: &str,
        span: Span,
        template: &mut Template,
    ) -> Result<(), CompilerError> {
        if matches!(property, CssProperty::FieldSizing | CssProperty::Resize)
            && tag != Tag::Textarea
        {
            return Err(self.fail(
                format!(
                    "CSS property `{}` applies to `<textarea>` only",
                    property.name()
                ),
                span,
            ));
        }
        if !css::property_value(property, value) {
            return Err(self.fail(
                format!(
                    "invalid value `{value}` for CSS property `{}` (grammar: {})",
                    property.name(),
                    property.grammar()
                ),
                span,
            ));
        }
        template.line(format!(
            "{VIEW}.setStyle({node}, {}, {})",
            property.code(),
            literal(value)
        ));
        Ok(())
    }
}

enum Child {
    /// The generated node binding of an intrinsic element.
    Node(String),
    /// The call text of a component.
    Component(String),
}

/// A parameterless arrow or function expression gets an unused event
/// parameter, so it fits the handler type; anything else is passed as is.
fn with_event_parameter(
    source: &Source,
    mut expr: Box<ast::Expr>,
) -> Result<Box<ast::Expr>, CompilerError> {
    let needs = match &*expr {
        ast::Expr::Arrow(arrow) => arrow.params.is_empty(),
        ast::Expr::Fn(function) => function.function.params.is_empty(),
        _ => false,
    };
    if !needs {
        return Ok(expr);
    }
    let module = parse_snippet(
        source,
        &format!("const {PREFIX}Handler = ({EVENT_PARAMETER}: ViewEvent): void => {{}};"),
    )?;
    let parameter = module.body.into_iter().find_map(|item| match item {
        ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(var))) => var
            .decls
            .into_iter()
            .next()
            .and_then(|d| d.init)
            .and_then(|init| match *init {
                ast::Expr::Arrow(arrow) => arrow.params.into_iter().next(),
                _ => None,
            }),
        _ => None,
    });
    let Some(mut parameter) = parameter else {
        return Err(CompilerError::Internal("event parameter".into()));
    };
    let span = expr.span();
    parameter.visit_mut_with(&mut Respan(span));
    match &mut *expr {
        ast::Expr::Arrow(arrow) => arrow.params.push(parameter),
        ast::Expr::Fn(function) => function.function.params.push(ast::Param {
            span,
            decorators: Vec::new(),
            pat: parameter,
        }),
        _ => {}
    }
    Ok(expr)
}

#[cfg(test)]
/// The typed `JSX` namespace of `varta.d.ts`, generated from the registry
/// and the TSX attribute rules above: the attributes every element takes,
/// then each element's own.
pub(crate) fn declarations() -> String {
    let attribute_line = |attribute: &Attribute| {
        let ty = if attribute.value() == AttributeValue::Boolean {
            "boolean"
        } else {
            "string"
        };
        format!("    {}?: {ty};\n", quoted(attribute.name()))
    };
    let mut out = String::new();
    out.push_str("declare namespace JSX {\n");
    out.push_str("  /** A TSX element evaluates to its L0 node id. */\n");
    out.push_str("  type Element = number;\n");
    out.push_str("  interface Attributes {\n");
    out.push_str("    class?: string;\n");
    out.push_str("    style?: string;\n");
    out.push_str("    ref?: (node: number) => void;\n");
    for attribute in Attribute::ALL {
        if attribute.tags() == varta_common::view::Tags::All {
            out.push_str(&attribute_line(attribute));
        }
    }
    for (event, _) in EVENTS {
        let _ = writeln!(out, "    {event}?: (event: ViewEvent) => void;");
    }
    out.push_str("  }\n");
    out.push_str("  interface IntrinsicElements {\n");
    for tag in Tag::ALL {
        let mut own = String::new();
        for attribute in Attribute::ALL {
            if attribute.tags() != varta_common::view::Tags::All && admits(*attribute, *tag) {
                own.push_str("  ");
                own.push_str(&attribute_line(attribute));
            }
        }
        if matches!(tag, Tag::Input | Tag::Textarea | Tag::Select) {
            own.push_str("      value?: string;\n");
        }
        if *tag == Tag::Input {
            own.push_str("      checked?: boolean;\n");
        }
        if own.is_empty() {
            let _ = writeln!(out, "    {}: Attributes;", tag.name());
        } else {
            let _ = write!(out, "    {}: Attributes & {{\n{own}    }};\n", tag.name());
        }
    }
    out.push_str("  }\n}\n");
    out
}

#[cfg(test)]
fn quoted(name: &str) -> String {
    if name.contains('-') {
        format!("\"{name}\"")
    } else {
        name.to_owned()
    }
}
