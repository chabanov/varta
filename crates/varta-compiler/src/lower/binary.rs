//! `Uint8Array` (v37 `collections`): a fixed-length view type over a dense
//! `number[]` of byte values, operated by the binary prelude
//! (`modules::engine::BINARY`). Writes convert with ToUint8 and ignore
//! out-of-range indexes, as for a typed array; `subarray` (a view sharing the
//! buffer) and `ArrayBuffer` are not modelled and are rejected.
use super::*;
use crate::modules::engine::BINARY;
use ir::CollectionKind as K;

/// `Uint8Array.prototype` methods and their binary prelude functions.
const METHODS: [(&str, &str); 7] = [
    ("set", "bytesSet"),
    ("slice", "bytesSlice"),
    ("fill", "bytesFill"),
    ("indexOf", "bytesIndexOf"),
    ("includes", "bytesIncludes"),
    ("join", "bytesJoin"),
    ("at", "bytesAt"),
];

impl Lower<'_> {
    /// `new Uint8Array()`, `new Uint8Array(length)`, `new Uint8Array(array)`
    /// and `new Uint8Array(bytes)` (a copy).
    pub(super) fn bytes_new(
        &mut self,
        new: &ast::NewExpr,
        argument: Option<&ast::ExprOrSpread>,
    ) -> Result<Expr> {
        let span = new.span;
        if new.type_args.is_some() {
            return Err(self
                .source
                .locate(unsupported("Uint8Array type argument"), span));
        }
        let ty = collections::bytes(self.types)?;
        let Type::Collection(_, id) = ty else {
            return Err(invalid("missing Uint8Array type"));
        };
        let number = self.types.number();
        let value = match argument {
            None => Expr {
                span,
                ty: number,
                kind: E::Float64(0.0),
            },
            Some(argument) => self.expr(&argument.expr)?,
        };
        let value = self.physical_array(value)?;
        let function = match value.ty {
            ty if ty == number => "bytesOfLength",
            Type::Collection(K::Uint8Array, _) => "bytesCopy",
            Type::Array(element) if self.types.arrays[element as usize] == number => {
                "bytesFromArray"
            }
            other => {
                return Err(self.source.locate(
                    unsupported(&format!(
                        "Uint8Array constructor argument {other}; pass a length, a number array or a Uint8Array (ArrayBuffer is not modelled)"
                    )),
                    value.span,
                ));
            }
        };
        let value = if matches!(value.ty, Type::Collection(..)) {
            self.storage(value)
        } else {
            value
        };
        let storage = self.engine_apply((&BINARY, function, &[]), vec![value], span)?;
        Ok(Expr {
            span,
            ty: Type::Collection(K::Uint8Array, id),
            kind: E::Reinterpret(Box::new(storage)),
        })
    }
    /// A `Uint8Array` method call.
    pub(super) fn bytes_method(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        id: u32,
        name: &str,
    ) -> Result<Expr> {
        let span = call.span;
        if name == "subarray" {
            return Err(self.source.locate(
                unsupported("Uint8Array.prototype.subarray (a view sharing the buffer); use slice for a copy"),
                span,
            ));
        }
        let Some((_, function)) = METHODS.iter().find(|(method, _)| *method == name) else {
            return Err(self
                .source
                .locate(unsupported(&format!("Uint8Array.prototype.{name}")), span));
        };
        let storage = self.storage(receiver);
        if name == "set" {
            // The source may be a number array or another Uint8Array.
            let Some(source) = call.args.first() else {
                return Err(self
                    .source
                    .locate(invalid("Uint8Array.set expects a source array"), span));
            };
            let source_value = self.expr(&source.expr)?;
            let source_value = self.physical_array(self.storage(source_value))?;
            return self.engine_call(
                (&BINARY, function, &[]),
                vec![storage, source_value],
                &call.args[1..],
                span,
            );
        }
        let result = self.engine_call((&BINARY, function, &[]), vec![storage], &call.args, span)?;
        if name == "slice" || name == "fill" {
            return Ok(Expr {
                span,
                ty: Type::Collection(K::Uint8Array, id),
                kind: E::Reinterpret(Box::new(result)),
            });
        }
        Ok(result)
    }
    /// `bytes[index] = value`: ToUint8, ignored outside the array.
    pub(super) fn bytes_write(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
        right: &ast::Expr,
    ) -> Result<Expr> {
        let ast::MemberProp::Computed(key) = &member.prop else {
            return Err(self.source.locate(
                unsupported("Uint8Array property assignment other than an index"),
                member.span,
            ));
        };
        let number = self.types.number();
        let storage = self.storage(base);
        let index = self.expr(&key.expr)?;
        if index.ty != number {
            return Err(self
                .source
                .locate(type_error("Uint8Array index must be a number"), index.span));
        }
        let value = self.expr_expected(right, Some(number))?;
        if value.ty != number {
            return Err(self.source.locate(
                type_error(format!(
                    "Uint8Array element assignment requires number, found {}",
                    value.ty
                )),
                value.span,
            ));
        }
        self.engine_apply(
            (&BINARY, "bytesStore", &[]),
            vec![storage, index, value],
            member.span,
        )
    }
}
