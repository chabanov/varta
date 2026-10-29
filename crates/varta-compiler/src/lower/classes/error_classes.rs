//! Error subclasses (v32 `error_subclasses()`). `class X extends Error` (or a
//! native family) descends from a synthetic root class of that family; the
//! TypeError/RangeError/SyntaxError roots extend the Error root, mirroring the
//! native prototype chain. Roots are never constructed, have no source module
//! and own no functions: they only carry the inherited `message`, `name` and
//! `cause` slots, and `super(...)` to a root is lowered inline. Instances stay
//! ordinary class records, so identity, dispatch, tracing and `instanceof`
//! reuse the class machinery; the catch union gains the Error root, and the
//! exception channel v5 lets the runtime report an uncaught instance.
use super::*;
use varta_common::value::ErrorClass;

pub(in crate::lower) const MESSAGE: &str = "message";
pub(in crate::lower) const NAME: &str = "name";
pub(in crate::lower) const CAUSE: &str = "cause";
pub(in crate::lower) const TO_STRING: &str = "toString";
const OPTIONS: &str = "options";
/// `Error.prototype.toString` joins a non-empty name and message with this.
const SEPARATOR: &str = ": ";
/// Roots belong to no source module, so per-module layout and body lowering
/// never visit them.
const NO_MODULE: usize = usize::MAX;
/// Index of the options parameter in the root initializer signature.
const OPTIONS_PARAMETER: usize = 1;

impl Classes {
    pub(in crate::lower) fn error_root(&self, family: ErrorClass) -> Option<u32> {
        self.error_roots.get(&family).copied()
    }
    /// The native family a synthetic root stands for; `None` for source classes.
    pub(in crate::lower) fn root_family(&self, id: u32) -> Option<ErrorClass> {
        self.error_roots
            .iter()
            .find(|(_, root)| **root == id)
            .map(|(family, _)| *family)
    }
    /// Whether instances of `id` are Error objects (`id` descends from the Error root).
    pub(in crate::lower) fn is_error(&self, id: u32) -> bool {
        self.error_root(ErrorClass::Error)
            .is_some_and(|root| self.descends(id, root))
    }
}

/// The synthetic root of `family`, created (with the Error root) on first use.
pub(in crate::lower) fn root(
    family: ErrorClass,
    source: &Source,
    span: Span,
    types: &mut ir::ValueTypes,
) -> Result<u32> {
    if let Some(id) = types.classes.error_root(family) {
        return Ok(id);
    }
    let parent = match family {
        ErrorClass::Error => None,
        _ => Some(root(ErrorClass::Error, source, span, types)?),
    };
    if types.classes.list.len() >= MAX_RECORD_SHAPES {
        return Err(source.locate(invalid("class count exceeds limit"), span));
    }
    let id = types.classes.list.len() as u32;
    types.classes.list.push(Class {
        name: family.name().to_owned(),
        module: NO_MODULE,
        source: source.clone(),
        span,
        declaration: Box::default(),
        parent,
        shape: 0,
        fields: Vec::new(),
        methods: BTreeMap::new(),
        statics: BTreeMap::new(),
        init: None,
        initializers: Vec::new(),
        state: State::Ready,
        prelude: false,
    });
    types.classes.error_roots.insert(family, id);
    let (fields, mut init) = match parent {
        Some(parent) => {
            let parent = &types.classes.list[parent as usize];
            let init = parent
                .init
                .clone()
                .ok_or_else(|| CompilerError::Internal("missing Error root initializer".into()))?;
            (parent.fields.clone(), init)
        }
        None => base_members(id, types).map_err(|e| source.locate(e, span))?,
    };
    init.receiver = Some(Type::Class(id));
    if types.records.len() >= MAX_RECORD_SHAPES {
        return Err(source.locate(invalid("object shape count exceeds limit"), span));
    }
    let shape = types.records.len() as u32;
    types.records.push(ir::RecordShape {
        fields: fields
            .iter()
            .map(|field| ir::RecordField {
                name: field.name.clone(),
                ty: field.ty,
                optional: false,
            })
            .collect(),
        class: Some(id),
    });
    let entry = &mut types.classes.list[id as usize];
    entry.shape = shape;
    entry.fields = fields;
    entry.init = Some(init);
    Ok(id)
}

