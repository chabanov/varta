//! ECMAScript `Symbol` values (v38 `symbol_values`, K9 D2; ECMA-262 §20.4).
//!
//! A `symbol` is `Type::Symbol` while lowering and physically the handle of
//! a private record of the symbol prelude class (`modules::engine::SYMBOL`,
//! `SymbolValue`), whose block address is the symbol's identity; erasure
//! replaces the type by that class, and emission tags it with the value18
//! symbol tag in every cell, field, element and dictionary value
//! (`ir::tag_of`). `Symbol()`, `Symbol.for`, `Symbol.keyFor`, the
//! well-known symbols, `description`, `toString` and `String(symbol)` call
//! the prelude; `typeof`, `===` and `Object.is` need no prelude call.
use super::*;
use crate::modules::engine::{SYMBOL, SYMBOL_CLASS, SYMBOL_GLOBAL};

/// The program's use of symbols.
#[derive(Default)]
pub(crate) struct Symbols {
    /// Whether any value or annotation has the `symbol` type, so erasure
    /// needs the symbol record class.
    pub used: bool,
}

/// The `symbol` type, noting that the program uses it.
pub(super) fn use_type(types: &mut ir::ValueTypes) -> Type {
    types.symbols.used = true;
    Type::Symbol
}

/// The well-known symbols (§6.1.5.1, ES2025 and explicit resource
/// management) in the index order of the prelude's `WELL_KNOWN_NAMES`.
const WELL_KNOWN: [&str; 15] = [
    "asyncIterator",
    "hasInstance",
    "isConcatSpreadable",
    "iterator",
    "match",
    "matchAll",
    "replace",
    "search",
    "species",
    "split",
    "toPrimitive",
    "toStringTag",
    "unscopables",
    "asyncDispose",
    "dispose",
];

/// V8's message for ToNumber of a symbol.
const NUMBER_CONVERSION: &str = "Cannot convert a Symbol value to a number";

/// The well-known symbols whose members drive a protocol here
/// (`modules::symbol_keys::WELL_KNOWN_KEYS`).
const TO_PRIMITIVE: &str = "toPrimitive";
const TO_STRING_TAG: &str = "toStringTag";
const HAS_INSTANCE: &str = "hasInstance";
/// `Object.prototype.toString`'s text around the tag (§20.1.3.6).
const TAG_OPEN: &str = "[object ";
const TAG_CLOSE: &str = "]";

/// The index of the well-known symbol `Symbol.<name>`.
pub(super) fn well_known(name: &str) -> Option<usize> {
    WELL_KNOWN.iter().position(|known| *known == name)
}

