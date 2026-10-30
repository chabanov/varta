//! Engine-internal code-unit intrinsics of the v35 Web type preludes. Header
//! names and values are validated and normalized per UTF-16 code unit; doing
//! that with `charCodeAt` would make one host call per unit and exhaust the
//! production host-call and fuel budgets after a few headers. These calls lower
//! to guest helpers instead (`ir::TextUnits`). They exist only inside engine
//! prelude modules: in program modules the names are ordinary unknown calls.
use super::*;
use ir::{TextUnits, UnitClass};

const INTRINSICS: [(&str, TextUnits); 7] = [
    (
        "varta.text.indexNonAscii",
        TextUnits::Index(UnitClass::NonAscii),
    ),
    ("varta.text.asciiUppercase", TextUnits::AsciiUppercase),
    (
        "varta.web.indexAboveByte",
        TextUnits::Index(UnitClass::AboveByte),
    ),
    (
        "varta.web.indexNonToken",
        TextUnits::Index(UnitClass::NonToken),
    ),
    (
        "varta.web.indexForbiddenValue",
        TextUnits::Index(UnitClass::ForbiddenValue),
    ),
    ("varta.web.asciiLowercase", TextUnits::AsciiLowercase),
    (
        "varta.web.trimHttpWhitespace",
        TextUnits::TrimHttpWhitespace,
    ),
];

impl Lower<'_> {
    /// The intrinsic call `name(text)` of an engine prelude, if it is one.
    pub(super) fn web_text(&mut self, call: &ast::CallExpr, name: &str) -> Result<Option<Expr>> {
        if !crate::modules::engine::is_prelude_source(self.source) || !self.types.profile.utf16() {
            return Ok(None);
        }
        if let Some(value) = self.engine_intrinsic(call, name)? {
            return Ok(Some(value));
        }
        let Some((_, op)) = INTRINSICS.iter().find(|(intrinsic, _)| *intrinsic == name) else {
            return Ok(None);
        };
        if name.starts_with("varta.text.") && !self.types.profile.collections() {
            return Ok(None);
        }
        let [argument] = call.args.as_slice() else {
            return Err(invalid(format!("{name} expects one string argument")));
        };
        if argument.spread.is_some() || call.type_args.is_some() {
            return Err(invalid(format!("{name} expects one string argument")));
        }
        let value = self.expr(&argument.expr)?;
        if value.ty != Type::String {
            return Err(type_error(format!("{name} requires a string")));
        }
        let ty = match op {
            TextUnits::Index(_) => self.types.number(),
            TextUnits::AsciiLowercase
            | TextUnits::AsciiUppercase
            | TextUnits::TrimHttpWhitespace => Type::String,
        };
        Ok(Some(Expr {
            span: call.span,
            ty,
            kind: E::TextUnits {
                value: Box::new(value),
                op: *op,
            },
        }))
    }
}

/// The v37 engine intrinsics (`ir::Intrinsic`) and their operand types.
const ENGINE_INTRINSICS: [(&str, ir::Intrinsic); 3] = [
    ("varta.text.unit", ir::Intrinsic::CodeUnit),
    ("varta.text.fromUnits", ir::Intrinsic::FromUnits),
    ("varta.text.identity", ir::Intrinsic::Identity),
];
/// The v37 pure case-conversion host operations of the text prelude.
const CASE_CONVERSIONS: [(&str, Host); 2] = [
    ("varta.text.toUpperCase", Host::StringToUpperCase16),
    ("varta.text.toLowerCase", Host::StringToLowerCase16),
];
impl Lower<'_> {
    /// A v37 engine intrinsic call of a prelude module (`collections`).
    fn engine_intrinsic(&mut self, call: &ast::CallExpr, name: &str) -> Result<Option<Expr>> {
        if !self.types.profile.collections() {
            return Ok(None);
        }
        if let Some((_, host)) = CASE_CONVERSIONS
            .iter()
            .find(|(intrinsic, _)| *intrinsic == name)
        {
            let [argument] = call.args.as_slice() else {
                return Err(invalid(format!("{name} expects one string argument")));
            };
            let value = self.expr(&argument.expr)?;
            if value.ty != Type::String || argument.spread.is_some() {
                return Err(type_error(format!("{name} requires a string")));
            }
            return Ok(Some(Expr {
                span: call.span,
                ty: Type::String,
                kind: E::Host(*host, vec![value]),
            }));
        }
        if name == "varta.date.now" {
            if !call.args.is_empty() {
                return Err(invalid(format!("{name} expects no arguments")));
            }
            return Ok(Some(Expr {
                span: call.span,
                ty: self.types.number(),
                kind: E::Host(Host::DateNow, vec![]),
            }));
        }
        if name == "varta.array.get" {
            // An element read without the `undefined` of a hole or an
            // out-of-range index: the engine prelude only reads indexes it
            // proved in range of arrays it filled densely (a stray index traps).
            let [array, index] = call.args.as_slice() else {
                return Err(invalid(format!("{name} expects an array and an index")));
            };
            if array.spread.is_some() || index.spread.is_some() || call.type_args.is_some() {
                return Err(invalid(format!("{name} takes plain arguments")));
            }
            let array = self.expr(&array.expr)?;
            let index = self.expr(&index.expr)?;
            let Type::Array(id) = array.ty else {
                return Err(type_error(format!("{name} requires an array")));
            };
            if index.ty != self.types.number() {
                return Err(type_error(format!("{name} requires a number index")));
            }
            return Ok(Some(Expr {
                span: call.span,
                ty: self.types.arrays[id as usize],
                kind: E::Index(Box::new(array), Box::new(index)),
            }));
        }
        if let Some(value) = self.collection_intrinsic(call, name)? {
            return Ok(Some(value));
        }
        let Some((_, op)) = ENGINE_INTRINSICS
            .iter()
            .find(|(intrinsic, _)| *intrinsic == name)
        else {
            return Ok(None);
        };
        if call.type_args.is_some() || call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(invalid(format!("{name} takes plain arguments")));
        }
        let args = call
            .args
            .iter()
            .map(|arg| self.expr(&arg.expr))
            .collect::<Result<Vec<_>>>()?;
        let number = self.types.number();
        let valid = match (op, args.as_slice()) {
            (ir::Intrinsic::CodeUnit, [text, index]) => {
                text.ty == Type::String && index.ty == number
            }
            (ir::Intrinsic::FromUnits, [units]) => {
                matches!(units.ty, Type::Array(id) if self.types.arrays[id as usize] == number)
            }
            (ir::Intrinsic::Identity, [value]) => {
                value.ty.is_handle()
                    && !matches!(value.ty, Type::String | Type::BigInt)
                    && !value.ty.is_cell()
            }
            _ => false,
        };
        if !valid {
            return Err(type_error(format!("invalid operands for {name}")));
        }
        let ty = if *op == ir::Intrinsic::FromUnits {
            Type::String
        } else {
            number
        };
        Ok(Some(Expr {
            span: call.span,
            ty,
            kind: E::Intrinsic(*op, args),
        }))
    }
}

