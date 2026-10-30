//! V37 RegExp (`regexp()`): regular expression literals and `new RegExp`
//! with constant arguments compile ahead of time (`crate::regexp`) into a
//! matcher program and construct the engine prelude class through its private
//! `RegExp.compiled`; the regex string methods call the prelude's private
//! statics; match arrays are prelude classes whose elements read their
//! private storage. Prelude modules (only) may call the `varta.regexp.*` guest
//! operations (`ir::RegExpIntrinsic`).
use super::*;
use crate::ir::RegExpIntrinsic;
use crate::modules::engine::{self, EXEC_ARRAY_CLASS, MATCH_ARRAY_CLASS, REGEXP_CLASS};

const INTRINSICS: [(&str, RegExpIntrinsic); 3] = [
    ("varta.regexp.exec", RegExpIntrinsic::Exec),
    ("varta.regexp.slot", RegExpIntrinsic::Slot),
    ("varta.regexp.unit", RegExpIntrinsic::Unit),
];
/// The string methods that take a RegExp (or a pattern string).
pub(super) const STRING_METHODS: [&str; 5] =
    ["match", "matchAll", "replace", "replaceAll", "search"];
/// Flag bits of `RegExp.compiled`, as in the prelude.
const GLOBAL: u32 = 1;
const IGNORE_CASE: u32 = 2;
const MULTILINE: u32 = 4;
const DOT_ALL: u32 = 8;
const UNICODE: u32 = 16;
const STICKY: u32 = 32;
/// The split limit when none is given (2^32 - 1).
const NO_LIMIT: f64 = u32::MAX as f64;
/// Prelude fields of a match array: element 0 and every element.
const FIRST: &str = "first";
const VALUES: &str = "values";
const GROUP_SEPARATOR: &str = ",";
/// The match-array parameter of a synthesized replacer adapter; no source
/// identifier can spell it.
const MATCH_PARAMETER: &str = "%match";
/// Positions of the group count and names among `RegExp.compiled`'s
/// arguments.
const NAMES_ARGUMENT: usize = 5;
const COUNT_ARGUMENT: usize = 6;
const NON_CONSTANT: &str = "new RegExp with a pattern or flags that are not string literals (v37 compiles every pattern ahead of time; write a regular expression literal)";

