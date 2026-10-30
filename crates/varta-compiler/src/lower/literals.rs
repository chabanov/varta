//! String, number and boolean literal types (v32 `literal_types`).
//!
//! A literal set is a constrained primitive: `Type::Literal(base, id)` names an
//! interned value set and is physically its base primitive. Values read from
//! literal-typed places erase to the base whenever no literal context expects
//! them; a literal context accepts only constants or literal-typed values whose
//! sets are subsets. Every literal type is erased before emission, so no value
//! tag, layout, helper or host import changes.
use super::*;
use ir::LiteralBase;
use std::collections::BTreeSet;

/// Members of one literal set; sized for generated API code (error-code and
/// enum-like unions with hundreds of members).
pub(crate) const MAX_LITERAL_MEMBERS: usize = 4096;
/// Members across all interned literal sets of one compilation, flow
/// refinements included. Refinements that would exceed it are skipped.
pub(crate) const MAX_LITERAL_TABLE_MEMBERS: usize = 65_536;
/// Members of a union that are not literal types (the pre-v32 syntactic limit).
pub(crate) const MAX_UNION_MEMBERS: usize = 16;
/// Members shown when a diagnostic describes a literal set.
const DESCRIBED_MEMBERS: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LiteralValue {
    /// Profile-encoded bytes, exactly as `Expression::String` carries them.
    String(Vec<u8>),
    /// Binary64 bits with -0 normalized to +0; NaN is never a literal type.
    Number(u64),
    Boolean(bool),
}
impl LiteralValue {
    fn number(value: f64) -> Option<Self> {
        if value.is_nan() {
            return None;
        }
        Some(Self::Number(
            if value == 0.0 { 0.0f64 } else { value }.to_bits(),
        ))
    }
    fn base(&self) -> LiteralBase {
        match self {
            Self::String(_) => LiteralBase::String,
            Self::Number(_) => LiteralBase::Number,
            Self::Boolean(_) => LiteralBase::Boolean,
        }
    }
    fn describe(&self, profile: varta_common::language::LanguageProfile) -> String {
        match self {
            Self::String(bytes) if profile.utf16() => {
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .collect();
                format!("'{}'", String::from_utf16_lossy(&units))
            }
            Self::String(bytes) => format!("'{}'", String::from_utf8_lossy(bytes)),
            Self::Number(bits) => f64::from_bits(*bits).to_string(),
            Self::Boolean(value) => value.to_string(),
        }
    }
}

type Set = BTreeSet<LiteralValue>;

#[derive(Default)]
pub(crate) struct LiteralTypes {
    sets: Vec<(LiteralBase, Set)>,
    index: BTreeMap<(LiteralBase, Set), u32>,
    members: usize,
    /// Union containing literal members -> the same union with erased members.
    erased_unions: BTreeMap<u32, Type>,
}
impl LiteralTypes {
    fn set(&self, id: u32) -> &Set {
        &self.sets[id as usize].1
    }
}

/// Whether no value of the literal type `literal` is a value of `other`
/// (v37 discriminants of record unions).
pub(super) fn disjoint(literal: Type, other: Type, types: &ir::ValueTypes) -> bool {
    let Type::Literal(base, id) = literal else {
        return false;
    };
    let values = types.literals.set(id);
    super::unions::members(other, types)
        .into_iter()
        .all(|member| match member {
            Type::Literal(other_base, other_id) => {
                other_base != base || values.is_disjoint(types.literals.set(other_id))
            }
            member => member != base.erased() && member.erased() != base.erased(),
        })
}

/// Lower one literal type annotation (`'none'`, `42`, `-1`, `true`).
pub(super) fn annotation(literal: &ast::TsLitType, types: &mut ir::ValueTypes) -> Result<Type> {
    let value = match &literal.lit {
        ast::TsLit::Str(text) => LiteralValue::String(string_bytes(text, types.profile)?),
        ast::TsLit::Number(number) => {
            LiteralValue::number(number.value).ok_or_else(|| unsupported("NaN literal type"))?
        }
        ast::TsLit::Bool(value) => LiteralValue::Boolean(value.value),
        ast::TsLit::BigInt(_) => return Err(unsupported("bigint literal type")),
        ast::TsLit::Tpl(_) => return Err(unsupported("template literal type")),
    };
    intern(value.base(), Set::from([value]), types)
}

