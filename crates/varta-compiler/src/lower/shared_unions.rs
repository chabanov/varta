//! Unions of several record shapes and class types (v37 `language_core`).
//!
//! Such members share the record tag of a union cell; the emitter tells them
//! apart by layout descriptor (`emit::shared_members`). Lowering adds:
//!
//! - field reads on the union: a field every member declares is read by
//!   dispatching on the member, with the union of the field types;
//! - discriminant narrowing: `x.kind === "lit"` (and `!==`, `switch`) keeps
//!   the members whose `kind` field can hold the literal, or drops the members
//!   whose `kind` is exactly that literal;
//! - `"key" in x`: keeps the members that declare `key` (or drops them);
//! - object literals in a union context take the record member whose fields
//!   and literal-typed fields match the literal.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

/// Whether `member` can hold the singleton literal `single`.
fn admits(member: Type, single: Type, types: &mut ir::ValueTypes) -> bool {
    member == single || literals::refine(member, single, true, types).is_some()
}

impl Lower<'_> {
    /// Whether `ty` is a union whose members are all records or class
    /// instances, at least two of them.
    pub(super) fn shared_records(&self, ty: Type) -> bool {
        if !self.types.profile.language_core() || !ty.is_union() {
            return false;
        }
        let members = unions::members(ty, self.types);
        members.len() > 1
            && members
                .iter()
                .all(|ty| matches!(ty, Type::Record(_) | Type::Class(_) | Type::Array(_)))
    }
    /// The type a member declares for `name`, as a read sees it.
    fn member_field(&self, member: Type, name: &str) -> Option<Type> {
        match member {
            Type::Record(id) => self.types.records[id as usize]
                .fields
                .iter()
                .find(|field| field.name == name)
                .map(|field| field.ty),
            Type::Class(class) => self.types.classes.field(class, name).map(|(_, f)| f.ty),
            _ => None,
        }
    }
    /// `x.name` on a union of record shapes: one read per member, selected by
    /// the member's descriptor (the last member needs no test).
    pub(super) fn shared_field(&mut self, member: &ast::MemberExpr, value: Expr) -> Result<Expr> {
        let span = member.span;
        let union = value.ty;
        let name = match &member.prop {
            ast::MemberProp::Ident(id) => id.sym.to_string(),
            ast::MemberProp::Computed(key) => match &*key.expr {
                ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str().unwrap_or("").to_owned(),
                // An index of a union of arrays: each branch lowers the
                // (effect-free) key once more; one branch runs.
                key if !paths::impure(key)
                    && unions::members(union, self.types)
                        .iter()
                        .all(|ty| matches!(ty, Type::Array(_))) =>
                {
                    String::new()
                }
                _ => {
                    return Err(unsupported(
                        "computed property of a union of object types; narrow it first",
                    ));
                }
            },
            ast::MemberProp::PrivateName(_) => {
                return Err(unsupported("private property of a union of object types"));
            }
        };
        let members = unions::members(union, self.types);
        let class_getter = |this: &Self, ty: Type| match ty {
            Type::Class(class) => this
                .types
                .classes
                .get(class)
                .methods
                .get(&name)
                .is_some_and(|method| method.getter),
            _ => false,
        };
        if let Some(missing) = members.iter().find(|ty| {
            !matches!(ty, Type::Array(_))
                && self.member_field(**ty, &name).is_none()
                && !class_getter(self, **ty)
        }) {
            return Err(self.source.locate(
                type_error(format!(
                    "property {name} is not declared by every member of the union (not by {}); \
                     narrow it first",
                    void_functions::describe(*missing, self.types)
                )),
                member.prop.span(),
            ));
        }
        self.dispatch(span, value, |this, ty, unboxed| match ty {
            Type::Record(_) => this.record_field(member, unboxed, true),
            Type::Array(_) => this.array_member(member, unboxed, true),
            _ => this.class_member(member, unboxed, true),
        })
    }
    /// `"name" in x` on a union of record shapes: a record's own (present)
    /// field or an inherited `Object.prototype` name, an instance's field,
    /// method or getter.
    pub(super) fn shared_in(
        &mut self,
        name: &str,
        value: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let constant = |unboxed: Expr, value: bool| Expr {
            span,
            ty: Type::Boolean,
            kind: E::Sequence(
                Box::new(unboxed),
                Box::new(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(i32::from(value)),
                }),
            ),
        };
        self.dispatch(span, value, |this, ty, unboxed| {
            Ok(match ty {
                Type::Record(_) if records::inherited(name) => constant(unboxed, true),
                Type::Record(id) => {
                    let slot = this.types.records[id as usize]
                        .fields
                        .iter()
                        .position(|field| field.name == name)
                        .map(|slot| slot as u32);
                    // A view (v38) may hold an object that has the name.
                    if slot.is_none() && this.types.profile.structural_subtyping() {
                        return this.undeclared_in(id, name, unboxed, span);
                    }
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::HasOwn(Box::new(unboxed), slot),
                    }
                }
                Type::Class(class) => {
                    let declared = this.types.classes.field(class, name).is_some()
                        || this.types.classes.get(class).methods.contains_key(name)
                        || records::inherited(name);
                    constant(unboxed, declared)
                }
                _ => constant(unboxed, false),
            })
        })
    }
    /// Whether `ty` is a union of class instance types only (two or more).
    pub(super) fn shared_classes(&self, ty: Type) -> bool {
        self.shared_records(ty)
            && unions::members(ty, self.types)
                .iter()
                .all(|ty| matches!(ty, Type::Class(_)))
    }
    /// `x.m(...)` on a union of classes: each member's method, selected by
    /// descriptor; the arguments are evaluated once, in the selected branch.
    pub(super) fn shared_method_call(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let value = self.expr(&member.obj)?;
        if !self.shared_classes(value.ty) {
            return self.class_method_call(call, member, Some(value), expected);
        }
        let span = call.span;
        let result = self.dispatch(span, value, |this, _, unboxed| {
            this.class_method_call(call, member, Some(unboxed), expected)
        })?;
        // A method may run any code (v37 `paths`).
        self.drop_paths();
        Ok(result)
    }
    /// One expression per member of the union `value`, selected by the
    /// member's descriptor (the last member needs no test), joined into the
    /// union of their types.
    fn dispatch(
        &mut self,
        span: swc_common::Span,
        value: Expr,
        mut each: impl FnMut(&mut Self, Type, Expr) -> Result<Expr>,
    ) -> Result<Expr> {
        let union = value.ty;
        let members = unions::members(union, self.types);
        let saved = self.hidden(union);
        let mut reads = Vec::new();
        for ty in &members {
            let unboxed = Expr {
                span,
                ty: *ty,
                kind: E::UnboxUnion(Box::new(local(span, saved, union))),
            };
            reads.push((*ty, each(self, *ty, unboxed)?));
        }
        let result = unions::intern(reads.iter().map(|(_, read)| read.ty).collect(), self.types)?;
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        let mut chain: Option<Vec<S>> = None;
        for (ty, read) in reads.into_iter().rev() {
            let store = vec![S::Init(
                output,
                unions::widen(read, Some(result), self.types)?,
            )];
            chain = Some(match chain {
                None => store,
                Some(otherwise) => vec![S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsUnionMember(Box::new(local(span, saved, union)), ty),
                    },
                    store,
                    otherwise,
                )],
            });
        }
        let mut body = vec![S::Init(saved, value)];
        body.extend(chain.unwrap_or_default());
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body,
            },
        })
    }
    /// The members of `ty` that remain when `x.name === single` holds
    /// (`positive`) or fails; `None` when nothing changes.
    pub(super) fn discriminated(
        &mut self,
        ty: Type,
        name: &str,
        single: Type,
        positive: bool,
    ) -> Option<Vec<Type>> {
        // A single record may still be excluded (leaving `never`).
        let single_record =
            self.types.profile.language_core() && matches!(ty, Type::Record(_) | Type::Class(_));
        if !self.shared_records(ty) && !single_record {
            return None;
        }
        let members = unions::members(ty, self.types);
        let mut kept = Vec::new();
        for member in &members {
            let keep = match self.member_field(*member, name) {
                // A member without the field is kept (TypeScript rejects the
                // read before narrowing).
                None => true,
                Some(field) if positive => admits(field, single, self.types),
                Some(field) => field != single,
            };
            if keep {
                kept.push(*member);
            }
        }
        (kept.len() < members.len()).then_some(kept)
    }
    /// The members of `ty` that declare (`positive`) or do not declare `name`.
    pub(super) fn with_property(&self, ty: Type, name: &str, positive: bool) -> Option<Vec<Type>> {
        if !self.shared_records(ty) {
            return None;
        }
        let members = unions::members(ty, self.types);
        let kept: Vec<_> = members
            .iter()
            .copied()
            .filter(|member| {
                let declared = self.member_field(*member, name).is_some();
                // An optional field may be absent, so `!("k" in x)` keeps it.
                let optional = matches!(member, Type::Record(id)
                    if self.types.records[*id as usize].fields.iter().any(|f| f.name == name && f.optional));
                if positive { declared } else { !declared || optional }
            })
            .collect();
        (kept.len() < members.len()).then_some(kept)
    }
    /// Narrow the object of `member` (`obj.name` compared with a literal) by
    /// its discriminant: a binding through its refinement, a path through a
    /// path fact.
    pub(super) fn discriminant_guard(
        &mut self,
        member: &ast::MemberExpr,
        single: Type,
        positive: bool,
    ) -> Result<()> {
        let ast::MemberProp::Ident(name) = &member.prop else {
            return Ok(());
        };
        let name = name.sym.to_string();
        let object = stmt::unparen(&member.obj);
        if let ast::Expr::Ident(id) = object {
            return self.refine_discriminant(id.sym.as_ref(), &name, single, positive);
        }
        self.discriminate_path(object, &name, single, positive)
    }
    fn refine_discriminant(
        &mut self,
        binding: &str,
        name: &str,
        single: Type,
        positive: bool,
    ) -> Result<()> {
        let Ok(found) = self
            .binding(binding)
            .or_else(|_| self.resolve_binding(binding))
        else {
            return Ok(());
        };
        let Some(ty) = self.refinements.get(&found.slot).copied().or(found.ty) else {
            return Ok(());
        };
        let Some(kept) = self.discriminated(ty, name, single, positive) else {
            return Ok(());
        };
        self.refine_with(binding, |_, _| Some(kept))
    }
    /// `"key" in x` for a binding `x` of a union of record shapes.
    pub(super) fn property_guard(
        &mut self,
        key: &ast::Expr,
        value: &ast::Expr,
        yes: bool,
    ) -> Result<bool> {
        if !self.types.profile.language_core() {
            return Ok(false);
        }
        let (ast::Expr::Lit(ast::Lit::Str(text)), ast::Expr::Ident(id)) =
            (stmt::unparen(key), stmt::unparen(value))
        else {
            return Ok(false);
        };
        let Some(name) = text.value.as_str().map(str::to_owned) else {
            return Ok(false);
        };
        let Ok(found) = self.binding(id.sym.as_ref()) else {
            return Ok(false);
        };
        let Some(ty) = self.refinements.get(&found.slot).copied().or(found.ty) else {
            return Ok(false);
        };
        let Some(kept) = self.with_property(ty, &name, yes) else {
            return Ok(self.shared_records(ty));
        };
        self.refine_with(id.sym.as_ref(), |_, _| Some(kept))?;
        Ok(true)
    }
    /// The record member of a union for the object literal `object`: the
    /// first whose fields hold every key, whose required fields are present
    /// (without spreads) and whose literal-typed fields admit the literal
    /// values given.
    pub(super) fn literal_member(
        &mut self,
        object: &ast::ObjectLit,
        members: &[Type],
    ) -> Option<Type> {
        let records: Vec<Type> = members
            .iter()
            .copied()
            .filter(|ty| matches!(ty, Type::Record(_)))
            .collect();
        if records.len() < 2 || !self.types.profile.language_core() {
            return records.first().copied();
        }
        let spread = object
            .props
            .iter()
            .any(|prop| matches!(prop, ast::PropOrSpread::Spread(_)));
        let mut keys: Vec<(String, Option<&ast::Expr>)> = Vec::new();
        for prop in &object.props {
            let ast::PropOrSpread::Prop(prop) = prop else {
                continue;
            };
            match &**prop {
                ast::Prop::Shorthand(id) => keys.push((id.sym.to_string(), None)),
                ast::Prop::KeyValue(pair) => {
                    let key = match &pair.key {
                        ast::PropName::Ident(id) => id.sym.to_string(),
                        ast::PropName::Str(text) => match text.value.as_str() {
                            Some(text) => text.to_owned(),
                            None => return records.first().copied(),
                        },
                        _ => return records.first().copied(),
                    };
                    keys.push((key, Some(&*pair.value)));
                }
                ast::Prop::Method(method) => {
                    if let ast::PropName::Ident(id) = &method.key {
                        keys.push((id.sym.to_string(), None));
                    }
                }
                _ => return records.first().copied(),
            }
        }
        let placeholder = ast::Expr::Ident(ast::Ident::new(
            "#literal".into(),
            object.span,
            Default::default(),
        ));
        records.into_iter().find(|record| {
            let Type::Record(id) = record else {
                return false;
            };
            let fields = self.types.records[*id as usize].fields.clone();
            let declared = keys
                .iter()
                .all(|(key, _)| fields.iter().any(|field| &field.name == key));
            let required = spread
                || fields
                    .iter()
                    .all(|field| field.optional || keys.iter().any(|(key, _)| *key == field.name));
            let literal = keys.iter().all(|(key, value)| {
                let (Some(value), Some(field)) = (value, fields.iter().find(|f| &f.name == key))
                else {
                    return true;
                };
                if !literals::contains(field.ty, self.types) {
                    return true;
                }
                match literals::guard_operand(&placeholder, value, self.types) {
                    Some((_, single)) => admits(field.ty, single, self.types),
                    None => true,
                }
            });
            declared && required && literal
        })
    }
}