fn string(span: swc_common::Span, bytes: Vec<u8>) -> Expr {
    Expr {
        span,
        ty: Type::String,
        kind: E::String(bytes),
    }
}
fn number(span: swc_common::Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
/// The text of a string literal or of a template literal without
/// substitutions.
fn constant_text(expression: &ast::Expr) -> Option<String> {
    match stmt::unparen(expression) {
        ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str().map(str::to_owned),
        ast::Expr::Tpl(template) if template.exprs.is_empty() => template
            .quasis
            .first()
            .and_then(|quasi| quasi.cooked.as_ref())
            .and_then(|cooked| cooked.as_str())
            .map(str::to_owned),
        _ => None,
    }
}

impl Lower<'_> {
    /// The `varta.regexp.*` guest operation `name`, inside a prelude module.
    pub(super) fn regexp_intrinsic(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
    ) -> Result<Option<Expr>> {
        let Some((_, op)) = INTRINSICS.iter().find(|(intrinsic, _)| *intrinsic == name) else {
            return Ok(None);
        };
        if !engine::is_prelude_source(self.source) || !self.types.profile.regexp() {
            return Ok(None);
        }
        let number = self.types.number();
        let (params, ty): (&[Type], Type) = match op {
            RegExpIntrinsic::Exec => (
                &[Type::String, Type::String, number, Type::Boolean],
                Type::String,
            ),
            RegExpIntrinsic::Slot | RegExpIntrinsic::Unit => (&[Type::String, number], number),
        };
        if call.args.len() != params.len() || call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(invalid(format!(
                "{name} expects {} arguments",
                params.len()
            )));
        }
        let mut args = Vec::new();
        for (arg, param) in call.args.iter().zip(params) {
            let value = self.expr(&arg.expr)?;
            if value.ty != *param {
                return Err(type_error(format!(
                    "{name} requires {param}, found {}",
                    value.ty
                )));
            }
            args.push(value);
        }
        Ok(Some(Expr {
            span: call.span,
            ty,
            kind: E::RegExp(*op, args),
        }))
    }

    fn prelude_class_id(&self, name: &str) -> Result<u32> {
        self.types
            .classes
            .prelude_class(name)
            .ok_or_else(|| CompilerError::Internal(format!("missing {name} prelude class")))
    }
    /// Whether `ty` is a match array (`RegExpExecArray`, `RegExpMatchArray`).
    pub(super) fn match_array(&self, ty: Type) -> bool {
        let Type::Class(class) = ty else {
            return false;
        };
        self.types.profile.regexp()
            && [EXEC_ARRAY_CLASS, MATCH_ARRAY_CLASS]
                .iter()
                .any(|name| self.types.classes.prelude_class(name) == Some(class))
    }
    fn match_field(&self, value: Expr, name: &str) -> Result<Expr> {
        let Type::Class(class) = value.ty else {
            return Err(CompilerError::Internal("missing match array".into()));
        };
        let (slot, field) = self
            .types
            .classes
            .field(class, name)
            .ok_or_else(|| CompilerError::Internal("missing match array field".into()))?;
        Ok(Expr {
            span: value.span,
            ty: field.ty,
            kind: E::Field(Box::new(value), slot),
        })
    }
    /// A match array viewed as the dense array of its elements (for
    /// destructuring and `for…of`); other values are unchanged.
    pub(super) fn match_array_values(&self, value: Expr) -> Result<Expr> {
        if self.match_array(value.ty) {
            return self.match_field(value, VALUES);
        }
        Ok(value)
    }
    /// `match[key]`: element 0 is always a string, other elements are
    /// `string | undefined` (a group that did not take part, or no element).
    pub(super) fn match_element(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
        key: &ast::Expr,
    ) -> Result<Expr> {
        if matches!(stmt::unparen(key), ast::Expr::Lit(ast::Lit::Num(n)) if n.value == 0.0) {
            return Ok(Expr {
                span: member.span,
                ..self.match_field(value, FIRST)?
            });
        }
        let values = self.match_field(value, VALUES)?;
        let Type::Array(id) = values.ty else {
            return Err(CompilerError::Internal("missing match array values".into()));
        };
        let element = self.types.arrays[id as usize];
        self.maybe_index(member, values, key, element)
    }

    /// A call of the prelude's private static `RegExp.name`.
    fn regexp_static(
        &mut self,
        name: &str,
        args: Vec<Expr>,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let class = self.prelude_class_id(REGEXP_CLASS)?;
        let signature = self
            .types
            .classes
            .get(class)
            .statics
            .get(name)
            .map(|method| method.signature.clone())
            .ok_or_else(|| CompilerError::Internal(format!("missing RegExp.{name}")))?;
        if args.len() != signature.params.len() {
            return Err(CompilerError::Internal(format!(
                "RegExp.{name} argument count"
            )));
        }
        let mut values = Vec::new();
        for (value, param) in args.into_iter().zip(&signature.params) {
            let value = unions::widen(value, Some(*param), self.types)?;
            if value.ty != *param {
                return Err(self.source.locate(
                    type_error(format!(
                        "argument type mismatch: expected {param}, found {}",
                        value.ty
                    )),
                    value.span,
                ));
            }
            values.push(value);
        }
        Ok(Expr {
            span,
            ty: signature.result,
            kind: E::Call(signature.index, values),
        })
    }
    /// A fresh instance of a compiled pattern.
    fn compiled_regexp(
        &mut self,
        regexp: crate::regexp::RegExp,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let crate::regexp::RegExp {
            source,
            flags,
            compiled,
        } = regexp;
        let bits = [
            (flags.global, GLOBAL),
            (flags.ignore_case, IGNORE_CASE),
            (flags.multiline, MULTILINE),
            (flags.dot_all, DOT_ALL),
            (flags.unicode, UNICODE),
            (flags.sticky, STICKY),
        ]
        .into_iter()
        .filter_map(|(set, bit)| set.then_some(bit))
        .fold(0, |bits, bit| bits | bit);
        let names = if compiled.names.is_empty() {
            String::new()
        } else {
            (1..=compiled.groups)
                .map(|group| {
                    compiled
                        .names
                        .iter()
                        .find(|(_, index)| *index == group)
                        .map_or("", |(name, _)| name.as_str())
                })
                .collect::<Vec<_>>()
                .join(GROUP_SEPARATOR)
        };
        let bytes = |units: &[u16]| units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
        let program = bytes(&compiled.units);
        let lean = bytes(&compiled.lean);
        let profile = self.types.profile;
        let args = vec![
            string(span, profile.string_bytes(&source)),
            string(span, profile.string_bytes(&flags.text())),
            number(span, f64::from(bits)),
            string(span, program),
            string(span, lean),
            string(span, profile.string_bytes(&names)),
            number(span, (compiled.groups + 1) as f64),
        ];
        self.regexp_static("compiled", args, span)
    }
    /// `/pattern/flags`: a new RegExp per evaluation.
    pub(super) fn regexp_literal(&mut self, regex: &ast::Regex) -> Result<Expr> {
        let compiled = crate::regexp::literal(regex.exp.as_ref(), regex.flags.as_ref())
            .map_err(|e| self.source.locate(e.into_compiler(), regex.span))?;
        self.compiled_regexp(compiled, regex.span)
    }
    /// Whether `class` is the prelude `RegExp` and this is a program module.
    pub(super) fn regexp_constructor(&self, class: u32) -> bool {
        self.types.profile.regexp()
            && !engine::is_prelude_source(self.source)
            && self.types.classes.prelude_class(REGEXP_CLASS) == Some(class)
    }
    /// `new RegExp(pattern?, flags?)` with string literal arguments.
    pub(super) fn regexp_new(&mut self, new: &ast::NewExpr) -> Result<Expr> {
        let args = new.args.as_deref().unwrap_or_default();
        if args.len() > 2 || args.iter().any(|arg| arg.spread.is_some()) || new.type_args.is_some()
        {
            return Err(self.source.locate(
                invalid("new RegExp expects a pattern and optional flags"),
                new.span,
            ));
        }
        let constant = |index: usize| match args.get(index) {
            None => Some(String::new()),
            Some(arg) => constant_text(&arg.expr),
        };
        let (Some(pattern), Some(flags)) = (constant(0), constant(1)) else {
            return Err(self.source.locate(unsupported(NON_CONSTANT), new.span));
        };
        let compiled = crate::regexp::constructed(&pattern, &flags)
            .map_err(|e| self.source.locate(e.into_compiler(), new.span))?;
        self.compiled_regexp(compiled, new.span)
    }
    /// The RegExp argument of `search`, `match` or `matchAll`: a RegExp
    /// value, or a string literal compiled like `new RegExp(text, flags)`.
    fn regexp_argument(&mut self, argument: &ast::Expr, flags: &str) -> Result<Expr> {
        if let Some(text) = constant_text(argument) {
            let compiled = crate::regexp::constructed(&text, flags)
                .map_err(|e| self.source.locate(e.into_compiler(), argument.span()))?;
            return self.compiled_regexp(compiled, argument.span());
        }
        let value = self.expr(argument)?;
        self.require_regexp(value)
    }
    fn require_regexp(&self, value: Expr) -> Result<Expr> {
        if self.prelude_class_id(REGEXP_CLASS).ok().map(Type::Class) == Some(value.ty) {
            return Ok(value);
        }
        Err(self.source.locate(
            type_error(format!(
                "expected a RegExp or a string literal pattern, found {}",
                value.ty
            )),
            value.span,
        ))
    }
    fn regexp_value(&self, ty: Type) -> bool {
        self.types
            .classes
            .prelude_class(REGEXP_CLASS)
            .map(Type::Class)
            == Some(ty)
    }

    /// `text.method(...)` for the regex string methods.
    pub(super) fn regexp_string_method(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        method: &str,
    ) -> Result<Expr> {
        let span = call.span;
        if call.args.iter().any(|arg| arg.spread.is_some()) || call.type_args.is_some() {
            return Err(invalid(format!("String.prototype.{method} arguments")));
        }
        let arity = |range: std::ops::RangeInclusive<usize>| {
            if range.contains(&call.args.len()) {
                Ok(())
            } else {
                Err(invalid(format!(
                    "String.prototype.{method} expects {} to {} arguments",
                    range.start(),
                    range.end()
                )))
            }
        };
        match method {
            "search" | "match" | "matchAll" => {
                arity(1..=1)?;
                let flags = if method == "matchAll" { "g" } else { "" };
                let regexp = self.regexp_argument(&call.args[0].expr, flags)?;
                self.regexp_static(method, vec![receiver, regexp], span)
            }
            "replace" | "replaceAll" => {
                arity(2..=2)?;
                let pattern = self.expr(&call.args[0].expr)?;
                let all = method == "replaceAll";
                if pattern.ty == Type::String {
                    let argument = &*call.args[1].expr;
                    let replacement = if matches!(
                        stmt::unparen(argument),
                        ast::Expr::Arrow(_) | ast::Expr::Fn(_)
                    ) {
                        let context = self.static_parameter("replaceTextWith", 2)?;
                        self.expr_expected(argument, Some(context))?
                    } else {
                        self.expr(argument)?
                    };
                    return self.text_replacement(receiver, pattern, replacement, all, span);
                }
                let pattern = self.require_regexp(pattern)?;
                let argument = &*call.args[1].expr;
                if self.callback_argument(argument) {
                    let groups = self.static_groups(&pattern);
                    let replacer = self.regexp_replacer(argument, groups)?;
                    let target = if all { "replaceAllWith" } else { "replaceWith" };
                    return self.regexp_static(target, vec![receiver, pattern, replacer], span);
                }
                let replacement = self.expr(argument)?;
                if replacement.ty != Type::String {
                    return Err(self.source.locate(
                        type_error(format!(
                            "replacement must be a string or a replacer function, found {}",
                            replacement.ty
                        )),
                        replacement.span,
                    ));
                }
                self.regexp_static(method, vec![receiver, pattern, replacement], span)
            }
            _ => Err(CompilerError::Internal(format!(
                "String.prototype.{method}"
            ))),
        }
    }
    /// `text.split(separator, limit?)` with a lowered `separator`: a RegExp
    /// splits through the RegExp prelude, anything else is `None` and goes
    /// to the string `split` of the text prelude (`builtin_methods`), so
    /// string `split` has one dispatch path.
    pub(super) fn regexp_split(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        separator: Expr,
    ) -> Result<std::result::Result<Expr, (Expr, Expr)>> {
        if !self.types.profile.regexp() || !self.regexp_value(separator.ty) {
            return Ok(Err((receiver, separator)));
        }
        let span = call.span;
        if call.args.len() > 2 {
            return Err(invalid("String.prototype.split expects 0 to 2 arguments"));
        }
        let limit = match call.args.get(1) {
            Some(arg) => {
                let limit = self.expr(&arg.expr)?;
                if limit.ty != self.types.number() {
                    return Err(self
                        .source
                        .locate(type_error("split limit must be a number"), limit.span));
                }
                limit
            }
            None => number(span, NO_LIMIT),
        };
        self.regexp_static("split", vec![receiver, separator, limit], span)
            .map(Ok)
    }
    fn static_parameter(&self, name: &str, position: usize) -> Result<Type> {
        let class = self.prelude_class_id(REGEXP_CLASS)?;
        self.types
            .classes
            .get(class)
            .statics
            .get(name)
            .and_then(|method| method.signature.params.get(position).copied())
            .ok_or_else(|| CompilerError::Internal(format!("missing RegExp.{name}")))
    }
    /// Whether a replacement argument is a replacer: a function literal or
    /// a binding of a function type.
    fn callback_argument(&self, argument: &ast::Expr) -> bool {
        match stmt::unparen(argument) {
            ast::Expr::Arrow(_) | ast::Expr::Fn(_) => true,
            ast::Expr::Ident(id) => {
                self.binding(id.sym.as_ref())
                    .ok()
                    .and_then(|binding| self.refinements.get(&binding.slot).copied().or(binding.ty))
                    .is_some_and(|ty| matches!(ty, Type::Function(_)))
                    || self.functions.contains_key(id.sym.as_ref())
            }
            _ => false,
        }
    }
    /// The group count and whether groups are named, when `regexp` is a
    /// pattern compiled in place (a literal or a constant `new RegExp`).
    fn static_groups(&self, regexp: &Expr) -> Option<(usize, bool)> {
        let class = self.types.classes.prelude_class(REGEXP_CLASS)?;
        let compiled = self.types.classes.get(class).statics.get("compiled")?;
        match &regexp.kind {
            E::Call(index, args) if *index == compiled.signature.index => {
                let count = match args.get(COUNT_ARGUMENT)?.kind {
                    E::Float64(count) => count as usize,
                    _ => return None,
                };
                let named = match &args.get(NAMES_ARGUMENT)?.kind {
                    E::String(names) => !names.is_empty(),
                    _ => return None,
                };
                Some((count.checked_sub(1)?, named))
            }
            _ => None,
        }
    }
    /// A replacer adapted to `(match: RegExpExecArray) => string`: its
    /// parameters become constants read from the match array in the order
    /// String.prototype.replace passes them (the match, each capture, the
    /// position, the input and, with named groups, the groups object). An
    /// annotated `string` capture asserts that the group took part. Without
    /// a statically known pattern only the match itself can be passed.
    fn regexp_replacer(
        &mut self,
        callback: &ast::Expr,
        groups: Option<(usize, bool)>,
    ) -> Result<Expr> {
        let span = callback.span();
        let context = self.static_parameter("replaceWith", 2)?;
        let source = |position: usize| replacer_source(position, groups, span);
        let too_many = || {
            self.source.locate(
                unsupported(
                    "RegExp replacer parameter beyond those String.prototype.replace passes (a RegExp that is not a literal passes only the match)",
                ),
                span,
            )
        };
        let (declarations, body, return_type) = match stmt::unparen(callback) {
            ast::Expr::Arrow(arrow)
                if !arrow.is_async && !arrow.is_generator && arrow.type_params.is_none() =>
            {
                let mut declarations = Vec::new();
                for (position, pattern) in arrow.params.iter().enumerate() {
                    let (init, capture) = source(position).ok_or_else(too_many)?;
                    declarations.push(
                        replacer_constant(pattern, init, capture, span)
                            .map_err(|e| self.source.locate(e, pattern.span()))?,
                    );
                }
                let body = match &*arrow.body {
                    ast::ArrowFunctionBody::FunctionBody(block) => block.clone(),
                    ast::ArrowFunctionBody::Expr(value) => ast::FunctionBody {
                        span,
                        stmts: vec![ast::Stmt::Return(ast::ReturnStmt {
                            span,
                            arg: Some(value.clone()),
                        })],
                    },
                };
                (declarations, body, arrow.return_type.clone())
            }
            ast::Expr::Fn(function)
                if !function.function.is_async
                    && !function.function.is_generator
                    && function.function.type_params.is_none()
                    && function.ident.is_none() =>
            {
                let mut declarations = Vec::new();
                for (position, param) in function.function.params.iter().enumerate() {
                    let (init, capture) = source(position).ok_or_else(too_many)?;
                    declarations.push(
                        replacer_constant(&param.pat, init, capture, span)
                            .map_err(|e| self.source.locate(e, param.span))?,
                    );
                }
                let body = function.function.body.clone().ok_or_else(|| {
                    self.source
                        .locate(unsupported("replacer function without a body"), span)
                })?;
                (declarations, body, function.function.return_type.clone())
            }
            ast::Expr::Ident(id) => {
                let value = self.expr(callback)?;
                let Type::Function(function) = value.ty else {
                    return Err(self.source.locate(
                        type_error(format!("replacer must be a function, found {}", value.ty)),
                        span,
                    ));
                };
                let count = self.types.functions[function as usize].params.len();
                let mut args = Vec::new();
                for position in 0..count {
                    let (init, _) = source(position).ok_or_else(too_many)?;
                    args.push(ast::ExprOrSpread {
                        spread: None,
                        expr: Box::new(init),
                    });
                }
                let call = ast::Expr::Call(ast::CallExpr {
                    span,
                    ctxt: Default::default(),
                    callee: ast::Callee::Expr(Box::new(ast::Expr::Ident(id.clone()))),
                    args,
                    type_args: None,
                });
                let body = ast::FunctionBody {
                    span,
                    stmts: vec![ast::Stmt::Return(ast::ReturnStmt {
                        span,
                        arg: Some(Box::new(call)),
                    })],
                };
                (Vec::new(), body, None)
            }
            _ => {
                return Err(self.source.locate(
                    unsupported(
                        "RegExp replacer other than a function literal or a function binding",
                    ),
                    span,
                ));
            }
        };
        let mut stmts = declarations;
        stmts.extend(body.stmts);
        let adapter = ast::Expr::Arrow(ast::ArrowExpr {
            span,
            ctxt: Default::default(),
            params: vec![ast::Pat::Ident(ast::BindingIdent {
                id: ident(MATCH_PARAMETER, span),
                type_ann: None,
            })],
            body: Box::new(ast::ArrowFunctionBody::FunctionBody(ast::FunctionBody {
                span: body.span,
                stmts,
            })),
            is_async: false,
            is_generator: false,
            type_params: None,
            return_type,
        });
        self.expr_expected(&adapter, Some(context))
    }
    /// `replace`/`replaceAll` with a string pattern.
    fn text_replacement(
        &mut self,
        receiver: Expr,
        pattern: Expr,
        replacement: Expr,
        all: bool,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let all = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Integer(i32::from(all)),
        };
        if replacement.ty == Type::String {
            return self.regexp_static(
                "replaceText",
                vec![receiver, pattern, replacement, all],
                span,
            );
        }
        if matches!(replacement.ty, Type::Function(_)) {
            return self.regexp_static(
                "replaceTextWith",
                vec![receiver, pattern, replacement, all],
                span,
            );
        }
        Err(self.source.locate(
            type_error(format!(
                "replacement must be a string or a replacer function, found {}",
                replacement.ty
            )),
            replacement.span,
        ))
    }
}

