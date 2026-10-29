//! Function values as tagged values (v36 `function_fields`, value layout 17):
//! calls through record and class fields, optional calls (`f?.()`), the
//! function member of a union as a literal context, and built-in functions
//! referenced as values. Calls through a field never bind `this`: a field
//! whose function type declares a `this` parameter is rejected at the call.
use super::*;

/// Built-in functions that may be referenced as values, with the namespace
/// and property naming them: each is one synthesized wrapper with a canonical
/// identity (`Math.random === Math.random`).
const BUILTIN_VALUES: &[(&str, &str, &str)] = &[("Math.random", "Math", "random")];

fn member_name(prop: &ast::MemberProp) -> Option<String> {
    match prop {
        ast::MemberProp::Ident(id) => Some(id.sym.to_string()),
        ast::MemberProp::Computed(key) => match stmt::unparen(&key.expr) {
            ast::Expr::Lit(ast::Lit::Str(s)) => s.value.as_str().map(str::to_owned),
            _ => None,
        },
        ast::MemberProp::PrivateName(_) => None,
    }
}

/// Whether a value of `ty` may hold a record or class field named `toJSON`
/// whose value is a function: `JSON.stringify` would call it, which the host
/// serializer cannot do, so such values are rejected (an `unknown` cell that
/// holds one fails at run time instead).
pub(super) fn reaches_callable_to_json(ty: Type, types: &ir::ValueTypes) -> bool {
    fn visit(
        ty: Type,
        types: &ir::ValueTypes,
        seen: &mut std::collections::BTreeSet<Type>,
    ) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        let fields = match ty {
            Type::Record(id) => types.records[id as usize].fields.clone(),
            Type::Class(id) => return visit(Type::Record(types.classes.shape(id)), types, seen),
            Type::Array(id) => return visit(types.arrays[id as usize], types, seen),
            Type::Dictionary(id) => {
                return visit(types.dictionaries[id as usize].value, types, seen);
            }
            Type::Union(id) | Type::ReferenceUnion(id) => {
                return types.unions[id as usize]
                    .clone()
                    .into_iter()
                    .any(|member| visit(member, types, seen));
            }
            _ => return false,
        };
        fields.into_iter().any(|field| {
            field.name == "toJSON"
                && unions::members(field.ty, types)
                    .iter()
                    .any(|member| matches!(member, Type::Function(_)))
                || visit(field.ty, types, seen)
        })
    }
    types.profile.function_fields() && visit(ty, types, &mut Default::default())
}

