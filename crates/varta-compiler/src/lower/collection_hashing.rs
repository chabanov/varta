//! Hashed `Map` and `Set` tables (v38 `collection_hashing`): the engine
//! intrinsics of the v38 collections prelude (`prelude/collections_hashed.ts`).
//!
//! A table is a `number[]` whose capacity equals its power-of-two length,
//! with the dictionary table encoding (`lower::dictionary_hashing`): a hole
//! is an empty slot, a present `+0` a tombstone and a present `p + 1` the
//! entry at position `p` of the key vector. `varta.collections.table(n)`
//! reserves `n` empty slots, and the probes select their guest helper by the
//! key type ([`ir::KeyHash`]): string and BigInt keys (canonical code-unit
//! payloads) hash with FNV-1a and compare by content, sharing the dictionary
//! probes where one exists; numbers and the identity-compared keys (booleans
//! and references) use the [`ir::Intrinsic::Collection`] probes.
use super::*;
use ir::{CollectionProbe, KeyHash};

/// The probe intrinsics and their operand count.
const PROBES: [(&str, CollectionProbe, usize); 4] = [
    ("varta.collections.lookup", CollectionProbe::Find, 3),
    ("varta.collections.unlink", CollectionProbe::Unlink, 3),
    ("varta.collections.insert", CollectionProbe::Insert, 4),
    ("varta.collections.indexKeys", CollectionProbe::Index, 2),
];
const TABLE: &str = "varta.collections.table";

/// How a key slot of type `key` hashes and compares.
fn key_hash(key: Type) -> Result<KeyHash> {
    match key {
        Type::String | Type::BigInt => Ok(KeyHash::Units),
        Type::Float64 => Ok(KeyHash::Number),
        Type::Boolean => Ok(KeyHash::Identity),
        key if key.is_handle() && !key.is_cell() => Ok(KeyHash::Identity),
        key => Err(type_error(format!("collection key type {key} has no hash"))),
    }
}

/// The intrinsic of `probe` for keys hashed as `hash`: the dictionary probe
/// of the same operands for code-unit keys, where one exists.
fn intrinsic(probe: CollectionProbe, hash: KeyHash) -> ir::Intrinsic {
    match (hash, probe) {
        (KeyHash::Units, CollectionProbe::Find) => ir::Intrinsic::DictionaryFind,
        (KeyHash::Units, CollectionProbe::Unlink) => ir::Intrinsic::DictionaryUnlink,
        (KeyHash::Units, CollectionProbe::Index) => ir::Intrinsic::DictionaryIndex,
        _ => ir::Intrinsic::Collection(probe, hash),
    }
}

/// The operand positions of a probe: the key vector, the table, the key and
/// the entry position (each when the probe takes it).
struct Operands {
    keys: Option<usize>,
    table: usize,
    key: Option<usize>,
    position: Option<usize>,
}
fn operands(probe: CollectionProbe) -> Operands {
    match probe {
        CollectionProbe::Find | CollectionProbe::Unlink => Operands {
            keys: Some(0),
            table: 1,
            key: Some(2),
            position: None,
        },
        CollectionProbe::Insert => Operands {
            keys: Some(0),
            table: 1,
            key: Some(2),
            position: Some(3),
        },
        CollectionProbe::Index => Operands {
            keys: Some(0),
            table: 1,
            key: None,
            position: None,
        },
    }
}

impl Lower<'_> {
    /// `varta.collections.table(capacity)` and the table probes, inside the
    /// v38 collections prelude.
    pub(super) fn collection_table(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.collection_hashing() {
            return Ok(None);
        }
        let probe = PROBES.iter().find(|(intrinsic, ..)| *intrinsic == name);
        if probe.is_none() && name != TABLE {
            return Ok(None);
        }
        if call.type_args.is_some() || call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(invalid(format!("{name} takes plain arguments")));
        }
        let span = call.span;
        let number = self.types.number();
        let table_ty = arrays::intern(number, self.types)?;
        let Some(&(_, probe, count)) = probe else {
            let [capacity] = call.args.as_slice() else {
                return Err(invalid(format!("{name} expects a capacity")));
            };
            let capacity = self.expr(&capacity.expr)?;
            if capacity.ty != number {
                return Err(type_error(format!("{name} requires a number capacity")));
            }
            return Ok(Some(Expr {
                span,
                ty: table_ty,
                kind: E::ArrayReserve {
                    length: Box::new(capacity),
                    count: self.hidden(Type::Number),
                    bytes: self.hidden(Type::Number),
                },
            }));
        };
        if call.args.len() != count {
            return Err(invalid(format!("{name} expects {count} arguments")));
        }
        let args = call
            .args
            .iter()
            .map(|arg| self.expr(&arg.expr))
            .collect::<Result<Vec<_>>>()?;
        let at = operands(probe);
        if args[at.table].ty != table_ty {
            return Err(type_error(format!("{name} requires a number[] table")));
        }
        if at
            .position
            .is_some_and(|position| args[position].ty != number)
        {
            return Err(type_error(format!("{name} requires a number position")));
        }
        let element = match at.keys.map(|keys| args[keys].ty) {
            Some(Type::Array(id)) => Some(self.types.arrays[id as usize]),
            Some(_) => return Err(type_error(format!("{name} requires a key array"))),
            None => None,
        };
        let key = at.key.map(|key| args[key].ty.erased());
        let key_ty = match (element, key) {
            (Some(element), Some(key)) if element != key => {
                return Err(type_error(format!(
                    "{name} key type {key} does not match its key array"
                )));
            }
            (Some(ty), _) | (None, Some(ty)) => ty,
            (None, None) => return Err(CompilerError::Internal(format!("{name} has no key"))),
        };
        Ok(Some(Expr {
            span,
            ty: number,
            kind: E::Intrinsic(intrinsic(probe, key_hash(key_ty)?), args),
        }))
    }
}