/// Writable `message`, `name` and `cause` slots and the `(message?, options?)`
/// initializer signature implicit subclass constructors forward.
fn base_members(id: u32, types: &mut ir::ValueTypes) -> Result<(Vec<Field>, Signature)> {
    // The cause holds any throwable value, including other Error instances,
    // and any `unknown` value under v34.
    let cause = exceptions::cause_type(types)?;
    let message = unions::intern(vec![Type::String, Type::Undefined], types)?;
    let options = records::intern(
        vec![ir::RecordField {
            name: CAUSE.to_owned(),
            ty: cause,
            optional: true,
        }],
        types,
    )?;
    let options = unions::intern(vec![options, Type::Undefined], types)?;
    let field = |name: &str, ty| Field {
        name: name.to_owned(),
        ty,
        owner: id,
        access: Access::Public,
        readonly: false,
        redeclared: None,
    };
    let signature = Signature {
        shape: ir::ParameterShape {
            optional: vec![true, true],
            rest: false,
        },
        receiver: Some(Type::Class(id)),
        index: 0,
        names: vec![Some(MESSAGE.to_owned()), Some(OPTIONS.to_owned())],
        params: vec![message, options],
        result: Type::Void,
    };
    Ok((
        vec![
            field(MESSAGE, Type::String),
            field(NAME, Type::String),
            field(CAUSE, cause),
        ],
        signature,
    ))
}

/// Replace the Error-instance class members of a prospective union by their
/// nearest common ancestor class: every Error class descends from the Error
/// root and all class members share the record union tag, so the union keeps
/// one class member and narrowing recovers subclasses through `instanceof`.
pub(in crate::lower) fn join_error_classes(values: Vec<Type>, types: &ir::ValueTypes) -> Vec<Type> {
    let classes = &types.classes;
    let errors: Vec<u32> = values
        .iter()
        .filter_map(|ty| match ty {
            Type::Class(id) if classes.is_error(*id) => Some(*id),
            _ => None,
        })
        .collect();
    let Some((&first, rest)) = errors.split_first() else {
        return values;
    };
    if rest.iter().all(|id| *id == first) {
        return values;
    }
    let common = rest.iter().fold(first, |mut ancestor, id| {
        while !classes.descends(*id, ancestor) {
            // The Error root is a common ancestor of every Error class.
            match classes.get(ancestor).parent {
                Some(parent) => ancestor = parent,
                None => break,
            }
        }
        ancestor
    });
    values
        .into_iter()
        .filter(|ty| !matches!(ty, Type::Class(id) if classes.is_error(*id)))
        .chain([Type::Class(common)])
        .collect()
}

/// `new.target.name`, the only admitted `new.target` form.
pub(in crate::lower) fn new_target_name(member: &ast::MemberExpr) -> bool {
    matches!(
        &*member.obj,
        ast::Expr::MetaProp(meta) if meta.kind == ast::MetaPropKind::NewTarget
    ) && matches!(&member.prop, ast::MemberProp::Ident(id) if id.sym == NAME)
}

/// Whether a value of `ty` may reach an Error subclass instance, whose own
/// property enumerability (non-enumerable message/cause) is not modelled.
pub(in crate::lower) fn reaches_instance(ty: Type, types: &ir::ValueTypes) -> bool {
    // `unknown` (v34) reaches an instance whenever an Error class exists.
    let unknown = types.classes.error_root(ErrorClass::Error).is_some();
    types.profile.error_subclasses()
        && reaches(ty, types, &|id| types.classes.is_error(id), unknown)
}

/// Whether a value of `ty` may reach an engine prelude instance (v35 Web
/// types), which has no own enumerable data property in JavaScript.
pub(in crate::lower) fn reaches_prelude(ty: Type, types: &ir::ValueTypes) -> bool {
    let unknown = types.classes.has_prelude();
    reaches(ty, types, &|id| types.classes.get(id).prelude, unknown)
}