/// A union annotation: literal members merge into one constrained primitive per
/// base; at most `MAX_UNION_MEMBERS` other members remain structural.
pub(super) fn union(union: &ast::TsUnionType, types: &mut ir::ValueTypes) -> Result<Type> {
    if union.types.len() > MAX_LITERAL_MEMBERS + MAX_UNION_MEMBERS {
        return Err(invalid("union member count exceeds limit"));
    }
    let members = union
        .types
        .iter()
        .map(|ty| keyword_type(ty, types))
        .collect::<Result<Vec<_>>>()?;
    if members
        .iter()
        .filter(|ty| !matches!(ty, Type::Literal(..)))
        .count()
        > MAX_UNION_MEMBERS
    {
        return Err(invalid("union member count exceeds limit"));
    }
    unions::intern(members, types)
}

fn string_bytes(
    text: &ast::Str,
    profile: varta_common::language::LanguageProfile,
) -> Result<Vec<u8>> {
    Ok(if profile.utf16() {
        text.value
            .as_wtf8()
            .to_ill_formed_utf16()
            .flat_map(u16::to_le_bytes)
            .collect()
    } else {
        text.value
            .as_str()
            .ok_or_else(|| unsupported("non-UTF-8 string"))?
            .as_bytes()
            .to_vec()
    })
}

fn name_bytes(name: &str, profile: varta_common::language::LanguageProfile) -> Vec<u8> {
    if profile.utf16() {
        name.encode_utf16().flat_map(u16::to_le_bytes).collect()
    } else {
        name.as_bytes().to_vec()
    }
}

/// The string literal set of `names` (v32 `keyof` over record field names).
pub(super) fn string_set<'a>(
    names: impl IntoIterator<Item = &'a str>,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let values = names
        .into_iter()
        .map(|name| LiteralValue::String(name_bytes(name, types.profile)))
        .collect();
    intern(LiteralBase::String, values, types)
}

/// The members of a string literal set as record keys; `None` for any other
/// type or for a member that is not a well-formed key string.
pub(super) fn string_members(ty: Type, types: &ir::ValueTypes) -> Option<Vec<String>> {
    let Type::Literal(LiteralBase::String, id) = ty else {
        return None;
    };
    types
        .literals
        .set(id)
        .iter()
        .map(|value| match value {
            LiteralValue::String(bytes) if types.profile.utf16() => {
                let units: Vec<u16> = bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes(*pair))
                    .collect();
                String::from_utf16(&units).ok()
            }
            LiteralValue::String(bytes) => String::from_utf8(bytes.clone()).ok(),
            _ => None,
        })
        .collect()
}

fn intern(base: LiteralBase, values: Set, types: &mut ir::ValueTypes) -> Result<Type> {
    if values.is_empty() {
        return Err(invalid("empty literal type"));
    }
    if base == LiteralBase::Boolean && values.len() == 2 {
        return Ok(Type::Boolean);
    }
    if values.len() > MAX_LITERAL_MEMBERS {
        return Err(invalid("literal type member count exceeds limit"));
    }
    let table = &mut types.literals;
    let key = (base, values);
    if let Some(&id) = table.index.get(&key) {
        return Ok(Type::Literal(base, id));
    }
    if table.members + key.1.len() > MAX_LITERAL_TABLE_MEMBERS {
        return Err(invalid("literal type table exceeds limit"));
    }
    let id =
        u32::try_from(table.sets.len()).map_err(|_| invalid("literal type table exceeds limit"))?;
    table.members += key.1.len();
    table.sets.push(key.clone());
    table.index.insert(key, id);
    Ok(Type::Literal(base, id))
}

