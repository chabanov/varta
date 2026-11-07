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
/// Members shown when a diagnostic describes a literal set.
const DESCRIBED_MEMBERS: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LiteralValue {
    /// Profile-encoded bytes, exactly as `Expression::String` carries them.
    String(Vec<u8>),
    /// Binary64 bits with -0 normalized to +0; NaN is never a literal type.
    Number(u64),
    Boolean(bool),
    /// A pattern template literal type of strings (v38 `type_level`), by
    /// its id in `LiteralTypes::patterns`; only in string sets.
    Pattern(u32),
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
            Self::String(_) | Self::Pattern(_) => LiteralBase::String,
            Self::Number(_) => LiteralBase::Number,
            Self::Boolean(_) => LiteralBase::Boolean,
        }
    }
    fn describe(&self, types: &ir::ValueTypes) -> String {
        let profile = types.profile;
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
            Self::Pattern(id) => type_level::describe_pattern(types.literals.pattern(*id)),
        }
    }
}

type Set = BTreeSet<LiteralValue>;

/// The singleton of a boolean truth test, used by v38 discriminant guards.
pub(super) fn boolean(value: bool, types: &mut ir::ValueTypes) -> Option<Type> {
    intern(
        LiteralBase::Boolean,
        Set::from([LiteralValue::Boolean(value)]),
        types,
    )
    .ok()
}

/// How a literal set relates to other literal types (v38
/// `declaration_forms`): an ordinary literal type, the numeric part of an
/// enum type (which, as in TypeScript, also accepts any `number`), or the
/// string part of an enum type (nominal: only values of that enum's type
/// are assignable, never a bare string literal).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LiteralKind {
    #[default]
    Plain,
    Open,
    Nominal(u32),
}

#[derive(Default)]
pub(crate) struct LiteralTypes {
    sets: Vec<(LiteralBase, Set)>,
    kinds: Vec<LiteralKind>,
    index: BTreeMap<(LiteralBase, Set, LiteralKind), u32>,
    /// Nominal enum groups by their generated enum name.
    groups: BTreeMap<String, u32>,
    members: usize,
    /// Union containing literal members -> the same union with erased members.
    erased_unions: BTreeMap<u32, Type>,
    /// Pattern template literal types (v38 `type_level`) by
    /// `LiteralValue::Pattern` id, and their ids.
    patterns: Vec<type_level::Pattern>,
    pattern_ids: BTreeMap<type_level::Pattern, u32>,
}
impl LiteralTypes {
    /// Literal constraints used only by compiler-generated JSON checkers;
    /// they are never erased into a public schema without their checks.
    pub(crate) fn values(&self, id: u32) -> &BTreeSet<LiteralValue> {
        &self.sets[id as usize].1
    }
    fn set(&self, id: u32) -> &Set {
        &self.sets[id as usize].1
    }
    /// The pieces of a pattern value.
    pub(crate) fn pattern(&self, id: u32) -> &type_level::Pattern {
        &self.patterns[id as usize]
    }
    /// The literal value of `pattern`, interned.
    pub(super) fn pattern_value(&mut self, pattern: type_level::Pattern) -> LiteralValue {
        if let Some(id) = self.pattern_ids.get(&pattern) {
            return LiteralValue::Pattern(*id);
        }
        let id = self.patterns.len() as u32;
        self.patterns.push(pattern.clone());
        self.pattern_ids.insert(pattern, id);
        LiteralValue::Pattern(id)
    }
    /// Whether a literal set has a pattern member.
    pub(super) fn has_pattern(&self, id: u32) -> bool {
        self.set(id)
            .iter()
            .any(|value| matches!(value, LiteralValue::Pattern(_)))
    }
    fn kind(&self, id: u32) -> LiteralKind {
        self.kinds.get(id as usize).copied().unwrap_or_default()
    }
}

/// Whether a value of literal kind `inner` may stand where kind `outer` is
/// expected: enum kinds accept ordinary literals of their values (numeric
/// enum members are such literals), a nominal enum only its own values.
fn kinds_accept(outer: LiteralKind, inner: LiteralKind) -> bool {
    match outer {
        LiteralKind::Plain | LiteralKind::Open => {
            matches!(inner, LiteralKind::Plain | LiteralKind::Open)
        }
        LiteralKind::Nominal(group) => inner == LiteralKind::Nominal(group),
    }
}