impl Lower<'_> {
    /// `varta.collections.hash(key)` and `varta.collections.key(key)` select
    /// the hash and SameValueZero key normalization of the key's type;
    /// `varta.bytes.view`/`adopt` reinterpret a `Uint8Array` and its storage.
    fn collection_intrinsic(&mut self, call: &ast::CallExpr, name: &str) -> Result<Option<Expr>> {
        if !matches!(
            name,
            "varta.collections.hash"
                | "varta.collections.key"
                | "varta.bytes.view"
                | "varta.bytes.adopt"
        ) {
            return Ok(None);
        }
        let [argument] = call.args.as_slice() else {
            return Err(invalid(format!("{name} expects one argument")));
        };
        if argument.spread.is_some() || call.type_args.is_some() {
            return Err(invalid(format!("{name} takes a plain argument")));
        }
        let value = self.expr(&argument.expr)?;
        let span = call.span;
        let number = self.types.number();
        let ty = value.ty.erased();
        match name {
            "varta.bytes.view" => {
                if !matches!(ty, Type::Collection(ir::CollectionKind::Uint8Array, _)) {
                    return Err(type_error(format!("{name} requires a Uint8Array")));
                }
                Ok(Some(self.storage(value)))
            }
            "varta.bytes.adopt" => {
                let bytes = collections::bytes(self.types)?;
                let Type::Collection(_, id) = bytes else {
                    return Err(invalid("missing Uint8Array type"));
                };
                if value.ty != self.types.collections[id as usize].storage {
                    return Err(type_error(format!("{name} requires a number array")));
                }
                Ok(Some(Expr {
                    span,
                    ty: bytes,
                    kind: E::Reinterpret(Box::new(value)),
                }))
            }
            "varta.collections.key" => Ok(Some(if ty == number {
                // SameValueZero stores -0 as +0 (-0 + 0 is +0, NaN stays NaN).
                Expr {
                    span,
                    ty: value.ty,
                    kind: E::Binary(
                        ast::BinaryOp::Add,
                        Box::new(value),
                        Box::new(Expr {
                            span,
                            ty: number,
                            kind: E::Float64(0.0),
                        }),
                    ),
                }
            } else {
                value
            })),
            _ => {
                let function = match ty {
                    Type::String => Some("hashString"),
                    ty if ty == number => Some("hashNumber"),
                    Type::Boolean => Some("hashBoolean"),
                    _ => None,
                };
                if let Some(function) = function {
                    let value = Expr { ty, ..value };
                    return self
                        .engine_apply(
                            (&crate::modules::engine::COLLECTIONS, function, &[]),
                            vec![value],
                            span,
                        )
                        .map(Some);
                }
                if ty == Type::BigInt {
                    // BigInt keys compare by value: one shared chain.
                    return Ok(Some(Expr {
                        span,
                        ty: number,
                        kind: E::Sequence(
                            Box::new(value),
                            Box::new(Expr {
                                span,
                                ty: number,
                                kind: E::Float64(0.0),
                            }),
                        ),
                    }));
                }
                if !ty.is_handle() || ty.is_cell() {
                    return Err(type_error(format!("collection key type {ty} has no hash")));
                }
                Ok(Some(Expr {
                    span,
                    ty: number,
                    kind: E::Intrinsic(ir::Intrinsic::Identity, vec![value]),
                }))
            }
        }
    }
}