fn reaches(ty: Type, types: &ir::ValueTypes, target: &dyn Fn(u32) -> bool, unknown: bool) -> bool {
    fn visit(
        ty: Type,
        types: &ir::ValueTypes,
        (target, unknown): (&dyn Fn(u32) -> bool, bool),
        seen: &mut BTreeSet<Type>,
    ) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        let visit = |ty, seen: &mut BTreeSet<Type>| visit(ty, types, (target, unknown), seen);
        match ty {
            Type::Class(id) if target(id) => true,
            Type::Class(id) => visit(Type::Record(types.classes.shape(id)), seen),
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .any(|f| visit(f.ty, seen)),
            Type::Array(id) => visit(types.arrays[id as usize], seen),
            Type::Dictionary(id) => visit(types.dictionaries[id as usize].value, seen),
            Type::Union(id) | Type::ReferenceUnion(id) => {
                types.unions[id as usize].iter().any(|&t| visit(t, seen))
            }
            Type::Unknown => unknown,
            _ => false,
        }
    }
    visit(ty, types, (target, unknown), &mut BTreeSet::new())
}

fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn boolean(span: Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn undefined(span: Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}
fn conditional(span: Span, ty: Type, test: Expr, yes: Expr, no: Expr) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Conditional(Box::new(test), Box::new(yes), Box::new(no)),
    }
}
fn binary(span: Span, op: ast::BinaryOp, ty: Type, left: Expr, right: Expr) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}