/// The type of an enum (v38 `declaration_forms`, `desugar::ENUM_TYPE`): the
/// literal members of `ty`, its numeric ones open and its string ones
/// nominal to `group`.
pub(super) fn enum_type(ty: Type, group: &str, types: &mut ir::ValueTypes) -> Result<Type> {
    let next = types.literals.groups.len() as u32;
    let group = *types
        .literals
        .groups
        .entry(group.to_owned())
        .or_insert(next);
    let kinded = |ty: Type, types: &mut ir::ValueTypes| -> Result<Type> {
        let Type::Literal(base, id) = ty else {
            return Err(invalid("enum member type is not a literal"));
        };
        let kind = if base == LiteralBase::Number {
            LiteralKind::Open
        } else {
            LiteralKind::Nominal(group)
        };
        let values = types.literals.set(id).clone();
        intern_kind(base, values, kind, types)
    };
    if !ty.is_union() {
        return kinded(ty, types);
    }
    let mut members = Vec::new();
    for member in unions::members(ty, types) {
        members.push(kinded(member, types)?);
    }
    unions::intern(members, types)
}

/// `'x' as E` of a string enum member: a free retag of an ordinary literal
/// to the enum's nominal type (or the nominal member of a mixed enum).
pub(super) fn retag(source: Type, target: Type, types: &ir::ValueTypes) -> Option<Type> {
    let Type::Literal(base, inner) = source else {
        return None;
    };
    if types.literals.kind(inner) != LiteralKind::Plain {
        return None;
    }
    let candidates = if target.is_union() {
        unions::members(target, types)
    } else {
        vec![target]
    };
    candidates.into_iter().find(|candidate| {
        matches!(*candidate, Type::Literal(b, outer) if b == base
            && matches!(types.literals.kind(outer), LiteralKind::Nominal(_))
            && types.literals.set(inner).is_subset(types.literals.set(outer)))
    })
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
            // Unknown admits every scalar. V38 strict matching must retain
            // all overlapping alternatives; older profiles keep their bytes.
            Type::Unknown if types.profile.web_bytes() => false,
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
/// base; at most the profile's `union_members` other members remain
/// structural (`LanguageProfile::limits`).
pub(super) fn union(union: &ast::TsUnionType, types: &mut ir::ValueTypes) -> Result<Type> {
    let limit = types.profile.limits().union_members;
    if union.types.len() > MAX_LITERAL_MEMBERS + limit {
        return Err(member_limit(
            union.types.len(),
            "",
            MAX_LITERAL_MEMBERS + limit,
        ));
    }
    let members = union
        .types
        .iter()
        .map(|ty| keyword_type(ty, types))
        .collect::<Result<Vec<_>>>()?;
    let structural = members
        .iter()
        .filter(|ty| !matches!(ty, Type::Literal(..)))
        .count();
    if structural > limit {
        return Err(member_limit(structural, "structural ", limit));
    }
    unions::intern(members, types)
}

/// A union annotation with more members than its profile admits. The text
/// keeps the earlier prefix, so v1 to v37 diagnostics only gain the counts.
pub(super) fn member_limit(count: usize, kind: &str, limit: usize) -> CompilerError {
    invalid(format!(
        "union member count exceeds limit: union has {count} {kind}members; limit {limit}"
    ))
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

/// The singleton literal type of `value` (v38 `type_level`).
pub(super) fn single(
    base: LiteralBase,
    value: LiteralValue,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    intern(base, Set::from([value]), types)
}
/// The literal set of `values` of one base (v38 `type_level`).
pub(super) fn of_values(
    base: LiteralBase,
    values: Set,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    intern(base, values, types)
}
/// The literal number value of `value`; `None` for NaN.
pub(super) fn number_value(value: f64) -> Option<LiteralValue> {
    LiteralValue::number(value)
}
/// The UTF-16 code units of a string literal value.
pub(super) fn string_units(
    value: &LiteralValue,
    profile: varta_common::language::LanguageProfile,
) -> Option<Vec<u16>> {
    match value {
        LiteralValue::String(bytes) if profile.utf16() => Some(
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect(),
        ),
        LiteralValue::String(bytes) => {
            Some(String::from_utf8_lossy(bytes).encode_utf16().collect())
        }
        _ => None,
    }
}
/// The string literal value of UTF-16 code units in `profile`'s encoding.
pub(super) fn string_value(
    units: &[u16],
    profile: varta_common::language::LanguageProfile,
) -> LiteralValue {
    if profile.utf16() {
        LiteralValue::String(units.iter().flat_map(|unit| unit.to_le_bytes()).collect())
    } else {
        LiteralValue::String(String::from_utf16_lossy(units).into_bytes())
    }
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
    intern_kind(base, values, LiteralKind::Plain, types)
}

fn intern_kind(
    base: LiteralBase,
    values: Set,
    kind: LiteralKind,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    if values.is_empty() {
        return Err(invalid("empty literal type"));
    }
    // A string matched by a pattern member of the same set is subsumed by it
    // (TypeScript's union reduction of template literal types).
    let values = type_level::reduce_patterns(values, types);
    if base == LiteralBase::Boolean && values.len() == 2 {
        return Ok(Type::Boolean);
    }
    if values.len() > MAX_LITERAL_MEMBERS {
        return Err(invalid("literal type member count exceeds limit"));
    }
    let table = &mut types.literals;
    let key = (base, values, kind);
    if let Some(&id) = table.index.get(&key) {
        return Ok(Type::Literal(base, id));
    }
    if table.members + key.1.len() > MAX_LITERAL_TABLE_MEMBERS {
        return Err(invalid("literal type table exceeds limit"));
    }
    let id =
        u32::try_from(table.sets.len()).map_err(|_| invalid("literal type table exceeds limit"))?;
    table.members += key.1.len();
    table.sets.push((key.0, key.1.clone()));
    table.kinds.push(kind);
    table.index.insert(key, id);
    Ok(Type::Literal(base, id))
}

/// Merge literal members of a sorted union member list: one set per base, a
/// base primitive absorbs its literals and `true | false` is `boolean`.
pub(super) fn normalize(values: Vec<Type>, types: &mut ir::ValueTypes) -> Result<Vec<Type>> {
    if !values.iter().any(|ty| matches!(ty, Type::Literal(..))) {
        return Ok(values);
    }
    // One set per base and kind: an enum's sets never merge with ordinary
    // literals.
    let mut groups: BTreeMap<(LiteralBase, LiteralKind), Vec<u32>> = BTreeMap::new();
    let mut rest = Vec::new();
    for ty in values {
        match ty {
            Type::Literal(base, id) => groups
                .entry((base, types.literals.kind(id)))
                .or_default()
                .push(id),
            other => rest.push(other),
        }
    }
    for ((base, kind), ids) in groups {
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
            intern_kind(base, values, kind, types)?
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
    let outer_set = types.literals.set(outer);
    inner == outer
        || types
            .literals
            .set(inner)
            .iter()
            .all(|value| type_level::value_in(value, outer_set, types))
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

/// The singleton literal type of a constant expression (v38 `type_level`:
/// the declaration type of `const x = "a"` is `"a"`, as in TypeScript).
pub(super) fn fresh(value: &Expr, types: &mut ir::ValueTypes) -> Option<Type> {
    let literal = constant(value)?;
    let base = literal.base();
    intern(base, Set::from([literal]), types).ok()
}

/// Whether `value` statically belongs to literal set `target` of `base`.
fn fits(value: &Expr, base: LiteralBase, target: u32, types: &ir::ValueTypes) -> bool {
    let kind = types.literals.kind(target);
    match value.ty {
        Type::Literal(own, set) => {
            own == base
                && subset(set, target, types)
                && kinds_accept(kind, types.literals.kind(set))
        }
        // A numeric enum type accepts any `number`, but a numeric literal
        // only if it is a member value (TypeScript 5).
        ty if ty == base.erased() && kind == LiteralKind::Open => {
            constant(value).is_none_or(|v| types.literals.set(target).contains(&v))
        }
        ty if ty == base.erased() && matches!(kind, LiteralKind::Nominal(_)) => false,
        ty if ty == base.erased() => constant(value)
            .is_some_and(|v| type_level::value_in(&v, types.literals.set(target), types)),
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
            || target.iter().any(|t| matches!(*t, Type::Literal(b, outer) if b == base
                && subset(set, outer, types)
                && kinds_accept(types.literals.kind(outer), types.literals.kind(set)))))
        || member == Type::Float64
            && target.iter().any(|t| {
                matches!(*t, Type::Literal(LiteralBase::Number, outer)
                if types.literals.kind(outer) == LiteralKind::Open)
            })
}

/// Literal-aware scalar assignability of an already-typed value.
pub(super) fn assignable(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    match (expected, actual) {
        (Type::Literal(b, outer), Type::Literal(a, inner)) => {
            b == a
                && subset(inner, outer, types)
                && kinds_accept(types.literals.kind(outer), types.literals.kind(inner))
        }
        (_, Type::Literal(base, _)) => expected == base.erased(),
        // A numeric enum type accepts any `number` (TypeScript).
        (Type::Literal(LiteralBase::Number, outer), Type::Float64) => {
            types.literals.kind(outer) == LiteralKind::Open
        }
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

pub(super) fn describe(id: u32, types: &ir::ValueTypes) -> String {
    let set = types.literals.set(id);
    let mut text: Vec<_> = set
        .iter()
        .take(DESCRIBED_MEMBERS)
        .map(|v| v.describe(types))
        .collect();
    if set.len() > DESCRIBED_MEMBERS {
        text.push(format!("… ({} members)", set.len()));
    }
    text.join(" | ")
}
fn describe_value(value: &Expr, types: &ir::ValueTypes) -> String {
    match (constant(value), value.ty) {
        (Some(v), _) => format!("value {}", v.describe(types)),
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
    // A string enum member is `'x' as E` (v38 `declaration_forms`).
    let literal = match literal {
        ast::Expr::TsAs(assertion) if types.profile.declaration_forms() => &*assertion.expr,
        literal => literal,
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
            Type::Literal(b, set)
                if b == base && type_level::value_in(&value, types.literals.set(set), types) =>
            {
                // An enum's narrowed set stays of the enum's kind.
                let kind = types.literals.kind(set);
                if positive {
                    let single = intern_kind(base, Set::from([value.clone()]), kind, types).ok()?;
                    refined.push(single);
                } else {
                    let mut rest = types.literals.set(set).clone();
                    rest.remove(&value);
                    if !rest.is_empty() {
                        refined.push(intern_kind(base, rest, kind, types).ok()?);
                    }
                }
            }
            m if !positive => refined.push(m),
            _ => {}
        }
    }
    // V38 (`type_level`): excluding the last literal leaves `never`, which
    // makes a `default` clause or `else` branch an exhaustiveness check.
    if refined.is_empty() && !positive && types.profile.type_level() {
        return Some(Type::Never);
    }
    if refined.is_empty() || refined == members {
        return None;
    }
    unions::intern(refined, types).ok()
}

/// Whether the singleton literals `singles` cover every value of `ty`, whose
/// members must all be literal sets (v38 exhaustive `switch`).
pub(super) fn exhausted_by(ty: Type, singles: &[Type], types: &ir::ValueTypes) -> bool {
    let covered: BTreeSet<&LiteralValue> = singles
        .iter()
        .filter_map(|single| match single {
            Type::Literal(_, id) => types.literals.set(*id).first(),
            _ => None,
        })
        .collect();
    let members = unions::members(ty, types);
    !members.is_empty()
        && members.iter().all(|member| match member {
            Type::Literal(_, id) => types
                .literals
                .set(*id)
                .iter()
                .all(|value| covered.contains(value)),
            _ => false,
        })
}

/// The constants of a literal set, in canonical order (v34 checked
/// assertions). A pattern member has no constant; callers refuse sets with
/// one (`LiteralTypes::has_pattern`).
pub(super) fn constants(id: u32, span: swc_common::Span, types: &ir::ValueTypes) -> Vec<Expr> {
    types
        .literals
        .set(id)
        .iter()
        .filter_map(|value| {
            Some(match value {
                LiteralValue::Pattern(_) => return None,
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
        })
        .collect()
}

/// Whether a type graph contains a literal set (rejected as external input).
pub(super) fn contains(ty: Type, types: &ir::ValueTypes) -> bool {
    contains_set(ty, types, &|_| true)
}

/// Whether a type graph contains a literal set with a pattern member (v38
/// `type_level`), which no run-time check tests.
pub(super) fn contains_pattern(ty: Type, types: &ir::ValueTypes) -> bool {
    contains_set(ty, types, &|id| types.literals.has_pattern(id))
}

/// Whether a type graph contains a literal set whose id satisfies `wanted`.
fn contains_set(ty: Type, types: &ir::ValueTypes, wanted: &dyn Fn(u32) -> bool) -> bool {
    // Recursive types (v34) make the type graph cyclic: visit each type once.
    fn visit(
        ty: Type,
        types: &ir::ValueTypes,
        wanted: &dyn Fn(u32) -> bool,
        seen: &mut std::collections::BTreeSet<Type>,
    ) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        match ty {
            Type::Literal(_, id) => wanted(id),
            Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
                .iter()
                .any(|m| visit(*m, types, wanted, seen)),
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .any(|f| visit(f.ty, types, wanted, seen)),
            Type::Array(id) => visit(types.arrays[id as usize], types, wanted, seen),
            Type::Dictionary(id) => {
                visit(types.dictionaries[id as usize].value, types, wanted, seen)
            }
            _ => false,
        }
    }
    visit(ty, types, wanted, &mut std::collections::BTreeSet::new())
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
