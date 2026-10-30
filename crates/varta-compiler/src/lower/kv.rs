//! Engine-internal intrinsics of the v38 KV prelude (`kv`). The plain ones
//! lower to one `kv_*` host import; `varta.kv.stage` and `varta.kv.value<T>`
//! choose by the static type of the value how it is stored (Deno KV's three
//! encodings: a `KvU64`, a top-level `Uint8Array`, or V8 serializer bytes of
//! any other checked value), and `varta.kv.value<T>`/`varta.kv.key<T>`
//! decode strictly into `T` like checked JSON parsing. They exist only
//! inside engine prelude modules: in program modules the names are ordinary
//! unknown calls, so application code reaches storage only through
//! `varta.kv.open`.
use super::*;
use crate::modules::engine::KV_U64_CLASS;
use std::collections::BTreeSet;
use varta_common::kv::StageMode;

/// `varta.kv.*` intrinsics with fixed signatures: (name, operation,
/// argument types, result type).
const INTRINSICS: [(&str, Host, &[Type], Type); 6] = [
    (
        "varta.kv.openFile",
        Host::KvOpen,
        &[Type::String],
        Type::Float64,
    ),
    (
        "varta.kv.read",
        Host::KvRead,
        &[Type::String],
        Type::Float64,
    ),
    (
        "varta.kv.commit",
        Host::KvCommit,
        &[Type::String],
        Type::Float64,
    ),
    (
        "varta.kv.entry",
        Host::KvEntry,
        &[Type::Float64, Type::Float64],
        Type::String,
    ),
    ("varta.kv.text", Host::KvText, &[], Type::String),
    (
        "varta.kv.release",
        Host::KvClose,
        &[Type::Float64],
        Type::Void,
    ),
];
const STAGE: &str = "varta.kv.stage";
const VALUE: &str = "varta.kv.value";
const KEY: &str = "varta.kv.key";
/// Prelude functions the type-directed intrinsics call.
const U64_TEXT: &str = "u64Text";
const U64_AT: &str = "u64At";
const BYTES_AT: &str = "bytesAt";

impl Lower<'_> {
    /// The intrinsic call `name(...)` of the KV prelude, if it is one.
    pub(super) fn prelude_kv(&mut self, call: &ast::CallExpr, name: &str) -> Result<Option<Expr>> {
        if !self.types.profile.kv()
            || !name.starts_with("varta.kv.")
            || !crate::modules::engine::is_prelude_source(self.source)
        {
            return Ok(None);
        }
        if let Some((_, host, parameters, result)) =
            INTRINSICS.iter().find(|(intrinsic, ..)| *intrinsic == name)
        {
            if call.type_args.is_some()
                || call.args.len() != parameters.len()
                || call.args.iter().any(|arg| arg.spread.is_some())
            {
                return Err(invalid(format!(
                    "{name} expects {} arguments",
                    parameters.len()
                )));
            }
            let mut args = Vec::with_capacity(parameters.len());
            for (argument, parameter) in call.args.iter().zip(parameters.iter()) {
                let value = self.expr(&argument.expr)?;
                if value.ty != *parameter {
                    return Err(type_error(format!(
                        "{name} requires a {parameter}, found {}",
                        value.ty
                    )));
                }
                args.push(value);
            }
            return Ok(Some(Expr {
                span: call.span,
                ty: *result,
                kind: E::Host(*host, args),
            }));
        }
        match name {
            STAGE => self.kv_stage_call(call).map(Some),
            VALUE | KEY => self.kv_read_call(call, name == KEY).map(Some),
            _ => Ok(None),
        }
    }

    /// Call the KV prelude's own function `name`: the type-directed
    /// intrinsics are lowered inside that prelude, whose functions are in
    /// scope there.
    fn kv_helper(&mut self, name: &str, args: Vec<Expr>, span: swc_common::Span) -> Result<Expr> {
        let signature = self.functions.get(name).cloned().ok_or_else(|| {
            CompilerError::Internal(format!("KV prelude function {name} is not in scope"))
        })?;
        let args = self.bind_arguments(args, &signature.params, &signature.shape, span)?;
        Ok(Expr {
            span,
            ty: signature.result,
            kind: E::Call(signature.index, args),
        })
    }

    /// Whether `ty` is the `KvU64` class of the KV prelude.
    fn kv_u64(&self, ty: Type) -> bool {
        matches!(ty, Type::Class(id) if self.types.classes.prelude_class(KV_U64_CLASS) == Some(id))
    }

    /// `varta.kv.stage(value)`: the stage id of `value`.
    fn kv_stage_call(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let [argument] = call.args.as_slice() else {
            return Err(invalid(format!("{STAGE} expects one value")));
        };
        if argument.spread.is_some() || call.type_args.is_some() {
            return Err(invalid(format!("{STAGE} expects one value")));
        }
        let value = self.expr(&argument.expr)?;
        let span = call.span;
        let (mode, value) = if matches!(
            value.ty,
            Type::Collection(ir::CollectionKind::Uint8Array, _)
        ) {
            // A top-level Uint8Array is stored as raw bytes: its byte array.
            (StageMode::Bytes, self.storage(value))
        } else if self.kv_u64(value.ty) {
            let text = self.kv_helper(U64_TEXT, vec![value], span)?;
            (StageMode::U64, text)
        } else {
            self.kv_checked(value.ty, span)?;
            let value = self.physical_array(value)?;
            (StageMode::Value, value)
        };
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::KvStage(mode as i32, Box::new(value)),
        })
    }

    /// `varta.kv.value<T>(index)` and `varta.kv.key<T>(index)`: entry `index`
    /// of the last read, at `T`.
    fn kv_read_call(&mut self, call: &ast::CallExpr, key: bool) -> Result<Expr> {
        let name = if key { KEY } else { VALUE };
        let [argument] = call.args.as_slice() else {
            return Err(invalid(format!("{name} expects one entry index")));
        };
        let target = call
            .type_args
            .as_ref()
            .filter(|args| args.params.len() == 1)
            .map(|args| &args.params[0])
            .ok_or_else(|| invalid(format!("{name} requires exactly one value type")))?;
        let ty =
            keyword_type(target, self.types).map_err(|e| self.source.locate(e, target.span()))?;
        let index = self.expr(&argument.expr)?;
        if argument.spread.is_some() || index.ty != Type::Float64 {
            return Err(invalid(format!("{name} expects one entry index")));
        }
        let span = call.span;
        if key {
            return self.kv_decode(Host::KvKey, index, ty, span);
        }
        if matches!(ty, Type::Collection(ir::CollectionKind::Uint8Array, _)) {
            return self.kv_helper(BYTES_AT, vec![index], span);
        }
        if self.kv_u64(ty) {
            return self.kv_helper(U64_AT, vec![index], span);
        }
        self.kv_checked(ty, span)?;
        self.kv_decode(Host::KvValue, index, ty, span)
    }

    /// Reject KV value types outside the checked subset with a diagnostic
    /// naming the type: class instances (other than a top-level `KvU64`),
    /// `Map`, `Set`, nested `Uint8Array`, `Date`, functions and promises
    /// have no stored form in this profile yet.
    fn kv_checked(&self, ty: Type, span: swc_common::Span) -> Result<()> {
        if !storable(ty, self.types, &mut BTreeSet::new()) {
            return Err(self.source.locate(
                unsupported(&format!(
                    "KV value type {ty}: KV values are records, dictionaries, arrays, unions, unknown and primitives other than BigInt; class instances (Date, a KvU64 inside a value), Map, Set, nested Uint8Array, functions and promises are not storable yet"
                )),
                span,
            ));
        }
        Ok(())
    }
}

