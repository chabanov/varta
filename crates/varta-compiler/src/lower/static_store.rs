//! The per-class static store (v38 `declaration_forms`, coordinator
//! decision of 2026-11-07 under the owner's delegated authority).
//!
//! A static field `x` declared by class `A` is `A`'s own property, a module
//! variable of `A`'s module (`desugar::statics`) in a root slot
//! ([`Stores::variables`]). A subclass `B` inherits it through its
//! prototype chain until code assigns `B.x`, which creates `B`'s own
//! property (ECMA-262 OrdinarySet on the class object): later reads
//! through `B` and its subclasses see it, while `A.x` keeps its value. Each
//! (subclass, inherited field) pair that code may reach gets root slots of
//! its own, allocated here on first use, in any module:
//!
//! - `flag`: the own property exists (`0`/`1`, below the heap base);
//! - `value`: its value, boxed in a union cell for a primitive type;
//! - `serial`: the order of its creation among all such properties, which
//!   `Object.keys` and `for…in` list after the fields the class declares.
//!
//! A read through class `K` tests `K`'s flag, then its parent's, up to the
//! class that declares the field; a write sets `K`'s own property. A class
//! value (`typeof A`) dispatches over its closed set of subclasses. The
//! slots are placeholders until every body is lowered ([`finish`]), then
//! numbered behind the stored module constants. Which class declares which
//! field, with its flags, comes from the class's `desugar::statics_key`
//! marker.
use super::*;
use std::collections::BTreeSet;
use swc_common::Span;

/// The first placeholder index (below `object_methods`' register and the
/// tagged template objects).
const BASE: u32 = 1 << 28;

/// The root slots of one subclass's own property.
#[derive(Clone, Copy)]
pub(crate) struct Store {
    flag: u32,
    value: u32,
    serial: u32,
    /// The slot type of `value` (a cell).
    storage: Type,
}

/// The module variable of a static field its class declares.
#[derive(Clone, Copy)]
pub(crate) struct Variable {
    index: u32,
    storage: Type,
    ty: Type,
}

/// The program's per-class static stores.
#[derive(Default)]
pub(crate) struct Stores {
    /// Subclass stores by (class, field name).
    slots: BTreeMap<(u32, String), Store>,
    /// Declared static fields by (declaring class, field name).
    variables: BTreeMap<(u32, String), Variable>,
    /// The creation counter (a boxed number) and its flag.
    counter: Option<(u32, u32)>,
    /// Placeholders allocated so far.
    count: u32,
}
impl Stores {
    fn allocate(&mut self) -> u32 {
        let index = BASE + self.count;
        self.count += 1;
        index
    }
}

/// After every body is lowered: number the placeholders behind the
/// `stored` module constants.
pub(super) fn finish(
    types: &mut ir::ValueTypes,
    functions: &mut [ir::Function],
    generated: &mut BTreeMap<usize, ir::Function>,
    stored: &mut u32,
) {
    let count = types.static_stores.count;
    if count == 0 {
        return;
    }
    let base = *stored;
    for function in functions.iter_mut().chain(generated.values_mut()) {
        prune::each_constant(&mut function.body, &mut |index| {
            if (BASE..BASE + count).contains(index) {
                *index = *index - BASE + base;
            }
        });
    }
    *stored += count;
}

/// The array index a property key denotes (a canonical numeric string
/// below 2^32 - 1): such keys come first, ascending
/// (OrdinaryOwnPropertyKeys).
fn array_index(key: &str) -> Option<u32> {
    let index: u32 = key.parse().ok()?;
    (index != u32::MAX && index.to_string() == key).then_some(index)
}

/// A static field reached through a class value.
pub(super) struct Field {
    /// The class of the class value's static type.
    class: u32,
    name: String,
    /// The field's type as that class sees it.
    pub(super) ty: Type,
}

fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

fn constant(span: Span, index: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::ModuleConstant(index),
    }
}

fn flag_set(span: Span, flag: u32) -> S {
    S::InitModuleConstant(
        flag,
        Expr {
            span,
            ty: Type::Boolean,
            kind: E::Integer(1),
        },
    )
}