/// Merge literal members of a sorted union member list: one set per base, a
/// base primitive absorbs its literals and `true | false` is `boolean`.
pub(super) fn normalize(values: Vec<Type>, types: &mut ir::ValueTypes) -> Result<Vec<Type>> {
    if !values.iter().any(|ty| matches!(ty, Type::Literal(..))) {
        return Ok(values);
    }
    let mut groups: BTreeMap<LiteralBase, Vec<u32>> = BTreeMap::new();
    let mut rest = Vec::new();
    for ty in values {
        match ty {
            Type::Literal(base, id) => groups.entry(base).or_default().push(id),
            other => rest.push(other),
        }
    }
    for (base, ids) in groups {
        if rest.contains(&base.erased()) {
            continue;
        }
        let merged = if let [id] = ids[..] {
            Type::Literal(base, id)
        } else {
            let values = ids
                .iter()
                .flat_map(|id| types.literals.set(*id).iter().cloned())
                .collect();
            intern(base, values, types)?
        };
        rest.push(merged);
    }
    rest.sort();
    rest.dedup();
    Ok(rest)
}

/// Record the erased form of a newly interned union with literal members, so a
/// value read in a non-literal context carries only physical member types.
pub(super) fn note_union(id: u32, types: &mut ir::ValueTypes) -> Result<()> {
    let members = &types.unions[id as usize];
    if !members.iter().any(|ty| matches!(ty, Type::Literal(..))) {
        return Ok(());
    }
    let erased = members.iter().map(|ty| ty.erased()).collect();
    let erased = unions::intern(erased, types)?;
    types.literals.erased_unions.insert(id, erased);
    Ok(())
}

/// Give union `to` the erased form recorded for union `from`; whether one was.
pub(super) fn copy_erased_union(from: u32, to: u32, types: &mut ir::ValueTypes) -> bool {
    match types.literals.erased_unions.get(&from).copied() {
        Some(erased) => {
            types.literals.erased_unions.insert(to, erased);
            true
        }
        None => false,
    }
}
fn subset(inner: u32, outer: u32, types: &ir::ValueTypes) -> bool {
    inner == outer
        || types
            .literals
            .set(inner)
            .is_subset(types.literals.set(outer))
}

/// The literal value of a constant expression, if it is one.
fn constant(value: &Expr) -> Option<LiteralValue> {
    match (&value.kind, value.ty) {
        (E::String(bytes), Type::String) => Some(LiteralValue::String(bytes.clone())),
        (E::Float64(number), Type::Float64) => LiteralValue::number(*number),
        (E::Negative(inner), Type::Float64) => match inner.kind {
            E::Float64(number) => LiteralValue::number(-number),
            _ => None,
        },
        (E::Integer(flag), Type::Boolean) => Some(LiteralValue::Boolean(*flag != 0)),
        _ => None,
    }
}

/// Whether `value` statically belongs to literal set `target` of `base`.
fn fits(value: &Expr, base: LiteralBase, target: u32, types: &ir::ValueTypes) -> bool {
    match value.ty {
        Type::Literal(own, set) => own == base && subset(set, target, types),
        ty if ty == base.erased() => {
            constant(value).is_some_and(|v| types.literals.set(target).contains(&v))
        }
        _ => false,
    }
}

/// The physical type of `ty` in a context that expects no literal type.
pub(super) fn erased(ty: Type, types: &ir::ValueTypes) -> Type {
    match ty {
        Type::Literal(base, _) => base.erased(),
        Type::Union(id) | Type::ReferenceUnion(id) => {
            types.literals.erased_unions.get(&id).copied().unwrap_or(ty)
        }
        _ => ty,
    }
}
pub(super) fn erase(value: Expr, types: &ir::ValueTypes) -> Expr {
    Expr {
        ty: erased(value.ty, types),
        ..value
    }
}