impl Lower<'_> {
    fn text(&self, span: Span, text: &str) -> Expr {
        Expr {
            span,
            ty: Type::String,
            kind: E::String(self.types.profile.string_bytes(text)),
        }
    }
    fn root_slot(&self, name: &str) -> Result<u32> {
        self.types
            .classes
            .error_root(ErrorClass::Error)
            .and_then(|root| self.types.classes.field(root, name))
            .map(|(slot, _)| slot)
            .ok_or_else(|| CompilerError::Internal("missing Error root field".into()))
    }
    /// `super(message?, options?)` to a native-error root: evaluate both
    /// arguments once, in order, then set `message` (empty when undefined),
    /// the family's default `name` and `cause` (`options.cause`, or undefined).
    pub(in crate::lower) fn error_super_call(
        &mut self,
        call: &ast::CallExpr,
        root: u32,
    ) -> Result<Expr> {
        let span = call.span;
        if call.type_args.is_some()
            || call.args.len() > OPTIONS_PARAMETER + 1
            || call.args.iter().any(|arg| arg.spread.is_some())
        {
            return Err(self.source.locate(
                unsupported("super(...) of an Error subclass other than (message?, options?)"),
                span,
            ));
        }
        let family = self
            .types
            .classes
            .root_family(root)
            .ok_or_else(|| CompilerError::Internal("missing Error root family".into()))?;
        let options_ty = self
            .types
            .classes
            .get(root)
            .init
            .as_ref()
            .map(|init| init.params[OPTIONS_PARAMETER])
            .ok_or_else(|| CompilerError::Internal("missing Error root initializer".into()))?;
        let message = match call.args.first() {
            Some(arg) => self.expr(&arg.expr)?,
            None => undefined(span),
        };
        if unions::members(message.ty, self.types)
            .iter()
            .any(|ty| !matches!(ty, Type::String | Type::Undefined))
        {
            return Err(self.source.locate(
                type_error("Error message must be string or undefined; coercion is unsupported"),
                message.span,
            ));
        }
        let options = match call.args.get(OPTIONS_PARAMETER) {
            Some(arg) if matches!(stmt::unparen(&arg.expr), ast::Expr::Object(_)) => {
                self.expr_expected(&arg.expr, Some(options_ty))?
            }
            Some(arg) => self.expr(&arg.expr)?,
            None => undefined(span),
        };
        if unions::members(options.ty, self.types)
            .iter()
            .any(|ty| !matches!(ty, Type::Undefined | Type::Record(_)))
        {
            return Err(self.source.locate(
                type_error("Error options must be an object type or undefined"),
                options.span,
            ));
        }
        let (message_ty, options_ty) = (message.ty, options.ty);
        let input = self.hidden(message_ty);
        let bag = self.hidden(options_ty);
        let marker = self.hidden(Type::Number);
        let text = match message_ty {
            Type::String => local(span, input, Type::String),
            Type::Undefined => self.text(span, ""),
            _ => conditional(
                span,
                Type::String,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUndefined(Box::new(local(span, input, message_ty))),
                },
                self.text(span, ""),
                Expr {
                    span,
                    ty: Type::String,
                    kind: E::UnboxUnion(Box::new(local(span, input, message_ty))),
                },
            ),
        };
        let cause = self.error_cause(span, bag, options_ty)?;
        let slots = [
            self.root_slot(MESSAGE)?,
            self.root_slot(NAME)?,
            self.root_slot(CAUSE)?,
        ];
        let mut body = vec![S::Init(input, message), S::Init(bag, options)];
        for (slot, value) in slots
            .into_iter()
            .zip([text, self.text(span, family.name()), cause])
        {
            body.push(S::SetField(self.this_value(span)?, slot, value));
        }
        Ok(Expr {
            span,
            ty: Type::Void,
            kind: E::ValueBlock {
                output: marker,
                marker,
                roots: [(input, message_ty), (bag, options_ty)]
                    .into_iter()
                    .filter(|(_, ty)| ty.is_handle())
                    .map(|(slot, _)| slot)
                    .collect(),
                body,
            },
        })
    }
    /// InstallErrorCause for a checked options value held in `bag`.
    fn error_cause(&mut self, span: Span, bag: u32, ty: Type) -> Result<Expr> {
        let target = exceptions::cause_type(self.types)?;
        let absent = unions::widen(undefined(span), Some(target), self.types)?;
        let members = unions::members(ty, self.types);
        let Some(record) = members
            .iter()
            .copied()
            .find(|m| matches!(m, Type::Record(_)))
        else {
            return Ok(absent);
        };
        let value = if ty.is_union() {
            Expr {
                span,
                ty: record,
                kind: E::UnboxUnion(Box::new(local(span, bag, ty))),
            }
        } else {
            local(span, bag, ty)
        };
        let Type::Record(shape) = record else {
            return Err(CompilerError::Internal(
                "missing Error options record".into(),
            ));
        };
        let found = self.types.records[shape as usize]
            .fields
            .iter()
            .position(|field| field.name == CAUSE);
        let present = match found {
            Some(slot) => {
                let field = Expr {
                    span,
                    ty: self.types.records[shape as usize].fields[slot].ty,
                    kind: E::Field(Box::new(value), slot as u32),
                };
                unions::widen(field, Some(target), self.types).map_err(|_| {
                    self.source.locate(
                        type_error("Error cause must be a primitive, BigInt or Error value"),
                        span,
                    )
                })?
            }
            // A missing own `cause` installs nothing; the slot reads undefined.
            None => unions::widen(undefined(span), Some(target), self.types)?,
        };
        if !ty.is_union() {
            return Ok(present);
        }
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsUndefined(Box::new(local(span, bag, ty))),
        };
        Ok(conditional(span, target, test, absent, present))
    }
    /// `new.target.name` in a constructor: the name of the most-derived class
    /// being constructed, selected by the receiver's class descriptor.
    pub(in crate::lower) fn new_target_member(&mut self, member: &ast::MemberExpr) -> Result<Expr> {
        let span = member.span;
        let Some(context) = self
            .types
            .classes
            .context
            .filter(|c| c.init == Some(self.index))
        else {
            return Err(self.source.locate(
                unsupported("new.target.name outside a class constructor"),
                span,
            ));
        };
        let classes = &self.types.classes;
        let class = context.class;
        let descendants: Vec<(u32, String)> = (0..classes.list.len() as u32)
            .filter(|id| *id != class && classes.descends(*id, class))
            .map(|id| (classes.shape(id), classes.get(id).name.clone()))
            .collect();
        let mut result = self.text(span, &classes.get(class).name.clone());
        for (shape, name) in descendants.into_iter().rev() {
            let test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsClass {
                    value: Box::new(self.this_value(span)?),
                    shapes: vec![shape],
                    slot: self.hidden(Type::Number),
                },
            };
            result = conditional(span, Type::String, test, self.text(span, &name), result);
        }
        Ok(result)
    }
    pub(in crate::lower) fn class_error_to_string(
        &mut self,
        call: &ast::CallExpr,
        value: Expr,
    ) -> Result<Expr> {
        if !call.args.is_empty() {
            return Err(self.source.locate(
                unsupported("toString requires Error receiver and no arguments"),
                call.span,
            ));
        }
        self.class_error_text(value, call.span)
    }
    /// `Error.prototype.toString` over the instance's `name` and `message`.
    pub(in crate::lower) fn class_error_text(&mut self, value: Expr, span: Span) -> Result<Expr> {
        let ty = value.ty;
        let (name_slot, message_slot) = (self.root_slot(NAME)?, self.root_slot(MESSAGE)?);
        let object = self.hidden(ty);
        let name = self.hidden(Type::String);
        let message = self.hidden(Type::String);
        let output = self.hidden(Type::String);
        let marker = self.hidden(Type::Number);
        let read = |slot| Expr {
            span,
            ty: Type::String,
            kind: E::Field(Box::new(local(span, object, ty)), slot),
        };
        let empty = |lower: &Self, slot| {
            binary(
                span,
                ast::BinaryOp::EqEqEq,
                Type::Boolean,
                local(span, slot, Type::String),
                lower.text(span, ""),
            )
        };
        let joined = binary(
            span,
            ast::BinaryOp::Add,
            Type::String,
            binary(
                span,
                ast::BinaryOp::Add,
                Type::String,
                local(span, name, Type::String),
                self.text(span, SEPARATOR),
            ),
            local(span, message, Type::String),
        );
        let text = conditional(
            span,
            Type::String,
            empty(self, name),
            local(span, message, Type::String),
            conditional(
                span,
                Type::String,
                empty(self, message),
                local(span, name, Type::String),
                joined,
            ),
        );
        Ok(Expr {
            span,
            ty: Type::String,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![object, name, message],
                body: vec![
                    S::Init(object, value),
                    S::Init(name, read(name_slot)),
                    S::Init(message, read(message_slot)),
                    S::Init(output, text),
                ],
            },
        })
    }
    /// The Error-instance class member of `ty` and its other members, when
    /// every other member is a native error (for example after `instanceof Error`).
    pub(in crate::lower) fn error_union(&self, ty: Type) -> Option<(Type, Vec<Type>)> {
        if !self.types.profile.error_subclasses() || !ty.is_union() {
            return None;
        }
        let members = unions::members(ty, self.types);
        let class = members
            .iter()
            .copied()
            .find(|m| matches!(m, Type::Class(id) if self.types.classes.is_error(*id)))?;
        let natives: Vec<_> = members.into_iter().filter(|m| *m != class).collect();
        natives
            .iter()
            .all(|m| m.is_error())
            .then_some((class, natives))
    }
    /// Read `name`/`message` (or `toString()` with `text`) of a mixed native
    /// error / Error-instance union by testing the class member first.
    pub(in crate::lower) fn error_union_read(
        &mut self,
        value: Expr,
        field: &str,
        text: bool,
        span: Span,
    ) -> Result<Expr> {
        let union = value.ty;
        let (class, natives) = self
            .error_union(union)
            .ok_or_else(|| CompilerError::Internal("missing Error instance union".into()))?;
        let saved = self.hidden(union);
        let output = self.hidden(Type::String);
        let marker = self.hidden(Type::Number);
        let instance = Expr {
            span,
            ty: class,
            kind: E::UnboxUnion(Box::new(local(span, saved, union))),
        };
        let instance = if text {
            self.class_error_text(instance, span)?
        } else {
            Expr {
                span,
                ty: Type::String,
                kind: E::Field(Box::new(instance), self.root_slot(field)?),
            }
        };
        let native = match natives.as_slice() {
            [single] => Expr {
                span,
                ty: *single,
                kind: E::UnboxUnion(Box::new(local(span, saved, union))),
            },
            _ => local(span, saved, unions::intern(natives, self.types)?),
        };
        let native = Box::new(native);
        let native = Expr {
            span,
            ty: Type::String,
            kind: match (text, field) {
                (true, _) => E::ErrorText(native),
                (false, NAME) => E::ErrorName(native),
                _ => E::ErrorMessage(native),
            },
        };
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsUnionMember(Box::new(local(span, saved, union)), class),
        };
        Ok(Expr {
            span,
            ty: Type::String,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, value),
                    S::Init(
                        output,
                        conditional(span, Type::String, test, instance, native),
                    ),
                ],
            },
        })
    }
    /// Whether some member of `ty` is an Error-instance class.
    pub(in crate::lower) fn has_error_instances(&self, ty: Type) -> bool {
        unions::members(ty, self.types)
            .iter()
            .any(|m| matches!(m, Type::Class(id) if self.types.classes.is_error(*id)))
    }
    /// `value instanceof Error/TypeError/...` when `value` may hold an Error
    /// instance: the native host test or the class test against the family root.
    pub(in crate::lower) fn error_instance_test(
        &mut self,
        value: Expr,
        family: ErrorClass,
        span: Span,
    ) -> Result<Expr> {
        let root = self.types.classes.error_root(family);
        if !value.ty.is_cell() {
            return match root {
                Some(root) => self.class_test(value, root, span),
                None => Ok(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Sequence(Box::new(value), Box::new(boolean(span, false))),
                }),
            };
        }
        let union = value.ty;
        let saved = self.hidden(union);
        let result = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        let native = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Host(
                self.types.profile.host_operation(Host::ValueIsErrorClass),
                vec![
                    local(span, saved, union),
                    Expr {
                        span,
                        ty: Type::Number,
                        kind: E::Integer(family as i32),
                    },
                ],
            ),
        };
        let instance = match root {
            Some(root) => self.class_test(local(span, saved, union), root, span)?,
            None => boolean(span, false),
        };
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output: result,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, value),
                    S::Init(
                        result,
                        conditional(span, Type::Boolean, native, boolean(span, true), instance),
                    ),
                ],
            },
        })
    }
    /// `x instanceof Error/TypeError/...` refinement when `x` may hold an Error
    /// instance: keep members that may match (positive) or may fail (negative).
    pub(in crate::lower) fn refine_error_family(
        &mut self,
        name: &str,
        family: ErrorClass,
        positive: bool,
    ) -> Result<()> {
        self.refine_with(name, |ty, types| {
            if ty == Type::Unknown {
                return positive.then(|| unknown::error_family(family, types));
            }
            if !ty.is_union() {
                return None;
            }
            let classes = &types.classes;
            let root = classes.error_root(family);
            let guard = references::Guard::Error(family);
            Some(
                unions::members(ty, types)
                    .into_iter()
                    .filter(|member| {
                        let (certain, possible) = match *member {
                            Type::Class(id) if classes.is_error(id) => {
                                let certain = root.is_some_and(|r| classes.descends(id, r));
                                let possible =
                                    certain || root.is_some_and(|r| classes.descends(r, id));
                                (certain, possible)
                            }
                            other => {
                                let matched = guard.matches(other);
                                (matched, matched)
                            }
                        };
                        if positive { possible } else { !certain }
                    })
                    .collect(),
            )
        })
    }
    /// `x instanceof C` for an Error class `C` below the class member of `x`:
    /// the descriptor test proves the subclass, so the binding narrows to `C`.
    pub(in crate::lower) fn refine_error_downcast(&mut self, name: &str, class: u32) -> Result<()> {
        self.refine_with(name, |ty, types| {
            unions::members(ty, types)
                .into_iter()
                .any(|m| matches!(m, Type::Class(id) if types.classes.descends(class, id)))
                .then(|| vec![Type::Class(class)])
        })
    }
    /// Caught `name`/`message` of an Error instance member of the catch union,
    /// or `None` when the program declares no Error subclass.
    pub(in crate::lower) fn caught_instance_field(
        &mut self,
        span: Span,
        source: u32,
        ty: Type,
        name: &str,
    ) -> Result<Option<(Expr, Expr)>> {
        let Some(class) = unions::members(ty, self.types)
            .into_iter()
            .find(|m| matches!(m, Type::Class(id) if self.types.classes.is_error(*id)))
        else {
            return Ok(None);
        };
        let value = Expr {
            span,
            ty: class,
            kind: E::UnboxUnion(Box::new(local(span, source, ty))),
        };
        let field = Expr {
            span,
            ty: Type::String,
            kind: E::Field(Box::new(value), self.root_slot(name)?),
        };
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsUnionMember(Box::new(local(span, source, ty)), class),
        };
        Ok(Some((test, field)))
    }
}