/// The value of a cell slot of `storage` as `ty`.
fn unboxed(span: Span, index: u32, storage: Type, ty: Type) -> Expr {
    let stored = constant(span, index, storage);
    if storage == ty {
        stored
    } else {
        Expr {
            span,
            ty,
            kind: E::UnboxUnion(Box::new(stored)),
        }
    }
}

impl Lower<'_> {
    /// Record the module variable in root slot `index` when it is a static
    /// field of a class (`desugar::statics`), for reads and writes through
    /// subclasses and class values.
    pub(super) fn note_static_variable(
        &mut self,
        name: &str,
        index: u32,
        (storage, ty): (Type, Type),
    ) {
        let Some(class) = self.types.classes.static_elements.get(name).copied() else {
            return;
        };
        if crate::desugar::is_static_block(name) {
            return;
        }
        let field = crate::desugar::display_name(name).to_owned();
        self.types
            .static_stores
            .variables
            .insert((class, field), Variable { index, storage, ty });
    }
    /// The class declaring the static field `name` that `class` reaches
    /// through its prototype chain, with the declaration.
    pub(super) fn static_owner_of(
        &self,
        class: u32,
        name: &str,
    ) -> Option<(u32, classes::StaticDeclaration)> {
        let mut current = Some(self.types.classes.static_class(class));
        let mut depth = 0;
        while let Some(id) = current {
            if let Some(declaration) = classes::static_declarations(self.types, id)
                .into_iter()
                .find(|declaration| declaration.name == name)
            {
                return Some((id, declaration));
            }
            depth += 1;
            if depth > self.types.classes.count() {
                return None;
            }
            current = self.static_parent(id);
        }
        None
    }
    /// The module variable of the static field `name` that `owner` declares.
    fn static_variable(&self, owner: u32, name: &str) -> Result<Variable> {
        self.types
            .static_stores
            .variables
            .get(&(owner, name.to_owned()))
            .copied()
            .ok_or_else(|| {
                unsupported(&format!(
                    "static field {name} of class {} reached before its module is linked",
                    self.types.classes.get(owner).name
                ))
            })
    }
    /// The static field `member` names on the class value `base`, if any.
    pub(super) fn static_field(
        &mut self,
        member: &ast::MemberExpr,
        base: &Expr,
    ) -> Result<Option<Field>> {
        if !self.types.profile.declaration_forms() {
            return Ok(None);
        }
        let Type::ClassValue(class) = base.ty else {
            return Ok(None);
        };
        let Ok(name) = self.member_key(&member.prop) else {
            return Ok(None);
        };
        self.class_functions()?;
        let Some((owner, declaration)) = self.static_owner_of(class, &name) else {
            return Err(self.source.locate(
                unsupported(&format!(
                    "member {name} of a class value (only name, static fields, new and static method calls are dispatched)"
                )),
                member.prop.span(),
            ));
        };
        self.check_access(owner, declaration.access, class, &name, member.prop.span())?;
        let ty = self
            .static_variable(owner, &name)
            .map_err(|e| self.source.locate(e, member.prop.span()))?
            .ty;
        Ok(Some(Field { class, name, ty }))
    }
    /// The classes a class value `base` of `class` may hold: the one it
    /// names, or the whole family.
    fn static_targets(&self, base: &Expr, class: u32) -> Vec<u32> {
        match base.kind {
            E::Integer(id) if id >= 0 => vec![id as u32],
            _ => class_values::family(self.types, class),
        }
    }
    /// The store of the own property `name` of `class`, which inherits it.
    fn static_store(&mut self, class: u32, name: &str, ty: Type) -> Result<Store> {
        let key = (class, name.to_owned());
        if let Some(store) = self.types.static_stores.slots.get(&key) {
            return Ok(*store);
        }
        let storage = if ty.needs_root() {
            ty
        } else {
            unions::intern(vec![ty, Type::Undefined], self.types)?
        };
        if !storage.needs_root() {
            return Err(unsupported(&format!("static field {name} of type {ty}")));
        }
        let stores = &mut self.types.static_stores;
        let store = Store {
            flag: stores.allocate(),
            value: stores.allocate(),
            serial: stores.allocate(),
            storage,
        };
        stores.slots.insert(key, store);
        Ok(store)
    }
    /// The class object a class's prototype chain continues with: its
    /// parent, the generic class itself for an instance parent.
    fn static_parent(&self, class: u32) -> Option<u32> {
        let parent = self.types.classes.get(class).parent?;
        Some(self.types.classes.static_class(parent))
    }
    /// The serial cell type (`number | undefined`).
    fn serial_storage(&mut self) -> Result<Type> {
        let number = self.types.number();
        unions::intern(vec![number, Type::Undefined], self.types)
    }
    /// The class declaring `name` for `class`, the declaration and the
    /// declaring class's variable.
    fn static_reached(
        &self,
        class: u32,
        name: &str,
    ) -> Result<(u32, classes::StaticDeclaration, Variable)> {
        let (owner, declaration) = self
            .static_owner_of(class, name)
            .ok_or_else(|| CompilerError::Internal(format!("missing static field {name}")))?;
        let variable = self.static_variable(owner, name)?;
        Ok((owner, declaration, variable))
    }
    /// The value of `name` read through `class`: its own property if it
    /// has one, else its parent's, ending at the declaring class. An
    /// auto-accessor is a get/set pair over the declaring class's storage,
    /// which tsc names through that class (`A.#x_accessor_storage`), so
    /// every subclass reads and writes the declaring class's.
    fn static_chain_read(&mut self, class: u32, name: &str, span: Span) -> Result<Expr> {
        let (owner, declaration, variable) = self.static_reached(class, name)?;
        let ty = variable.ty;
        if owner == self.types.classes.static_class(class) || declaration.accessor {
            return self.module_slot_read(variable.index, (variable.storage, ty), span);
        }
        let store = self.static_store(class, name, ty)?;
        let parent = self
            .static_parent(class)
            .ok_or_else(|| CompilerError::Internal("static field without a parent".into()))?;
        let inherited = self.static_chain_read(parent, name, span)?;
        let inherited = unions::widen(inherited, Some(ty), self.types)?;
        Ok(Expr {
            span,
            ty,
            kind: E::Conditional(
                Box::new(constant(span, store.flag, Type::Boolean)),
                Box::new(unboxed(span, store.value, store.storage, ty)),
                Box::new(inherited),
            ),
        })
    }
    /// The statements that write `value` (a local of type `ty`) as the
    /// property `name` of `class`: the declaring class's variable, or
    /// `class`'s own property, created on first write.
    fn static_chain_write(
        &mut self,
        class: u32,
        name: &str,
        (value, ty): (u32, Type),
        span: Span,
    ) -> Result<Vec<S>> {
        let (owner, declaration, variable) = self.static_reached(class, name)?;
        if declaration.readonly {
            return Err(self.source.locate(
                type_error(format!(
                    "cannot assign to {name} because it is a read-only static field (TS2540)"
                )),
                span,
            ));
        }
        let field = variable.ty;
        if !arguments::accepts(field, ty, self.types) {
            let (expected, found) = self.mismatch(field, ty);
            return Err(self.source.locate(
                type_error(format!(
                    "static field {name} of class {} expects {expected}, found {found}",
                    self.types.classes.get(class).name
                )),
                span,
            ));
        }
        let value = unions::widen(local(span, value, ty), Some(field), self.types)?;
        if owner == self.types.classes.static_class(class) || declaration.accessor {
            return Ok(vec![self.module_slot_store(
                variable.index,
                variable.storage,
                value,
            )?]);
        }
        let store = self.static_store(class, name, field)?;
        let created = self.static_creation(store, span)?;
        let value = unions::widen(value, Some(store.storage), self.types)?;
        Ok(vec![
            S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Not(Box::new(constant(span, store.flag, Type::Boolean))),
                },
                created,
                vec![],
            ),
            S::InitModuleConstant(store.value, value),
        ])
    }
    /// The creation of an own property: the next serial, then the flag.
    fn static_creation(&mut self, store: Store, span: Span) -> Result<Vec<S>> {
        let cell = self.serial_storage()?;
        let number = self.types.number();
        let (counter, counted) = match self.types.static_stores.counter {
            Some(slots) => slots,
            None => {
                let stores = &mut self.types.static_stores;
                let slots = (stores.allocate(), stores.allocate());
                stores.counter = Some(slots);
                slots
            }
        };
        let float = |value: f64| Expr {
            span,
            ty: number,
            kind: E::Float64(value),
        };
        let previous = Expr {
            span,
            ty: number,
            kind: E::Conditional(
                Box::new(constant(span, counted, Type::Boolean)),
                Box::new(unboxed(span, counter, cell, number)),
                Box::new(float(0.0)),
            ),
        };
        let next = Expr {
            span,
            ty: number,
            kind: E::Binary(ast::BinaryOp::Add, Box::new(previous), Box::new(float(1.0))),
        };
        Ok(vec![
            S::InitModuleConstant(counter, unions::widen(next, Some(cell), self.types)?),
            flag_set(span, counted),
            S::InitModuleConstant(
                store.serial,
                unions::widen(unboxed(span, counter, cell, number), Some(cell), self.types)?,
            ),
            flag_set(span, store.flag),
        ])
    }
    /// `field` read through the class value in local `identity`.
    fn static_read_at(
        &mut self,
        field: &Field,
        (identity, targets): (u32, &[u32]),
        span: Span,
    ) -> Result<Expr> {
        let ty = field.ty;
        let output = self.hidden(ty);
        let mut arms = Vec::new();
        for target in targets {
            let value = self.static_chain_read(*target, &field.name, span)?;
            let value = unions::widen(value, Some(ty), self.types)?;
            arms.push((*target, vec![S::Init(output, value)]));
        }
        let body = self.static_arms(identity, arms, span);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![output],
                body,
            },
        })
    }
    /// One arm per class, or the only arm.
    fn static_arms(&self, identity: u32, mut arms: Vec<(u32, Vec<S>)>, span: Span) -> Vec<S> {
        if arms.len() == 1 {
            return arms.pop().map(|(_, body)| body).unwrap_or_default();
        }
        class_values::dispatch(span, identity, arms)
    }
    /// The writes of the local `value` through the class value in local
    /// `identity`.
    fn static_write_at(
        &mut self,
        field: &Field,
        (identity, targets): (u32, &[u32]),
        value: (u32, Type),
        span: Span,
    ) -> Result<Vec<S>> {
        let mut arms = Vec::new();
        for target in targets {
            arms.push((
                *target,
                self.static_chain_write(*target, &field.name, value, span)?,
            ));
        }
        // A setter runs code (v37 `paths`).
        self.drop_paths();
        Ok(self.static_arms(identity, arms, span))
    }
    /// `v.x` of a static field `x`.
    pub(super) fn static_field_read(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
    ) -> Result<Expr> {
        let Some(field) = self.static_field(member, &base)? else {
            return Err(CompilerError::Internal("missing static field".into()));
        };
        let span = member.span;
        let targets = self.static_targets(&base, field.class);
        let identity = self.hidden(Type::Number);
        let read = self.static_read_at(&field, (identity, &targets), span)?;
        Ok(self.static_block(span, (identity, base), (vec![], vec![]), read))
    }
    /// `{ identity = base; …prefix; output = value }`, rooting `roots`.
    fn static_block(
        &mut self,
        span: Span,
        (identity, base): (u32, Expr),
        (prefix, roots): (Vec<S>, Vec<u32>),
        value: Expr,
    ) -> Expr {
        let ty = value.ty;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let mut body = vec![S::Init(
            identity,
            Expr {
                ty: Type::Number,
                ..base
            },
        )];
        body.extend(prefix);
        body.push(S::Init(output, value));
        let mut roots = roots;
        roots.push(output);
        Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        }
    }
    /// The statements that evaluate `value` into a new local and write it;
    /// the local holds the written value.
    fn static_store_value(
        &mut self,
        field: &Field,
        (identity, targets): (u32, &[u32]),
        value: Expr,
        span: Span,
    ) -> Result<(Vec<S>, u32)> {
        let ty = value.ty;
        let output = self.hidden(ty);
        let mut body = vec![S::Init(output, value)];
        body.extend(self.static_write_at(field, (identity, targets), (output, ty), span)?);
        Ok((body, output))
    }
    /// `v.x = value`.
    pub(super) fn static_assignment(
        &mut self,
        field: Field,
        base: Expr,
        right: &ast::Expr,
        span: Span,
    ) -> Result<Expr> {
        let targets = self.static_targets(&base, field.class);
        let identity = self.hidden(Type::Number);
        let value = self.expr_expected(right, Some(field.ty))?;
        let value = self.static_value(&field, value)?;
        self.static_written(&field, (identity, base, &targets), value, span)
    }
    /// `value` as the field's type, or the TypeScript error.
    fn static_value(&mut self, field: &Field, value: Expr) -> Result<Expr> {
        if !arguments::accepts(field.ty, value.ty, self.types) {
            let (expected, found) = self.mismatch(field.ty, value.ty);
            return Err(self.source.locate(
                type_error(format!(
                    "static field {} expects {expected}, found {found} (TS2322)",
                    field.name
                )),
                value.span,
            ));
        }
        unions::widen(value, Some(field.ty), self.types)
    }
    /// The block that evaluates the base and the value, writes it and
    /// yields it.
    fn static_written(
        &mut self,
        field: &Field,
        (identity, base, targets): (u32, Expr, &[u32]),
        value: Expr,
        span: Span,
    ) -> Result<Expr> {
        let (prefix, output) = self.static_store_value(field, (identity, targets), value, span)?;
        let result = local(span, output, field.ty);
        Ok(self.static_block(span, (identity, base), (prefix, vec![output]), result))
    }
    /// `v.x op= value`.
    pub(super) fn static_compound(
        &mut self,
        field: Field,
        base: Expr,
        right: &ast::Expr,
        op: ast::BinaryOp,
        span: Span,
    ) -> Result<Expr> {
        let targets = self.static_targets(&base, field.class);
        let identity = self.hidden(Type::Number);
        let old = self.static_read_at(&field, (identity, &targets), span)?;
        let value = self.compound_operation(old, right, op, span)?;
        let value = self.static_value(&field, value)?;
        self.static_written(&field, (identity, base, &targets), value, span)
    }
    /// `v.x++` and friends: a postfix update yields the old value.
    pub(super) fn static_update(
        &mut self,
        field: Field,
        base: Expr,
        update: &ast::UpdateExpr,
    ) -> Result<Expr> {
        let span = update.span;
        let targets = self.static_targets(&base, field.class);
        let identity = self.hidden(Type::Number);
        let old = self.static_read_at(&field, (identity, &targets), span)?;
        let old = self.checked_update_numeric(old)?;
        let old_ty = old.ty;
        let previous = self.hidden(old_ty);
        let next = self.update_next(span, previous, old_ty, update.op)?;
        let next = self.static_value(&field, next)?;
        let (stored, output) = self.static_store_value(&field, (identity, &targets), next, span)?;
        let mut prefix = vec![S::Init(previous, old)];
        prefix.extend(stored);
        let result = if update.prefix {
            local(span, output, field.ty)
        } else {
            local(span, previous, old_ty)
        };
        Ok(self.static_block(
            span,
            (identity, base),
            (prefix, vec![previous, output]),
            result,
        ))
    }
    /// `v.x &&= value`, `||=` and `??=`: the write runs only when the
    /// operator selects the right side.
    pub(super) fn static_logical(
        &mut self,
        field: Field,
        base: Expr,
        right: &ast::Expr,
        op: ast::AssignOp,
        span: Span,
    ) -> Result<Expr> {
        let targets = self.static_targets(&base, field.class);
        let identity = self.hidden(Type::Number);
        let old = self.static_read_at(&field, (identity, &targets), span)?;
        let saved = self.flow_snapshot();
        let value = self.expr_expected(right, Some(field.ty));
        self.flow_restore(&saved);
        let value = self.static_value(&field, value?)?;
        let (body, output) = self.static_store_value(&field, (identity, &targets), value, span)?;
        let marker = self.hidden(Type::Number);
        let store = Expr {
            span,
            ty: field.ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![output],
                body,
            },
        };
        let value = self.logical_assignment_operation(old, store, op, span)?;
        Ok(self.static_block(span, (identity, base), (vec![], vec![]), value))
    }
    /// `Object.keys(v)` (`chain` false) or the keys `for…in` visits of the
    /// class value `value`: the fields each class declares, in definition
    /// order, then the properties code created on it, in creation order;
    /// `for…in` continues with each ancestor's keys that no closer class
    /// has (static methods and auto-accessors are not enumerable).
    pub(super) fn class_value_keys(
        &mut self,
        value: Expr,
        chain: bool,
        span: Span,
    ) -> Result<Expr> {
        let Type::ClassValue(class) = value.ty else {
            return Err(CompilerError::Internal("keys of a non-class value".into()));
        };
        self.class_functions()?;
        let strings = arrays::intern(Type::String, self.types)?;
        let targets = self.static_targets(&value, class);
        let identity = self.hidden(Type::Number);
        let output = self.hidden(strings);
        let mut arms = Vec::new();
        for target in targets {
            arms.push((
                target,
                self.class_keys_into(target, (output, strings), chain, span)?,
            ));
        }
        let mut body = vec![
            S::Init(
                identity,
                Expr {
                    ty: Type::Number,
                    ..value
                },
            ),
            S::Init(
                output,
                Expr {
                    span,
                    ty: strings,
                    kind: E::Array(vec![]),
                },
            ),
        ];
        body.extend(self.static_arms(identity, arms, span));
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: strings,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![output],
                body,
            },
        })
    }
    /// The pushes of `class`'s keys (and with `chain` its ancestors') onto
    /// the array in local `output`.
    fn class_keys_into(
        &mut self,
        class: u32,
        output: (u32, Type),
        chain: bool,
        span: Span,
    ) -> Result<Vec<S>> {
        let mut classes = vec![class];
        while chain && let Some(parent) = classes.last().and_then(|c| self.static_parent(*c)) {
            classes.push(parent);
        }
        let mut declared: BTreeSet<String> = BTreeSet::new();
        let mut created: Vec<(String, u32)> = Vec::new();
        let mut body = Vec::new();
        for current in classes {
            let own = self.own_static_keys(current);
            for name in &own {
                if declared.contains(name) {
                    continue;
                }
                let push = self.key_push(output, name, span);
                body.push(absent_in(&created, name, vec![push], span));
            }
            let inherited: Vec<(String, Store)> = self
                .static_inherited(current)?
                .into_iter()
                .filter(|(name, _)| !declared.contains(name))
                .collect();
            body.extend(self.created_pushes(&inherited, &created, output, span));
            declared.extend(own);
            created.extend(
                inherited
                    .into_iter()
                    .map(|(name, store)| (name, store.flag)),
            );
        }
        Ok(body)
    }
    /// The own enumerable static fields `class` declares: integer keys
    /// ascending, then the others in definition order (auto-accessors are
    /// not enumerable).
    fn own_static_keys(&self, class: u32) -> Vec<String> {
        let mut keys: Vec<String> = classes::static_declarations(self.types, class)
            .into_iter()
            .filter(|declaration| !declaration.accessor)
            .map(|declaration| declaration.name)
            .collect();
        keys.sort_by_key(|key| array_index(key).map_or((1, 0), |index| (0, index)));
        keys
    }
    /// The stores of the static fields `class` inherits (the properties it
    /// may own), by name.
    fn static_inherited(&mut self, class: u32) -> Result<Vec<(String, Store)>> {
        let mut names = BTreeSet::new();
        let mut current = self.static_parent(class);
        let mut depth = 0;
        while let Some(id) = current {
            for declaration in classes::static_declarations(self.types, id) {
                if !declaration.accessor {
                    names.insert(declaration.name);
                }
            }
            depth += 1;
            if depth > self.types.classes.count() {
                break;
            }
            current = self.static_parent(id);
        }
        let mut stores = Vec::new();
        for name in names {
            let Some((owner, declaration)) = self.static_owner_of(class, &name) else {
                continue;
            };
            if owner == self.types.classes.static_class(class) || declaration.accessor {
                continue;
            }
            let ty = self.static_variable(owner, &name)?.ty;
            let store = self.static_store(class, &name, ty)?;
            stores.push((name, store));
        }
        Ok(stores)
    }
    /// `output.push(name)`.
    fn key_push(&self, (output, strings): (u32, Type), name: &str, span: Span) -> S {
        S::Expr(Expr {
            span,
            ty: self.types.number(),
            kind: E::ArrayPush(
                Box::new(local(span, output, strings)),
                Box::new(Expr {
                    span,
                    ty: strings,
                    kind: E::Array(vec![Some(Expr {
                        span,
                        ty: Type::String,
                        kind: E::String(self.types.profile.string_bytes(name)),
                    })]),
                }),
            ),
        })
    }
    /// The pushes of the created properties among `fields`, in creation
    /// order (the least serial above the last pushed one, once per field),
    /// skipping those a closer class created too.
    fn created_pushes(
        &mut self,
        fields: &[(String, Store)],
        closer: &[(String, u32)],
        output: (u32, Type),
        span: Span,
    ) -> Vec<S> {
        if fields.is_empty() {
            return Vec::new();
        }
        let number = self.types.number();
        let Ok(cell) = self.serial_storage() else {
            return Vec::new();
        };
        let float = |value: f64| Expr {
            span,
            ty: number,
            kind: E::Float64(value),
        };
        let compare = |op: ast::BinaryOp, left: Expr, right: Expr| Expr {
            span,
            ty: Type::Boolean,
            kind: E::Binary(op, Box::new(left), Box::new(right)),
        };
        let (last, best, least) = (
            self.hidden(number),
            self.hidden(number),
            self.hidden(number),
        );
        let mut body = vec![
            S::Init(last, float(0.0)),
            S::Init(best, float(-1.0)),
            S::Init(least, float(f64::INFINITY)),
        ];
        for _ in fields {
            body.push(S::Set(best, float(-1.0)));
            body.push(S::Set(least, float(f64::INFINITY)));
            for (index, (name, store)) in fields.iter().enumerate() {
                let serial = || unboxed(span, store.serial, cell, number);
                let select = vec![S::If(
                    compare(ast::BinaryOp::Gt, serial(), local(span, last, number)),
                    vec![S::If(
                        compare(ast::BinaryOp::Lt, serial(), local(span, least, number)),
                        vec![S::Set(best, float(index as f64)), S::Set(least, serial())],
                        vec![],
                    )],
                    vec![],
                )];
                body.push(S::If(
                    constant(span, store.flag, Type::Boolean),
                    vec![absent_in(closer, name, select, span)],
                    vec![],
                ));
            }
            for (index, (name, _)) in fields.iter().enumerate() {
                let push = self.key_push(output, name, span);
                body.push(S::If(
                    compare(
                        ast::BinaryOp::EqEqEq,
                        local(span, best, number),
                        float(index as f64),
                    ),
                    vec![push],
                    vec![],
                ));
            }
            body.push(S::Set(last, local(span, least, number)));
        }
        body
    }
}

/// `body` when no closer class created its own `name` (none of the flags
/// of `closer` for that name is set).
fn absent_in(closer: &[(String, u32)], name: &str, body: Vec<S>, span: Span) -> S {
    let mut statement = S::Block(body);
    for (_, flag) in closer.iter().filter(|(closer, _)| closer == name) {
        statement = S::If(
            constant(span, *flag, Type::Boolean),
            vec![],
            vec![statement],
        );
    }
    statement
}