/// Whether a union member of an actual type is accepted by target members.
pub(super) fn member_of(member: Type, target: &[Type], types: &ir::ValueTypes) -> bool {
    target.contains(&member)
        || matches!(member, Type::Literal(base, set) if target.contains(&base.erased())
            || target.iter().any(|t| matches!(*t, Type::Literal(b, outer) if b == base && subset(set, outer, types))))
}

/// Literal-aware scalar assignability of an already-typed value.
pub(super) fn assignable(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    match (expected, actual) {
        (Type::Literal(b, outer), Type::Literal(a, inner)) => b == a && subset(inner, outer, types),
        (_, Type::Literal(base, _)) => expected == base.erased(),
        _ => false,
    }
}

/// Retag `value` for a literal context, or erase it for any other context.
/// Union targets select their literal member here; boxing stays in `widen`.
pub(super) fn conform(value: Expr, expected: Option<Type>, types: &ir::ValueTypes) -> Result<Expr> {
    let Some(expected) = expected else {
        return Ok(value);
    };
    if !types.profile.literal_types() {
        return Ok(value);
    }
    match expected {
        Type::Literal(base, target) => {
            if fits(&value, base, target, types) {
                Ok(Expr {
                    ty: expected,
                    ..value
                })
            } else {
                Err(type_error(format!(
                    "{} is not assignable to literal type {}; narrow it first",
                    describe_value(&value, types),
                    describe(target, types)
                )))
            }
        }
        Type::Union(id) | Type::ReferenceUnion(id) if !value.ty.is_union() => {
            if let Some(&member) = types.unions[id as usize].iter().find(
                |m| matches!(m, Type::Literal(base, target) if fits(&value, *base, *target, types)),
            ) {
                return Ok(Expr {
                    ty: member,
                    ..value
                });
            }
            Ok(erase(value, types))
        }
        Type::Union(_) | Type::ReferenceUnion(_) => Ok(value),
        _ => Ok(erase(value, types)),
    }
}

fn describe(id: u32, types: &ir::ValueTypes) -> String {
    let set = types.literals.set(id);
    let mut text: Vec<_> = set
        .iter()
        .take(DESCRIBED_MEMBERS)
        .map(|v| v.describe(types.profile))
        .collect();
    if set.len() > DESCRIBED_MEMBERS {
        text.push(format!("… ({} members)", set.len()));
    }
    text.join(" | ")
}
fn describe_value(value: &Expr, types: &ir::ValueTypes) -> String {
    match (constant(value), value.ty) {
        (Some(v), _) => format!("value {}", v.describe(types.profile)),
        (None, Type::Literal(_, id)) => format!("value of type {}", describe(id, types)),
        (None, ty) => format!("value of type {ty}"),
    }
}

/// Singleton literal operand of an `===`/`!==` guard against a direct local.
pub(super) fn guard_operand(
    name: &ast::Expr,
    literal: &ast::Expr,
    types: &mut ir::ValueTypes,
) -> Option<(String, Type)> {
    let ast::Expr::Ident(id) = name else {
        return None;
    };
    let value = match literal {
        ast::Expr::Lit(ast::Lit::Str(text)) => {
            LiteralValue::String(string_bytes(text, types.profile).ok()?)
        }
        ast::Expr::Lit(ast::Lit::Num(number)) => LiteralValue::number(number.value)?,
        ast::Expr::Lit(ast::Lit::Bool(flag)) => LiteralValue::Boolean(flag.value),
        ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::Minus => match &*unary.arg {
            ast::Expr::Lit(ast::Lit::Num(number)) => LiteralValue::number(-number.value)?,
            _ => return None,
        },
        _ => return None,
    };
    let base = value.base();
    let single = intern(base, Set::from([value]), types).ok()?;
    Some((id.sym.to_string(), single))
}

