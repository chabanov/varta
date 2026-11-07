//! V37 RegExp (`regexp()`): regular expression literals and `new RegExp`
//! with constant arguments compile ahead of time (`crate::regexp`) into a
//! matcher program and construct the engine prelude class through its private
//! `RegExp.compiled`; the regex string methods call the prelude's private
//! statics; match arrays are prelude classes whose elements read their
//! private storage. Prelude modules (only) may call the `varta.regexp.*` guest
//! operations (`ir::RegExpIntrinsic`).
use super::*;
use crate::ir::RegExpIntrinsic;
use crate::modules::engine::{
    self, EXEC_ARRAY_CLASS, INDICES_ARRAY_CLASS, MATCH_ARRAY_CLASS, REGEXP_CLASS,
};

const INTRINSICS: [(&str, RegExpIntrinsic); 3] = [
    ("varta.regexp.exec", RegExpIntrinsic::Exec),
    ("varta.regexp.slot", RegExpIntrinsic::Slot),
    ("varta.regexp.unit", RegExpIntrinsic::Unit),
];
/// The string methods that take a RegExp (or a pattern string).
pub(super) const STRING_METHODS: [&str; 5] =
    ["match", "matchAll", "replace", "replaceAll", "search"];
/// V38 (`full_regexp`): the run-time pattern compiler, `varta.regexp.compile`.
const COMPILE_INTRINSIC: &str = "varta.regexp.compile";
/// The split limit when none is given (2^32 - 1).
const NO_LIMIT: f64 = u32::MAX as f64;
/// The prelude field of a match array that holds its elements, and (v37)
/// the field that holds element 0.
const VALUES: &str = "values";
const FIRST: &str = "first";
/// The hidden binding of a match array's elements while one of its array
/// members is lowered; no source identifier can spell it.
const ELEMENTS_BINDING: &str = "#match";
/// The own data properties of a v38 match array; every other member is an
/// array member of its elements.
const OWN_MEMBERS: [&str; 4] = ["index", "input", "groups", "indices"];
/// The match-array parameter of a synthesized replacer adapter; no source
/// identifier can spell it.
const MATCH_PARAMETER: &str = "%match";
/// The arguments parameter of a synthesized v38 run-time replacer adapter.
const ARGUMENTS_PARAMETER: &str = "%arguments";
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
        if !engine::is_prelude_source(self.source) || !self.types.profile.regexp() {
            return Ok(None);
        }
        if name == COMPILE_INTRINSIC && self.types.profile.full_regexp() {
            return self.regexp_compile_intrinsic(call).map(Some);
        }
        let Some((_, op)) = INTRINSICS.iter().find(|(intrinsic, _)| *intrinsic == name) else {
            return Ok(None);
        };
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
        // V38: the match runs the linked backtracking matcher.
        let kind = if *op == RegExpIntrinsic::Exec && self.types.profile.full_regexp() {
            E::Host(varta_common::HostOperation::RegExpExec, args)
        } else {
            E::RegExp(*op, args)
        };
        Ok(Some(Expr {
            span: call.span,
            ty,
            kind,
        }))
    }

    /// `varta.regexp.compile(pattern, flags)`: the linked `regexp_compile`.
    fn regexp_compile_intrinsic(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if call.args.len() != 2 || call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(invalid(format!("{COMPILE_INTRINSIC} expects 2 arguments")));
        }
        let mut args = Vec::new();
        for arg in &call.args {
            let value = self.expr(&arg.expr)?;
            if value.ty != Type::String {
                return Err(type_error(format!(
                    "{COMPILE_INTRINSIC} requires string, found {}",
                    value.ty
                )));
            }
            args.push(value);
        }
        Ok(Expr {
            span: call.span,
            ty: Type::String,
            kind: E::Host(varta_common::HostOperation::RegExpCompile, args),
        })
    }

    fn prelude_class_id(&self, name: &str) -> Result<u32> {
        self.types
            .classes
            .prelude_class(name)
            .ok_or_else(|| CompilerError::Internal(format!("missing {name} prelude class")))
    }
    /// Whether `ty` is a match array (`RegExpExecArray`, `RegExpMatchArray`,
    /// and under v38 the `RegExpIndicesArray` of a `d` match).
    pub(super) fn match_array(&self, ty: Type) -> bool {
        let Type::Class(class) = ty else {
            return false;
        };
        self.types.profile.regexp()
            && [EXEC_ARRAY_CLASS, MATCH_ARRAY_CLASS, INDICES_ARRAY_CLASS]
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
    /// `match[key]`: element 0 (the whole match, which always took part) is
    /// a string (v38: read live, as the elements may change), other elements
    /// are `string | undefined` (a group that did not take part, or no
    /// element).
    pub(super) fn match_element(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
        key: &ast::Expr,
    ) -> Result<Expr> {
        let zero = matches!(stmt::unparen(key), ast::Expr::Lit(ast::Lit::Num(n)) if n.value == 0.0);
        if zero && !self.types.profile.full_regexp() {
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
        let read = self.maybe_index(member, values, key, element)?;
        if zero {
            // TypeScript types element 0 as present; only a program that
            // removed it reads `undefined`, which fails the assertion.
            return self.non_null_value(read, member.span);
        }
        Ok(read)
    }
    /// V38: `Array.isArray(value)` when `value` may be a match array (an
    /// array in JavaScript); `None` otherwise. `value` is evaluated once.
    pub(super) fn match_array_test(
        &mut self,
        value: &Expr,
        span: swc_common::Span,
    ) -> Option<Expr> {
        if !self.types.profile.full_regexp() {
            return None;
        }
        let boolean = |kind| Expr {
            span,
            ty: Type::Boolean,
            kind,
        };
        if self.match_array(value.ty) {
            return Some(boolean(E::Sequence(
                Box::new(value.clone()),
                Box::new(boolean(E::Integer(1))),
            )));
        }
        let matches: Vec<Type> = unions::members(value.ty, self.types)
            .into_iter()
            .filter(|ty| self.match_array(*ty))
            .collect();
        if !value.ty.is_cell() || matches.is_empty() {
            return None;
        }
        let saved = self.hidden(value.ty);
        let local = || Expr {
            span,
            ty: value.ty,
            kind: E::Local(saved),
        };
        let mut test = boolean(E::Host(
            self.types
                .profile
                .host_operation(varta_common::HostOperation::ValueIsArray),
            vec![local()],
        ));
        for member in matches {
            test = boolean(E::Conditional(
                Box::new(boolean(E::IsUnionMember(Box::new(local()), member))),
                Box::new(boolean(E::Integer(1))),
                Box::new(test),
            ));
        }
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        Some(boolean(E::ValueBlock {
            output,
            marker,
            roots: vec![saved],
            body: vec![S::Init(saved, value.clone()), S::Init(output, test)],
        }))
    }
    /// V38: whether `member` of a match array of type `ty` writes its
    /// elements: an element or `length`, not one of its own properties.
    pub(super) fn match_elements_place(&self, member: &ast::MemberExpr, ty: Type) -> bool {
        let own = matches!(&member.prop, ast::MemberProp::Ident(name)
            if OWN_MEMBERS.contains(&name.sym.as_ref()));
        self.match_elements(ty) && !own
    }
    /// V38: whether `ty` is a match array whose array members a program
    /// module reaches through its elements (the prelude reads its fields).
    fn match_elements(&self, ty: Type) -> bool {
        self.types.profile.full_regexp()
            && !engine::is_prelude_source(self.source)
            && self.match_array(ty)
    }
    /// V38: whether `name` is a member of a match array's elements rather
    /// than of the match array itself.
    pub(super) fn match_elements_member(&self, ty: Type, name: &str) -> bool {
        self.match_elements(ty) && !OWN_MEMBERS.contains(&name)
    }
    /// V38: a member read or member call of the match array `value` lowered
    /// on its elements: they bind to a hidden binding, and `rewrite` builds
    /// the expression with that binding in place of the receiver.
    pub(super) fn on_match_elements(
        &mut self,
        value: Expr,
        rewrite: impl FnOnce(ast::Expr) -> ast::Expr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let values = self.match_field(value, VALUES)?;
        self.on_bound(vec![values], |mut bound| rewrite(bound.remove(0)), expected)
    }
    /// `values`, evaluated in order into hidden bindings, then the
    /// expression `rewrite` builds over those bindings.
    fn on_bound(
        &mut self,
        values: Vec<Expr>,
        rewrite: impl FnOnce(Vec<ast::Expr>) -> ast::Expr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let span = values
            .first()
            .map_or(swc_common::DUMMY_SP, |value| value.span);
        self.scopes.push(std::collections::BTreeMap::new());
        let lowered = (|| {
            let mut slots = Vec::new();
            let mut idents = Vec::new();
            for value in &values {
                let name = format!("{ELEMENTS_BINDING}{}", self.next);
                self.declare(&name, Some(value.ty), true, false)?;
                slots.push(self.binding(&name)?.slot);
                idents.push(ast::Expr::Ident(ident(&name, span)));
            }
            let rewritten = rewrite(idents);
            Ok::<_, CompilerError>((slots, self.expr_expected(&rewritten, expected)?))
        })();
        self.scopes.pop();
        let (slots, result) = lowered?;
        let result = if result.ty == Type::Void {
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Undefined(Box::new(result)),
            }
        } else {
            result
        };
        let (ty, result_span) = (result.ty, result.span);
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let mut roots = slots.clone();
        roots.push(output);
        let mut body: Vec<S> = slots
            .into_iter()
            .zip(values)
            .map(|(slot, value)| S::Init(slot, value))
            .collect();
        body.push(S::Init(output, result));
        Ok(Expr {
            span: result_span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
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
            pattern,
            flags,
            compiled,
        } = regexp;
        let bits = flags.bits();
        let names = compiled.joined_names();
        let bytes = |units: &[u16]| units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
        let program = bytes(&compiled.units);
        let lean = bytes(&compiled.lean);
        let profile = self.types.profile;
        let mut args = vec![
            string(span, profile.string_bytes(&source)),
            string(span, profile.string_bytes(&flags.text())),
            number(span, f64::from(bits)),
            string(span, program),
            string(span, lean),
            string(span, profile.string_bytes(&names)),
            number(span, (compiled.groups + 1) as f64),
        ];
        // V38 keeps [[OriginalSource]] for `new RegExp(regexp, flags)`.
        if profile.full_regexp() {
            args.push(string(span, profile.string_bytes(&pattern)));
        }
        self.regexp_static("compiled", args, span)
    }
    /// A pattern error as this profile reports it.
    fn regexp_error(&self, error: crate::regexp::RegExpError) -> CompilerError {
        if self.types.profile.full_regexp() {
            error.into_full_compiler()
        } else {
            error.into_compiler()
        }
    }
    /// `/pattern/flags`: a new RegExp per evaluation.
    pub(super) fn regexp_literal(&mut self, regex: &ast::Regex) -> Result<Expr> {
        let compiled = crate::regexp::literal(
            regex.exp.as_ref(),
            regex.flags.as_ref(),
            self.types.profile.full_regexp(),
        )
        .map_err(|e| self.source.locate(self.regexp_error(e), regex.span))?;
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
            if self.types.profile.full_regexp() {
                return self.regexp_runtime("create", args, new.span);
            }
            return Err(self.source.locate(unsupported(NON_CONSTANT), new.span));
        };
        let full = self.types.profile.full_regexp();
        match crate::regexp::constructed(&pattern, &flags, full) {
            Ok(compiled) => self.compiled_regexp(compiled, new.span),
            // V38: the constructor throws its SyntaxError when it runs.
            Err(crate::regexp::RegExpError::Syntax(_)) if full => {
                self.regexp_runtime("create", args, new.span)
            }
            Err(error) => Err(self.source.locate(self.regexp_error(error), new.span)),
        }
    }
    /// V38: `new RegExp(…)` (`create`) or `RegExp(…)` (`call`) whose
    /// pattern is compiled at run time by the linked pattern compiler.
    fn regexp_runtime(
        &mut self,
        target: &str,
        args: &[ast::ExprOrSpread],
        span: swc_common::Span,
    ) -> Result<Expr> {
        let mut values = Vec::new();
        for arg in args {
            values.push(self.expr(&arg.expr)?);
        }
        if values.is_empty() {
            values.push(string(span, Vec::new()));
        }
        if values.len() == 1 {
            values.push(Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            });
        }
        self.regexp_static(target, values, span)
    }
    /// V38: `RegExp(pattern?, flags?)` called without `new`
    /// (§22.2.4.1: a RegExp without flags is returned itself).
    pub(super) fn regexp_call(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let args = &call.args;
        if args.len() > 2 || args.iter().any(|arg| arg.spread.is_some()) || call.type_args.is_some()
        {
            return Err(self.source.locate(
                invalid("RegExp expects a pattern and optional flags"),
                call.span,
            ));
        }
        self.regexp_runtime("call", args, call.span)
    }
    /// The RegExp argument of `search`, `match` or `matchAll`: a RegExp
    /// value, or a string literal compiled like `new RegExp(text, flags)`.
    fn regexp_argument(&mut self, argument: &ast::Expr, flags: &str) -> Result<Expr> {
        let full = self.types.profile.full_regexp();
        let span = argument.span();
        let profile = self.types.profile;
        if let Some(text) = constant_text(argument) {
            match crate::regexp::constructed(&text, flags, full) {
                Ok(compiled) => return self.compiled_regexp(compiled, span),
                // V38: RegExpCreate throws its SyntaxError when it runs.
                Err(crate::regexp::RegExpError::Syntax(_)) if full => {
                    let pattern = string(span, profile.string_bytes(&text));
                    let flags = string(span, profile.string_bytes(flags));
                    return self.regexp_static("create", vec![pattern, flags], span);
                }
                Err(error) => return Err(self.source.locate(self.regexp_error(error), span)),
            }
        }
        let value = self.expr(argument)?;
        // V38: a string pattern only known at run time (RegExpCreate).
        if full && value.ty == Type::String {
            let flags = string(span, profile.string_bytes(flags));
            return self.regexp_static("create", vec![value, flags], span);
        }
        Ok(value)
    }
    /// V38: whether a value of type `ty` is a custom matcher for the string
    /// method `method`: a class instance, other than a RegExp, with the
    /// method `[Symbol.<method>]` (§22.1.3: the String methods call it).
    fn custom_matcher(&self, ty: Type, method: &str) -> bool {
        let Type::Class(class) = ty else {
            return false;
        };
        let symbol = if method == "replaceAll" {
            "replace"
        } else {
            method
        };
        let key = crate::modules::symbol_keys::well_known_key(symbol);
        self.types.profile.full_regexp()
            && self.types.profile.symbol_values()
            && !self.regexp_value(ty)
            && self.types.classes.get(class).methods.contains_key(&key)
    }
    /// V38: `text.method(matcher, …rest)` with a custom matcher:
    /// `matcher[Symbol.<method>](text, …rest)`, the receiver evaluated
    /// first, then the matcher, then the other arguments.
    fn custom_matcher_call(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        matcher: Expr,
        method: &str,
    ) -> Result<Expr> {
        let span = call.span;
        // IsRegExp (§7.2.8): a matcher with `[Symbol.match]` is a regular
        // expression to `replaceAll` and `matchAll`, which then require a
        // global `flags` property first.
        if matches!(method, "replaceAll" | "matchAll")
            && let Type::Class(class) = matcher.ty
            && self
                .types
                .classes
                .get(class)
                .methods
                .contains_key(&crate::modules::symbol_keys::well_known_key("match"))
        {
            return Err(self.source.locate(
                unsupported(&format!(
                    "String.prototype.{method} with a custom matcher that has [Symbol.match] (its IsRegExp flags check)"
                )),
                call.span,
            ));
        }
        let symbol = if method == "replaceAll" {
            "replace"
        } else {
            method
        };
        let key = crate::modules::symbol_keys::well_known_key(symbol);
        let rest: Vec<ast::ExprOrSpread> = call.args.iter().skip(1).cloned().collect();
        self.on_bound(
            vec![receiver, matcher],
            |bound| {
                let mut bound = bound.into_iter();
                let text = bound
                    .next()
                    .unwrap_or(ast::Expr::Invalid(ast::Invalid { span }));
                let matcher = bound
                    .next()
                    .unwrap_or(ast::Expr::Invalid(ast::Invalid { span }));
                let mut args = vec![ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(text),
                }];
                args.extend(rest);
                ast::Expr::Call(ast::CallExpr {
                    span,
                    ctxt: Default::default(),
                    callee: ast::Callee::Expr(Box::new(ast::Expr::Member(ast::MemberExpr {
                        span,
                        obj: Box::new(matcher),
                        prop: ast::MemberProp::Ident(ast::IdentName::new(key.into(), span)),
                    }))),
                    args,
                    type_args: None,
                })
            },
            None,
        )
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
                if self.custom_matcher(regexp.ty, method) {
                    return self.custom_matcher_call(call, receiver, regexp, method);
                }
                let regexp = self.require_regexp(regexp)?;
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
                // V38: a custom matcher's `[Symbol.replace]` (§22.1.3.19).
                if self.custom_matcher(pattern.ty, "replace") {
                    return self.custom_matcher_call(call, receiver, pattern, "replace");
                }
                let pattern = self.require_regexp(pattern)?;
                let argument = &*call.args[1].expr;
                if self.callback_argument(argument) {
                    let groups = self.static_groups(&pattern);
                    // V38: a replacer of a pattern the compiler cannot see,
                    // or with a rest parameter, takes the arguments as
                    // String.prototype.replace computes them at run time.
                    if self.types.profile.full_regexp()
                        && (groups.is_none() || rest_parameter(argument))
                    {
                        let replacer = self.dynamic_replacer(argument)?;
                        let target = if all { "replaceAllCall" } else { "replaceCall" };
                        return self.regexp_static(target, vec![receiver, pattern, replacer], span);
                    }
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
        // V38: a custom splitter's `[Symbol.split]` (§22.1.3.23).
        if self.custom_matcher(separator.ty, "split") {
            return self
                .custom_matcher_call(call, receiver, separator, "split")
                .map(Ok);
        }
        if !self.types.profile.regexp() || !self.regexp_value(separator.ty) {
            return Ok(Err((receiver, separator)));
        }
        let span = call.span;
        if call.args.len() > 2 {
            return Err(invalid("String.prototype.split expects 0 to 2 arguments"));
        }
        let limit = match call.args.get(1) {
            Some(arg) => {
                // V38: an optional limit (`undefined` is no limit).
                let limit = if self.types.profile.full_regexp()
                    && unions::members(
                        self.peek_type(&arg.expr, 0).unwrap_or(Type::Void),
                        self.types,
                    )
                    .contains(&Type::Undefined)
                {
                    self.expr(&ast::Expr::Bin(ast::BinExpr {
                        span,
                        op: ast::BinaryOp::NullishCoalescing,
                        left: arg.expr.clone(),
                        right: Box::new(ast::Expr::Lit(ast::Lit::Num(ast::Number {
                            span,
                            value: NO_LIMIT,
                            raw: None,
                        }))),
                    }))?
                } else {
                    self.expr(&arg.expr)?
                };
                if limit.ty != self.types.number() {
                    return Err(self
                        .source
                        .locate(type_error("split limit must be a number"), limit.span));
                }
                limit
            }
            None => number(span, NO_LIMIT),
        };
        // V38: a separator whose groups may not take part splits into
        // `(string | undefined)[]`; one written in place whose groups always
        // take part (or that has none) into `string[]`.
        let target = if !self.types.profile.full_regexp()
            || call
                .args
                .first()
                .and_then(|arg| static_pattern(&arg.expr))
                .and_then(|(pattern, flags)| {
                    crate::regexp::captures_always_take_part(&pattern, &flags)
                })
                .unwrap_or(false)
        {
            "split"
        } else {
            "splitParts"
        };
        self.regexp_static(target, vec![receiver, separator, limit], span)
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
    /// V38: a replacer adapted to `(args: unknown[]) => string`, where `args`
    /// are the arguments String.prototype.replace passes (the match, every
    /// capture, the position, the input and, with named groups, the groups
    /// object), counted at run time. Each parameter is a constant checked
    /// against its annotation (`as`: a `TypeError` when the argument does
    /// not fit), `unknown` without one (`string` for the match); a rest
    /// parameter takes the remaining arguments as `unknown[]`.
    fn dynamic_replacer(&mut self, callback: &ast::Expr) -> Result<Expr> {
        let span = callback.span();
        let context = self.static_parameter("replaceCall", 2)?;
        let (params, body, return_type) = match stmt::unparen(callback) {
            ast::Expr::Arrow(arrow)
                if !arrow.is_async && !arrow.is_generator && arrow.type_params.is_none() =>
            {
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
                (arrow.params.clone(), body, arrow.return_type.clone())
            }
            ast::Expr::Fn(function)
                if !function.function.is_async
                    && !function.function.is_generator
                    && function.function.type_params.is_none()
                    && function.ident.is_none() =>
            {
                let body = function.function.body.clone().ok_or_else(|| {
                    self.source
                        .locate(unsupported("replacer function without a body"), span)
                })?;
                let params = function
                    .function
                    .params
                    .iter()
                    .map(|param| param.pat.clone());
                (
                    params.collect(),
                    body,
                    function.function.return_type.clone(),
                )
            }
            ast::Expr::Ident(id) => {
                let body = self.dynamic_replacer_call(callback, id)?;
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
        let mut stmts = Vec::new();
        for (position, param) in params.iter().enumerate() {
            let declaration = dynamic_parameter(param, position, span)
                .map_err(|e| self.source.locate(e, param.span()))?;
            stmts.push(declaration);
        }
        stmts.extend(body.stmts);
        let adapter = ast::Expr::Arrow(ast::ArrowExpr {
            span,
            ctxt: Default::default(),
            params: vec![ast::Pat::Ident(ast::BindingIdent {
                id: ident(ARGUMENTS_PARAMETER, span),
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
    /// The body of a run-time replacer adapter for a function binding: a
    /// call with each argument checked against the parameter's type.
    fn dynamic_replacer_call(
        &mut self,
        callback: &ast::Expr,
        id: &ast::Ident,
    ) -> Result<ast::FunctionBody> {
        let span = callback.span();
        let value = self.expr(callback)?;
        let Type::Function(function) = value.ty else {
            return Err(self.source.locate(
                type_error(format!("replacer must be a function, found {}", value.ty)),
                span,
            ));
        };
        let params = self.types.functions[function as usize].params.clone();
        let mut args = Vec::new();
        for (position, param) in params.into_iter().enumerate() {
            let annotation = type_annotation(param, self.types, span).ok_or_else(|| {
                self.source.locate(
                    unsupported(&format!(
                        "replacer parameter of type {param} for a RegExp that is not written in place"
                    )),
                    span,
                )
            })?;
            args.push(ast::ExprOrSpread {
                spread: None,
                expr: Box::new(checked_argument(position, Some(annotation), span)),
            });
        }
        let call = ast::Expr::Call(ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(Box::new(ast::Expr::Ident(id.clone()))),
            args,
            type_args: None,
        });
        Ok(ast::FunctionBody {
            span,
            stmts: vec![ast::Stmt::Return(ast::ReturnStmt {
                span,
                arg: Some(Box::new(call)),
            })],
        })
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
        let full = self.types.profile.full_regexp();
        let source = |position: usize| replacer_source(position, groups, full, span);
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

/// `call` (of a member) with `receiver` as the member's object.
pub(super) fn call_on(call: &ast::CallExpr, receiver: ast::Expr) -> ast::Expr {
    let callee = match &call.callee {
        ast::Callee::Expr(callee) => match stmt::unparen(callee) {
            ast::Expr::Member(member) => ast::Callee::Expr(Box::new(member_on(member, receiver))),
            _ => call.callee.clone(),
        },
        other => other.clone(),
    };
    ast::Expr::Call(ast::CallExpr {
        callee,
        ..call.clone()
    })
}
/// `member` with `receiver` as its object.
pub(super) fn member_on(member: &ast::MemberExpr, receiver: ast::Expr) -> ast::Expr {
    ast::Expr::Member(ast::MemberExpr {
        obj: Box::new(receiver),
        ..member.clone()
    })
}

/// The pattern and flags of a regular expression written in place: a
/// literal, or `new RegExp` with constant arguments.
fn static_pattern(expression: &ast::Expr) -> Option<(String, String)> {
    match stmt::unparen(expression) {
        ast::Expr::Lit(ast::Lit::Regex(regex)) => {
            Some((regex.exp.to_string(), regex.flags.to_string()))
        }
        ast::Expr::New(new) if matches!(&*new.callee, ast::Expr::Ident(id) if id.sym == REGEXP_CLASS) =>
        {
            let args = new.args.as_deref().unwrap_or_default();
            if args.len() > 2 || args.iter().any(|arg| arg.spread.is_some()) {
                return None;
            }
            let text = |index: usize| match args.get(index) {
                None => Some(String::new()),
                Some(arg) => constant_text(&arg.expr),
            };
            Some((text(0)?, text(1)?))
        }
        _ => None,
    }
}

/// V38 (`full_regexp`): a match array entering `unknown` is the array of
/// its elements, as JavaScript readers see an array; other values are
/// unchanged.
pub(super) fn elements_value(value: Expr, types: &crate::ir::ValueTypes) -> Expr {
    let Type::Class(class) = value.ty else {
        return value;
    };
    let match_array = [EXEC_ARRAY_CLASS, MATCH_ARRAY_CLASS, INDICES_ARRAY_CLASS]
        .iter()
        .any(|name| types.classes.prelude_class(name) == Some(class));
    let Some((slot, field)) = types.classes.field(class, VALUES) else {
        return value;
    };
    if !types.profile.full_regexp() || !match_array {
        return value;
    }
    Expr {
        span: value.span,
        ty: field.ty,
        kind: E::Field(Box::new(value), slot),
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
    full: bool,
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
        // V38: a pattern with named groups always passes its groups object.
        p if p == count + 3 && named && full => Some((
            ast::Expr::TsNonNull(ast::TsNonNullExpr {
                span,
                expr: Box::new(field("groups")),
            }),
            false,
        )),
        p if p == count + 3 && named => Some((field("groups"), false)),
        _ => None,
    }
}

/// Whether a replacer function literal has a rest parameter.
fn rest_parameter(callback: &ast::Expr) -> bool {
    match stmt::unparen(callback) {
        ast::Expr::Arrow(arrow) => arrow
            .params
            .iter()
            .any(|param| matches!(param, ast::Pat::Rest(_))),
        ast::Expr::Fn(function) => function
            .function
            .params
            .iter()
            .any(|param| matches!(param.pat, ast::Pat::Rest(_))),
        _ => false,
    }
}

fn number_literal(value: usize, span: swc_common::Span) -> ast::Expr {
    ast::Expr::Lit(ast::Lit::Num(ast::Number {
        span,
        value: value as f64,
        raw: None,
    }))
}

fn keyword(kind: ast::TsKeywordTypeKind, span: swc_common::Span) -> ast::TsType {
    ast::TsType::TsKeywordType(ast::TsKeywordType { span, kind })
}

/// The run-time replacer argument at `position`, checked against
/// `annotation` (`as`) when there is one.
fn checked_argument(
    position: usize,
    annotation: Option<Box<ast::TsType>>,
    span: swc_common::Span,
) -> ast::Expr {
    let argument = ast::Expr::Member(ast::MemberExpr {
        span,
        obj: Box::new(ast::Expr::Ident(ident(ARGUMENTS_PARAMETER, span))),
        prop: ast::MemberProp::Computed(ast::ComputedPropName {
            span,
            expr: Box::new(number_literal(position, span)),
        }),
    });
    match annotation {
        Some(annotation) => ast::Expr::TsAs(ast::TsAsExpr {
            span,
            expr: Box::new(argument),
            type_ann: annotation,
        }),
        None => argument,
    }
}

/// The constant of a run-time replacer parameter at `position`: checked
/// against its annotation (the match is a `string`), `unknown` without
/// one, and a rest parameter as the remaining arguments.
fn dynamic_parameter(
    param: &ast::Pat,
    position: usize,
    span: swc_common::Span,
) -> Result<ast::Stmt> {
    match param {
        ast::Pat::Ident(binding) => {
            let annotation = binding
                .type_ann
                .as_ref()
                .map(|annotation| annotation.type_ann.clone())
                .or_else(|| {
                    (position == 0)
                        .then(|| Box::new(keyword(ast::TsKeywordTypeKind::TsStringKeyword, span)))
                });
            Ok(constant_declaration(
                binding.clone(),
                checked_argument(position, annotation, span),
                span,
            ))
        }
        ast::Pat::Rest(rest) => {
            let ast::Pat::Ident(binding) = &*rest.arg else {
                return Err(unsupported(
                    "RegExp replacer rest parameter other than an identifier",
                ));
            };
            let slice = ast::Expr::Call(ast::CallExpr {
                span,
                ctxt: Default::default(),
                callee: ast::Callee::Expr(Box::new(ast::Expr::Member(ast::MemberExpr {
                    span,
                    obj: Box::new(ast::Expr::Ident(ident(ARGUMENTS_PARAMETER, span))),
                    prop: ast::MemberProp::Ident(ast::IdentName {
                        span,
                        sym: "slice".into(),
                    }),
                }))),
                args: vec![ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(number_literal(position, span)),
                }],
                type_args: None,
            });
            let mut binding = binding.clone();
            if binding.type_ann.is_none() {
                binding.type_ann = rest.type_ann.clone();
            }
            Ok(constant_declaration(binding, slice, span))
        }
        _ => Err(unsupported(
            "RegExp replacer parameter other than a plain identifier",
        )),
    }
}

/// The annotation of a replacer parameter of type `ty`: primitives, their
/// unions and `unknown`; `None` for other types.
fn type_annotation(
    ty: Type,
    types: &crate::ir::ValueTypes,
    span: swc_common::Span,
) -> Option<Box<ast::TsType>> {
    use ast::TsKeywordTypeKind as K;
    let single = |ty: Type| {
        Some(Box::new(keyword(
            match ty {
                Type::String => K::TsStringKeyword,
                Type::Float64 => K::TsNumberKeyword,
                Type::Boolean => K::TsBooleanKeyword,
                Type::Undefined => K::TsUndefinedKeyword,
                Type::Null => K::TsNullKeyword,
                Type::Unknown => K::TsUnknownKeyword,
                _ => return None,
            },
            span,
        )))
    };
    let members = unions::members(ty, types);
    if members.len() <= 1 {
        return single(ty);
    }
    let types = members
        .into_iter()
        .map(single)
        .collect::<Option<Vec<_>>>()?;
    Some(Box::new(ast::TsType::TsUnionOrIntersectionType(
        ast::TsUnionOrIntersectionType::TsUnionType(ast::TsUnionType { span, types }),
    )))
}

/// `const <binding> = <init>;`.
fn constant_declaration(
    binding: ast::BindingIdent,
    init: ast::Expr,
    span: swc_common::Span,
) -> ast::Stmt {
    ast::Stmt::Decl(ast::Decl::Var(Box::new(ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Const,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: ast::Pat::Ident(binding),
            init: Some(Box::new(init)),
            definite: false,
        }],
    })))
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

#[cfg(test)]
mod tests {
    /// The v38 RegExp prelude mirrors the construction record of
    /// `regexp_compile` and the flag bits by name.
    #[test]
    fn the_regexp_prelude_mirrors_the_construction_record() {
        use varta_common::regexp::{bits, construction};
        const PRELUDE: &str = include_str!("../prelude/regexp_v38.ts");
        let constant = |name: &str, value: usize| {
            let line = format!("const {name} = {value};");
            assert!(PRELUDE.contains(&line), "regexp_v38.ts lacks `{line}`");
        };
        for (name, value) in [
            ("RECORD_STATUS", construction::STATUS),
            ("RECORD_OK", construction::OK as usize),
            ("RECORD_MESSAGE_UNIT", construction::MESSAGE_UNIT),
            ("RECORD_BITS", construction::BITS),
            ("RECORD_COUNT", construction::COUNT),
            ("RECORD_PROGRAM_UNITS", construction::PROGRAM_UNITS),
            ("RECORD_LEAN_UNITS", construction::LEAN_UNITS),
            ("RECORD_NAMES_UNITS", construction::NAMES_UNITS),
            ("RECORD_SOURCE_UNITS", construction::SOURCE_UNITS),
            ("RECORD_FLAGS_UNITS", construction::FLAGS_UNITS),
            ("RECORD_TEXT_UNIT", construction::TEXT_UNIT),
            ("GLOBAL", bits::GLOBAL as usize),
            ("IGNORE_CASE", bits::IGNORE_CASE as usize),
            ("MULTILINE", bits::MULTILINE as usize),
            ("DOT_ALL", bits::DOT_ALL as usize),
            ("UNICODE", bits::UNICODE as usize),
            ("STICKY", bits::STICKY as usize),
            ("HAS_INDICES", bits::HAS_INDICES as usize),
            ("UNICODE_SETS", bits::UNICODE_SETS as usize),
        ] {
            constant(name, value);
        }
    }
}