fn ident(name: &str, span: swc_common::Span) -> ast::Ident {
    ast::Ident {
        span,
        ctxt: Default::default(),
        sym: name.into(),
        optional: false,
    }
}

/// What String.prototype.replace passes at parameter `position` of a
/// replacer, read from the adapter's match array, and whether it is a
/// capture; `None` beyond the known arguments.
fn replacer_source(
    position: usize,
    groups: Option<(usize, bool)>,
    span: swc_common::Span,
) -> Option<(ast::Expr, bool)> {
    let matched = || Box::new(ast::Expr::Ident(ident(MATCH_PARAMETER, span)));
    let element = |index: usize| {
        ast::Expr::Member(ast::MemberExpr {
            span,
            obj: matched(),
            prop: ast::MemberProp::Computed(ast::ComputedPropName {
                span,
                expr: Box::new(ast::Expr::Lit(ast::Lit::Num(ast::Number {
                    span,
                    value: index as f64,
                    raw: None,
                }))),
            }),
        })
    };
    let field = |name: &str| {
        ast::Expr::Member(ast::MemberExpr {
            span,
            obj: matched(),
            prop: ast::MemberProp::Ident(ast::IdentName {
                span,
                sym: name.into(),
            }),
        })
    };
    if position == 0 {
        return Some((element(0), false));
    }
    let (count, named) = groups?;
    match position {
        p if p <= count => Some((element(p), true)),
        p if p == count + 1 => Some((field("index"), false)),
        p if p == count + 2 => Some((field("input"), false)),
        p if p == count + 3 && named => Some((field("groups"), false)),
        _ => None,
    }
}

/// `const <parameter> = <init>;` for one replacer parameter.
fn replacer_constant(
    pattern: &ast::Pat,
    init: ast::Expr,
    capture: bool,
    span: swc_common::Span,
) -> Result<ast::Stmt> {
    let ast::Pat::Ident(binding) = pattern else {
        return Err(unsupported(
            "RegExp replacer parameter other than a plain identifier",
        ));
    };
    // `string` for a capture asserts it took part; unannotated it is
    // `string | undefined`.
    let asserted = capture
        && binding.type_ann.as_ref().is_some_and(|annotation| {
            matches!(
                &*annotation.type_ann,
                ast::TsType::TsKeywordType(keyword)
                    if keyword.kind == ast::TsKeywordTypeKind::TsStringKeyword
            )
        });
    let init = if asserted {
        ast::Expr::TsNonNull(ast::TsNonNullExpr {
            span,
            expr: Box::new(init),
        })
    } else {
        init
    };
    Ok(ast::Stmt::Decl(ast::Decl::Var(Box::new(ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Const,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: ast::Pat::Ident(binding.clone()),
            init: Some(Box::new(init)),
            definite: false,
        }],
    }))))
}