/// Refine `ty` by `local === single` (`positive`) or `!==`. Returns `None` when
/// nothing changes, the comparison cannot hold, or the table budget is spent.
pub(super) fn refine(
    ty: Type,
    single: Type,
    positive: bool,
    types: &mut ir::ValueTypes,
) -> Option<Type> {
    let Type::Literal(base, id) = single else {
        // `true`/`false` singletons never intern to plain boolean.
        return None;
    };
    let value = types.literals.set(id).first()?.clone();
    let members = unions::members(ty, types);
    let mut refined = Vec::new();
    for member in &members {
        match *member {
            m if positive && m == base.erased() => refined.push(single),
            Type::Literal(b, set) if b == base && types.literals.set(set).contains(&value) => {
                if positive {
                    refined.push(single);
                } else {
                    let mut rest = types.literals.set(set).clone();
                    rest.remove(&value);
                    if !rest.is_empty() {
                        refined.push(intern(base, rest, types).ok()?);
                    }
                }
            }
            m if !positive => refined.push(m),
            _ => {}
        }
    }
    if refined.is_empty() || refined == members {
        return None;
    }
    unions::intern(refined, types).ok()
}

/// The constants of a literal set, in canonical order (v34 checked assertions).
pub(super) fn constants(id: u32, span: swc_common::Span, types: &ir::ValueTypes) -> Vec<Expr> {
    types
        .literals
        .set(id)
        .iter()
        .map(|value| match value {
            LiteralValue::String(bytes) => Expr {
                span,
                ty: Type::String,
                kind: E::String(bytes.clone()),
            },
            LiteralValue::Number(bits) => Expr {
                span,
                ty: Type::Float64,
                kind: E::Float64(f64::from_bits(*bits)),
            },
            LiteralValue::Boolean(value) => Expr {
                span,
                ty: Type::Boolean,
                kind: E::Integer(i32::from(*value)),
            },
        })
        .collect()
}

/// Whether a type graph contains a literal set (rejected as external input).
pub(super) fn contains(ty: Type, types: &ir::ValueTypes) -> bool {
    // Recursive types (v34) make the type graph cyclic: visit each type once.
    fn visit(
        ty: Type,
        types: &ir::ValueTypes,
        seen: &mut std::collections::BTreeSet<Type>,
    ) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        match ty {
            Type::Literal(..) => true,
            Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
                .iter()
                .any(|m| visit(*m, types, seen)),
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .any(|f| visit(f.ty, types, seen)),
            Type::Array(id) => visit(types.arrays[id as usize], types, seen),
            Type::Dictionary(id) => visit(types.dictionaries[id as usize].value, types, seen),
            _ => false,
        }
    }
    visit(ty, types, &mut std::collections::BTreeSet::new())
}

impl Lower<'_> {
    /// `flow_fact` with literal sets: a fact survives when it is strictly
    /// narrower than the declaration, including a literal subset of a base or
    /// literal member, and also for non-union declarations.
    pub(super) fn literal_flow_fact(
        &mut self,
        slot: u32,
        declared: Type,
        observed: Type,
    ) -> Result<()> {
        let upper = unions::members(declared, self.types);
        let members = unions::members(observed, self.types);
        self.refinements.remove(&slot);
        if members != upper && members.iter().all(|ty| member_of(*ty, &upper, self.types)) {
            let ty = unions::intern(members, self.types)?;
            self.refinements.insert(slot, ty);
        }
        Ok(())
    }
    /// `a || b` holds when `a` does, or when `a` fails and `b` holds (dually
    /// for a failed `a && b`); the two guarded states join.
    pub(super) fn either_guard(
        &mut self,
        left: &ast::Expr,
        right: &ast::Expr,
        yes: bool,
    ) -> Result<()> {
        let saved = self.flow_snapshot();
        self.guard(left, yes)?;
        let first = self.flow_snapshot();
        self.flow_alternate(&saved);
        self.guard(left, !yes)?;
        self.guard(right, yes)?;
        let second = self.flow_snapshot();
        // Refinement is optional precision: a join that cannot intern its
        // union keeps the unrefined state instead of rejecting the program.
        if self.flow_join(&saved, &[first, second]).is_err() {
            self.flow_alternate(&saved);
        }
        Ok(())
    }
}