/// Whether values of `ty` have a stored V8 form in this profile.
fn storable(ty: Type, types: &ir::ValueTypes, seen: &mut BTreeSet<Type>) -> bool {
    if !seen.insert(ty) {
        return true;
    }
    match ty {
        Type::Undefined
        | Type::Null
        | Type::Boolean
        | Type::Float64
        | Type::Number
        | Type::String
        | Type::Unknown
        | Type::Literal(..) => true,
        Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
            .clone()
            .into_iter()
            .all(|member| storable(member, types, seen)),
        Type::Record(id) => types.records[id as usize]
            .fields
            .clone()
            .into_iter()
            .all(|field| storable(field.ty, types, seen)),
        Type::Array(id) => storable(types.arrays[id as usize], types, seen),
        Type::Dictionary(id) => storable(types.dictionaries[id as usize].value, types, seen),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use varta_common::kv::{
        CHECK_FAILED, COMMITTED, DEFAULT_BATCH_SIZE, DESCRIPTOR_VERSION, EntryField, FAILED,
        MAX_DEFAULT_BATCH_SIZE, MutationCode, Selector,
    };

    const PRELUDE: &str = include_str!("../prelude/kv.ts");

    /// The prelude's mirrored constants match the shared KV contract.
    #[test]
    fn the_kv_prelude_mirrors_the_kv_contract() {
        let constant = |name: &str, value: &str| {
            let line = format!("const {name} = {value};");
            assert!(PRELUDE.contains(&line), "kv.ts lacks `{line}`");
        };
        constant("DESCRIPTOR_VERSION", &format!("'{DESCRIPTOR_VERSION}'"));
        constant("SEPARATOR", "'\\n'");
        constant("FAILED", &(FAILED as i32).to_string());
        constant("CHECK_FAILED", &(CHECK_FAILED as i32).to_string());
        constant("COMMITTED", &(COMMITTED as i32).to_string());
        constant("DEFAULT_BATCH_SIZE", &DEFAULT_BATCH_SIZE.to_string());
        constant(
            "MAX_DEFAULT_BATCH_SIZE",
            &MAX_DEFAULT_BATCH_SIZE.to_string(),
        );
        for (name, selector) in [
            ("SELECTOR_KEY", Selector::Key),
            ("SELECTOR_PREFIX", Selector::Prefix),
            ("SELECTOR_PREFIX_START", Selector::PrefixStart),
            ("SELECTOR_PREFIX_END", Selector::PrefixEnd),
            ("SELECTOR_START_END", Selector::StartEnd),
        ] {
            constant(name, &(selector as u8).to_string());
        }
        for (name, kind) in [
            ("MUTATION_SET", MutationCode::Set),
            ("MUTATION_DELETE", MutationCode::Delete),
            ("MUTATION_SUM", MutationCode::Sum),
            ("MUTATION_MIN", MutationCode::Min),
            ("MUTATION_MAX", MutationCode::Max),
        ] {
            constant(name, &(kind as u8).to_string());
        }
        for (name, field) in [
            ("FIELD_VERSIONSTAMP", EntryField::Versionstamp),
            ("FIELD_ENCODING", EntryField::Encoding),
            ("FIELD_U64", EntryField::U64),
            ("FIELD_BYTES", EntryField::Bytes),
            ("FIELD_CURSOR", EntryField::Cursor),
            ("FIELD_RANGE", EntryField::Range),
        ] {
            constant(name, &(field as u8).to_string());
        }
        constant(
            "ENCODING_U64",
            &format!("'{}'", varta_common::kv::ENCODING_U64),
        );
        constant(
            "ENCODING_BYTES",
            &format!("'{}'", varta_common::kv::ENCODING_BYTES),
        );
    }
}