impl Lower<'_> {
    /// `Math.random` read as a value: a `() => number` function whose body is
    /// the intrinsic call. The namespace must not be shadowed.
    pub(super) fn builtin_function_value(
        &mut self,
        member: &ast::MemberExpr,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.function_fields() {
            return Ok(None);
        }
        let ast::Expr::Ident(namespace) = stmt::unparen(&member.obj) else {
            return Ok(None);
        };
        let Some(property) = member_name(&member.prop) else {
            return Ok(None);
        };
        let Some(&(name, owner, _)) = BUILTIN_VALUES
            .iter()
            .find(|(_, owner, method)| namespace.sym == *owner && property == *method)
        else {
            return Ok(None);
        };
        if self.binding(owner).is_ok() || self.functions.contains_key(owner) {
            return Ok(None);
        }
        let span = member.span;
        let ty = functions::intern_full(
            None,
            Vec::new(),
            ir::ParameterShape::fixed(0),
            Type::Float64,
            self.types,
        )?;
        let index = match self.types.builtin_functions.get(name) {
            Some(index) => *index,
            None => {
                let index = *self.next_function;
                if index >= varta_common::abi::MAX_USER_FUNCTIONS {
                    return Err(invalid("too many functions including built-in values"));
                }
                *self.next_function += 1;
                self.generated.insert(
                    index,
                    ir::Function {
                        shape: ir::ParameterShape::fixed(0),
                        self_slot: None,
                        receiver: None,
                        cells: Default::default(),
                        captures: Vec::new(),
                        environment: false,
                        generator: None,
                        source: self.source.clone(),
                        span,
                        params: Vec::new(),
                        result: Type::Float64,
                        locals: Vec::new(),
                        body: vec![S::Return(Some(Expr {
                            span,
                            ty: Type::Float64,
                            kind: E::Math(ir::MathIntrinsic::Random, Vec::new()),
                        }))],
                    },
                );
                self.types.builtin_functions.insert(name, index);
                index
            }
        };
        Ok(Some(Expr {
            span,
            ty,
            kind: E::FunctionRef {
                index,
                fresh: false,
                captures: Vec::new(),
            },
        }))
    }
    /// The static type of a side-effect-free receiver path (an identifier,
    /// `this` or a member chain of records and class instances), determined
    /// without lowering it, so a call can select field dispatch before the
    /// builtin method names.
    pub(super) fn peek_type(&self, expression: &ast::Expr, depth: usize) -> Option<Type> {
        if depth >= 64 {
            return None;
        }
        match stmt::unparen(expression) {
            ast::Expr::This(_) => self.binding("this").ok()?.ty,
            ast::Expr::Ident(id) => {
                let binding = self.binding(id.sym.as_ref()).ok()?;
                self.refinements.get(&binding.slot).copied().or(binding.ty)
            }
            ast::Expr::Member(member) => {
                let base = self.peek_type(&member.obj, depth + 1)?;
                if let Type::Dictionary(id) = base {
                    // A dictionary read is the stored value or `undefined`.
                    let value = self.types.dictionaries[id as usize].value;
                    return matches!(value, Type::Function(_)).then_some(value);
                }
                let name = member_name(&member.prop)?;
                self.field_type(base, &name)
            }
            // `x?.p` with a single non-nullish member: an `unknown` field stays
            // `unknown` (its `undefined` short circuit is absorbed).
            ast::Expr::OptChain(chain) => {
                let ast::OptChainBase::Member(member) = &*chain.base else {
                    return None;
                };
                let base = self.peek_type(&member.obj, depth + 1)?;
                let mut present = unions::members(base, self.types)
                    .into_iter()
                    .filter(|ty| !matches!(ty, Type::Null | Type::Undefined));
                let base = present.next()?;
                if present.next().is_some() {
                    return None;
                }
                let name = member_name(&member.prop)?;
                self.field_type(base, &name)
                    .filter(|ty| *ty == Type::Unknown)
            }
            _ => None,
        }
    }
    fn field_type(&self, base: Type, name: &str) -> Option<Type> {
        match base {
            Type::Record(id) => self.types.records[id as usize]
                .fields
                .iter()
                .find(|field| field.name == name)
                .map(|field| field.ty),
            Type::Class(class) => match self.types.classes.field(class, name) {
                Some((_, field)) => Some(field.ty),
                None => self
                    .types
                    .classes
                    .get(class)
                    .methods
                    .get(name)
                    .filter(|method| method.getter)
                    .map(|method| method.signature.result),
            },
            _ => None,
        }
    }
    /// Whether `callee` is a member path whose field holds a function value,
    /// possibly together with `undefined` (an optional field or a dictionary
    /// entry, which must be narrowed or called with `?.()`).
    pub(super) fn function_field_callee(&self, callee: &ast::Expr) -> bool {
        self.types.profile.function_fields()
            && matches!(stmt::unparen(callee), ast::Expr::Member(_))
            && self.peek_type(callee, 0).is_some_and(|ty| {
                unions::members(ty, self.types)
                    .iter()
                    .any(|member| matches!(member, Type::Function(_)))
            })
    }
    /// `object.field(args)` where the field holds a function value: the field
    /// is read once and called without a receiver.
    pub(super) fn function_field_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &ast::Expr,
    ) -> Result<Expr> {
        let value = self.expr(callee)?;
        if !matches!(value.ty, Type::Function(_)) {
            return Err(self.source.locate(
                type_error(format!(
                    "called field may not be a function ({}); narrow it or call it with ?.()",
                    value.ty
                )),
                value.span,
            ));
        }
        self.unbound(&value)?;
        self.function_call_value(call, value)
    }
    fn unbound(&self, value: &Expr) -> Result<()> {
        if let Type::Function(id) = value.ty
            && self.types.functions[id as usize].receiver.is_some()
        {
            return Err(self.source.locate(
                unsupported(
                    "method-style call of a function field whose type declares a this parameter",
                ),
                value.span,
            ));
        }
        Ok(())
    }
    /// Call an already lowered function value with the call's arguments.
    pub(super) fn function_call_value(
        &mut self,
        call: &ast::CallExpr,
        callee: Expr,
    ) -> Result<Expr> {
        let Type::Function(index) = callee.ty else {
            return Err(type_error("called value is not a typed function"));
        };
        let signature = self.types.functions[index as usize].clone();
        if !signature.shape.extended() && call.args.len() != signature.params.len() {
            return Err(invalid("callback argument count mismatch"));
        }
        let args = self.call_arguments(&call.args, &signature.params, &signature.shape)?;
        self.invoke_value(callee, args, call.span)
    }
    /// The function member of an expected union (`cb?: () => void`), used as
    /// the context of a function literal that is then boxed into the union.
    pub(super) fn function_context(&self, expected: Option<Type>) -> Option<Type> {
        let expected = expected?;
        if !self.types.profile.function_fields() || !expected.is_union() {
            return None;
        }
        let mut functions = unions::members(expected, self.types)
            .into_iter()
            .filter(|ty| matches!(ty, Type::Function(_)));
        let function = functions.next()?;
        functions.next().is_none().then_some(function)
    }
}