impl Lower<'_> {
    /// Whether `expression` names the unshadowed global `Symbol`.
    pub(super) fn symbol_global(&self, expression: &ast::Expr) -> bool {
        self.types.profile.symbol_values()
            && matches!(stmt::unparen(expression), ast::Expr::Ident(id)
                if id.sym == SYMBOL_GLOBAL
                    && self.binding(SYMBOL_GLOBAL).is_err()
                    && !self.outer.contains_key(SYMBOL_GLOBAL)
                    && !self.functions.contains_key(SYMBOL_GLOBAL))
    }
    /// Whether `expression` statically is a `symbol`: a binding or path of
    /// that type, a well-known symbol, or a `Symbol(...)`/`Symbol.for(...)`
    /// call.
    fn symbol_expression(&self, expression: &ast::Expr) -> bool {
        if self.peek_type(expression, 0) == Some(Type::Symbol) {
            return true;
        }
        let well_known_member = |member: &ast::MemberExpr| {
            self.symbol_global(&member.obj)
                && matches!(&member.prop, ast::MemberProp::Ident(name)
                    if well_known(name.sym.as_ref()).is_some())
        };
        let symbol_for = |member: &ast::MemberExpr| {
            self.symbol_global(&member.obj)
                && matches!(&member.prop, ast::MemberProp::Ident(name) if name.sym == "for")
        };
        match stmt::unparen(expression) {
            ast::Expr::Member(member) => well_known_member(member),
            ast::Expr::Call(ast::CallExpr {
                callee: ast::Callee::Expr(callee),
                ..
            }) => {
                self.symbol_global(callee)
                    || matches!(stmt::unparen(callee), ast::Expr::Member(member) if symbol_for(member))
            }
            _ => false,
        }
    }
    /// The symbol record class of the loaded prelude.
    fn symbol_record(&self) -> Result<Type> {
        self.types
            .classes
            .prelude_class(SYMBOL_CLASS)
            .map(Type::Class)
            .ok_or_else(|| CompilerError::Internal("the symbol prelude is not loaded".into()))
    }
    /// A prelude result of the record class as a `symbol`.
    fn as_symbol(&mut self, record: Expr) -> Expr {
        use_type(self.types);
        Expr {
            span: record.span,
            ty: Type::Symbol,
            kind: E::Reinterpret(Box::new(record)),
        }
    }
    /// A `symbol` as the prelude's record class.
    fn symbol_storage(&self, value: Expr) -> Result<Expr> {
        Ok(Expr {
            span: value.span,
            ty: self.symbol_record()?,
            kind: E::Reinterpret(Box::new(value)),
        })
    }
    fn symbol_prelude(
        &mut self,
        name: &str,
        args: Vec<Expr>,
        span: swc_common::Span,
    ) -> Result<Expr> {
        self.engine_apply((&SYMBOL, name, &[]), args, span)
    }

    /// `Symbol(...)`, `Symbol.for(key)`, `Symbol.keyFor(symbol)` and the
    /// `toString`/`valueOf` methods of a `symbol` receiver.
    pub(super) fn symbol_call(&mut self, call: &ast::CallExpr) -> Result<Option<Expr>> {
        let ast::Callee::Expr(callee) = &call.callee else {
            return Ok(None);
        };
        let span = call.span;
        if self.symbol_global(callee) {
            return self.symbol_create(call).map(Some);
        }
        let ast::Expr::Member(member) = stmt::unparen(callee) else {
            return Ok(None);
        };
        let ast::MemberProp::Ident(method) = &member.prop else {
            return Ok(None);
        };
        let method = method.sym.as_ref();
        if self.symbol_global(&member.obj) {
            return match method {
                "for" => {
                    let [key] = self
                        .symbol_args(call, "Symbol.for", 1)?
                        .try_into()
                        .ok()
                        .ok_or_else(|| CompilerError::Internal("Symbol.for argument".into()))?;
                    if key.ty.erased() != Type::String {
                        return Err(self.source.locate(
                            type_error(format!(
                                "Symbol.for key must be a string, found {}",
                                key.ty
                            )),
                            key.span,
                        ));
                    }
                    let record = self.symbol_prelude("symbolFor", vec![key], span)?;
                    Ok(Some(self.as_symbol(record)))
                }
                "keyFor" => {
                    let [symbol] = self
                        .symbol_args(call, "Symbol.keyFor", 1)?
                        .try_into()
                        .ok()
                        .ok_or_else(|| CompilerError::Internal("Symbol.keyFor argument".into()))?;
                    if symbol.ty != Type::Symbol {
                        return Err(self.source.locate(
                            type_error(format!("{} is not a symbol", symbol.ty)),
                            symbol.span,
                        ));
                    }
                    let storage = self.symbol_storage(symbol)?;
                    self.symbol_prelude("symbolKeyFor", vec![storage], span)
                        .map(Some)
                }
                _ => Err(self
                    .source
                    .locate(unsupported(&format!("Symbol.{method} call")), span)),
            };
        }
        if !matches!(method, "toString" | "valueOf") || !self.symbol_expression(&member.obj) {
            return Ok(None);
        }
        if !call.args.is_empty() || call.type_args.is_some() {
            return Err(self.source.locate(
                invalid(format!("Symbol.prototype.{method} expects no arguments")),
                span,
            ));
        }
        let value = self.expr(&member.obj)?;
        if method == "valueOf" {
            return Ok(Some(value));
        }
        self.symbol_text(value).map(Some)
    }
    /// The lowered arguments of a `Symbol` static call taking `count`.
    fn symbol_args(&mut self, call: &ast::CallExpr, name: &str, count: usize) -> Result<Vec<Expr>> {
        if call.args.len() != count
            || call.type_args.is_some()
            || call.args.iter().any(|arg| arg.spread.is_some())
        {
            return Err(self.source.locate(
                invalid(format!("{name} expects {count} argument")),
                call.span,
            ));
        }
        call.args.iter().map(|arg| self.expr(&arg.expr)).collect()
    }
    /// `Symbol()` and `Symbol(description)` (§20.4.1.1): a string
    /// description is kept, `undefined` gives none, a number converts with
    /// `ToString`.
    fn symbol_create(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let span = call.span;
        if call.type_args.is_some() || call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(self.source.locate(
                unsupported("Symbol call with spread or type arguments"),
                span,
            ));
        }
        let Some((first, rest)) = call.args.split_first() else {
            let record = self.symbol_prelude("symbolCreate", vec![], span)?;
            return Ok(self.as_symbol(record));
        };
        let description = self.expr(&first.expr)?;
        // Extra arguments are evaluated after the description and ignored.
        let mut extra = Vec::new();
        for arg in rest {
            extra.push(S::Expr(self.expr(&arg.expr)?));
        }
        let members = unions::members(description.ty, self.types);
        let string = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
        let record = match description.ty.erased() {
            Type::String => self.symbol_prelude("symbolDescribed", vec![description], span)?,
            Type::Float64 | Type::Number => {
                let text = self.primitive_convert(description, &[], false, span)?;
                self.symbol_prelude("symbolDescribed", vec![text], span)?
            }
            Type::Undefined => {
                let create = self.symbol_prelude("symbolCreate", vec![], span)?;
                Expr {
                    span,
                    ty: create.ty,
                    kind: E::Sequence(Box::new(description), Box::new(create)),
                }
            }
            _ if members
                .iter()
                .all(|ty| matches!(ty.erased(), Type::String | Type::Undefined)) =>
            {
                let optional = unions::widen(description, Some(string), self.types)?;
                self.symbol_prelude("symbolOf", vec![optional], span)?
            }
            _ => {
                return Err(self.source.locate(
                    type_error(format!(
                        "Symbol description must be a string, a number or undefined, found {}",
                        description.ty
                    )),
                    description.span,
                ));
            }
        };
        let symbol = self.as_symbol(record);
        if extra.is_empty() {
            return Ok(symbol);
        }
        let output = self.hidden(Type::Symbol);
        let marker = self.hidden(Type::Number);
        let mut body = vec![S::Init(output, symbol)];
        body.extend(extra);
        Ok(Expr {
            span,
            ty: Type::Symbol,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![output],
                body,
            },
        })
    }
    /// `symbol.toString()` and `String(symbol)`: `Symbol(description)`.
    pub(super) fn symbol_text(&mut self, value: Expr) -> Result<Expr> {
        let span = value.span;
        let storage = self.symbol_storage(value)?;
        self.symbol_prelude("symbolToString", vec![storage], span)
    }

    /// `Number(symbol)`: the symbol is evaluated, then ToNumber throws a
    /// `TypeError` (§7.1.4).
    pub(super) fn symbol_number(&mut self, value: Expr, span: swc_common::Span) -> Result<Expr> {
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            NUMBER_CONVERSION,
            span,
        )?;
        let output = self.hidden(Type::Float64);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body: vec![S::Expr(value), S::Throw(error)],
            },
        })
    }
    /// The class of `ty` when it declares (or inherits) the instance member
    /// keyed by the well-known symbol `Symbol.<name>`, with whether that
    /// member is a getter.
    fn well_known_member(&self, ty: Type, name: &str) -> Option<(u32, bool)> {
        let Type::Class(class) = ty else {
            return None;
        };
        let key = crate::modules::symbol_keys::well_known_key(name);
        self.types
            .classes
            .get(class)
            .methods
            .get(&key)
            .map(|method| (class, method.getter))
    }
    /// A synthesized `receiver[Symbol.<name>](args)` member expression and
    /// call, whose receiver the caller passes already lowered.
    fn well_known_call(
        name: &str,
        args: Vec<ast::Expr>,
        span: swc_common::Span,
        receiver: ast::Expr,
    ) -> (ast::CallExpr, ast::MemberExpr) {
        let member = ast::MemberExpr {
            span,
            obj: Box::new(receiver),
            prop: ast::MemberProp::Ident(ast::IdentName::new(
                crate::modules::symbol_keys::well_known_key(name).into(),
                span,
            )),
        };
        let call = ast::CallExpr {
            span,
            callee: ast::Callee::Expr(Box::new(ast::Expr::Member(member.clone()))),
            args: args
                .into_iter()
                .map(|expr| ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(expr),
                })
                .collect(),
            ..Default::default()
        };
        (call, member)
    }
    /// ToPrimitive of a class instance with a `[Symbol.toPrimitive](hint)`
    /// method (§7.1.1): the method's primitive result, or the value back
    /// when its class has none. `hint` is `"string"`, `"number"` or
    /// `"default"`.
    pub(super) fn object_primitive(
        &mut self,
        value: Expr,
        hint: &str,
    ) -> Result<std::result::Result<Expr, Expr>> {
        if !self.types.profile.symbol_values() {
            return Ok(Err(value));
        }
        let Some((_, false)) = self.well_known_member(value.ty, TO_PRIMITIVE) else {
            return Ok(Err(value));
        };
        let span = value.span;
        let hint = ast::Expr::Lit(ast::Lit::Str(ast::Str {
            span,
            value: hint.into(),
            raw: None,
        }));
        let placeholder = ast::Expr::This(ast::ThisExpr { span });
        let (call, member) = Self::well_known_call(TO_PRIMITIVE, vec![hint], span, placeholder);
        let result = self.class_method_call(&call, &member, Some(value), None)?;
        if unions::members(result.ty, self.types)
            .iter()
            .any(|ty| !unions::primitive(*ty) && !matches!(ty, Type::BigInt | Type::Symbol))
        {
            return Err(self.source.locate(
                type_error(format!(
                    "[Symbol.toPrimitive] must return a primitive value, found {}",
                    result.ty
                )),
                span,
            ));
        }
        Ok(Ok(result))
    }
    /// `value instanceof C` for a class `C` with a static
    /// `[Symbol.hasInstance](value)` method (§13.10.2): ToBoolean of its
    /// result, or `None` when the class has none.
    pub(super) fn has_instance(
        &mut self,
        binary: &ast::BinExpr,
        class: u32,
    ) -> Result<Option<Expr>> {
        let key = crate::modules::symbol_keys::well_known_key(HAS_INSTANCE);
        if !self.types.profile.symbol_values()
            || !self.types.classes.get(class).statics.contains_key(&key)
        {
            return Ok(None);
        }
        let (call, member) = Self::well_known_call(
            HAS_INSTANCE,
            vec![(*binary.left).clone()],
            binary.span,
            (*binary.right).clone(),
        );
        let result = self.class_static_call(&call, &member, class, None)?;
        self.truth(result).map(Some)
    }
    /// `Object.prototype.toString` of a class instance with a
    /// `[Symbol.toStringTag]` getter (§20.1.3.6): `[object <tag>]`, or the
    /// value back when its class has none.
    pub(super) fn tagged_text(&mut self, value: Expr) -> Result<std::result::Result<Expr, Expr>> {
        if !self.types.profile.symbol_values() {
            return Ok(Err(value));
        }
        let Some((_, true)) = self.well_known_member(value.ty, TO_STRING_TAG) else {
            return Ok(Err(value));
        };
        let span = value.span;
        let placeholder = ast::Expr::This(ast::ThisExpr { span });
        let (_, member) = Self::well_known_call(TO_STRING_TAG, vec![], span, placeholder);
        let tag = self.class_member(&member, value, true)?;
        if tag.ty.erased() != Type::String {
            return Err(self.source.locate(
                type_error(format!(
                    "[Symbol.toStringTag] getter of type {} (a string is required)",
                    tag.ty
                )),
                span,
            ));
        }
        let text = |text: &str, this: &Self| Expr {
            span,
            ty: Type::String,
            kind: E::String(this.types.profile.string_bytes(text)),
        };
        let open = text(TAG_OPEN, self);
        let close = text(TAG_CLOSE, self);
        let concat = |a: Expr, b: Expr| Expr {
            span,
            ty: Type::String,
            kind: E::Binary(ast::BinaryOp::Add, Box::new(a), Box::new(b)),
        };
        Ok(Ok(concat(concat(open, tag), close)))
    }
    /// `Symbol.<name>` (a well-known symbol) and `symbol.description`.
    pub(super) fn symbol_member(&mut self, member: &ast::MemberExpr) -> Result<Option<Expr>> {
        let span = member.span;
        let ast::MemberProp::Ident(name) = &member.prop else {
            return Ok(None);
        };
        let name = name.sym.as_ref();
        if self.symbol_global(&member.obj) {
            let Some(index) = well_known(name) else {
                return Err(self
                    .source
                    .locate(unsupported(&format!("Symbol.{name}")), span));
            };
            let record = self.symbol_prelude(
                "wellKnownSymbol",
                vec![Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Float64(index as f64),
                }],
                span,
            )?;
            return Ok(Some(self.as_symbol(record)));
        }
        if !self.symbol_expression(&member.obj) {
            return Ok(None);
        }
        if name != "description" {
            return Err(self.source.locate(
                type_error(format!("property {name} does not exist on type symbol")),
                span,
            ));
        }
        let value = self.expr(&member.obj)?;
        let storage = self.symbol_storage(value)?;
        self.symbol_prelude("symbolDescription", vec![storage], span)
            .map(Some)
    }
}
