//! Bounded array iteration through ordinary typed calls and rooted lexical frames.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Method {
    Map,
    Filter,
    ForEach,
    Find,
    Some,
    Every,
    Reduce,
}
impl Method {
    fn parse(name: &str) -> Result<Self> {
        Ok(match name {
            "map" => Self::Map,
            "filter" => Self::Filter,
            "forEach" => Self::ForEach,
            "find" => Self::Find,
            "some" => Self::Some,
            "every" => Self::Every,
            "reduce" => Self::Reduce,
            _ => return Err(invalid("unknown array callback method")),
        })
    }
}
pub(super) struct Iteration {
    pub(super) method: Method,
    pub(super) array: Type,
    pub(super) item: Type,
    pub(super) callback: Type,
    pub(super) span: swc_common::Span,
    pub(super) initial: bool,
    pub(super) this_arg: Option<Type>,
}
impl Lower<'_> {
    pub(super) fn array_callback(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
        name: &str,
        expected: Option<Type>,
    ) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported("array callbacks require checked-f64-utf16-v12"));
        }
        let method = Method::parse(name)?;
        if method == Method::Reduce && !(1..=2).contains(&call.args.len()) {
            return Err(unsupported(
                "array.reduce requires a callback and optional initial value",
            ));
        }
        if method != Method::Reduce && !(1..=2).contains(&call.args.len()) {
            return Err(unsupported(
                "array callback method requires a callback and optional thisArg",
            ));
        }
        let base = self.expr(&member.obj)?;
        let Type::Array(id) = base.ty else {
            return Err(type_error(
                "array callback requires a narrowed array receiver",
            ));
        };
        let element = self.types.arrays[id as usize];
        let item = if method == Method::Find {
            unions::intern(vec![element, Type::Undefined], self.types)?
        } else {
            element
        };
        let (initial, resolved_callback) = if method == Method::Reduce && call.args.len() == 2 {
            self.reduce_seed(&call.args[0].expr, &call.args[1].expr, expected)?
        } else {
            (None, None)
        };
        let this_arg = if method != Method::Reduce {
            call.args
                .get(1)
                .map(|arg| self.expr(&arg.expr))
                .transpose()?
        } else {
            None
        };
        let accumulator = initial.as_ref().map_or(element, |value| value.ty);
        let params = if method == Method::Reduce {
            vec![accumulator, item, Type::Float64, base.ty]
        } else {
            vec![item, Type::Float64, base.ty]
        };
        let callback = if let Some(callback) = resolved_callback {
            callback
        } else {
            self.array_callback_value(&call.args[0].expr, &params, method, accumulator)?
        };
        let Type::Function(signature) = callback.ty else {
            return Err(self.source.locate(
                type_error("array callback must be a typed function"),
                callback.span,
            ));
        };
        let signature = self.types.functions[signature as usize].clone();
        if signature.shape.minimum() > params.len()
            || params.iter().enumerate().any(|(index, actual)| {
                arguments::context(index, &signature.params, &signature.shape, self.types)
                    .is_some_and(|expected| !arguments::accepts(expected, *actual, self.types))
            })
        {
            return Err(self.source.locate(type_error("array callback parameter types or required arity do not accept (value/index/array) arguments"),callback.span));
        }
        if let Some(expected) = signature.receiver {
            let actual = this_arg.as_ref().map_or(Type::Undefined, |v| v.ty);
            let accepted = actual == expected
                || expected.is_union()
                    && unions::members(actual, self.types)
                        .iter()
                        .all(|member| unions::members(expected, self.types).contains(member));
            if !accepted {
                return Err(self.source.locate(
                    type_error(format!(
                        "array callback this type mismatch: expected {expected}, found {actual}"
                    )),
                    this_arg.as_ref().map_or(call.span, |v| v.span),
                ));
            }
        }
        let result = match method {
            Method::Map => arrays::intern(
                if signature.result == Type::Void {
                    Type::Undefined
                } else {
                    signature.result
                },
                self.types,
            )?,
            Method::Filter => base.ty,
            Method::ForEach => Type::Void,
            Method::Find => item,
            Method::Some | Method::Every => Type::Boolean,
            Method::Reduce => {
                if signature.result != accumulator {
                    return Err(self.source.locate(
                        type_error(format!(
                            "array.reduce callback result must match accumulator: expected {accumulator}, found {}",
                            signature.result
                        )),
                        callback.span,
                    ));
                }
                accumulator
            }
        };
        let index = *self.next_function;
        if index >= varta_common::abi::MAX_USER_FUNCTIONS {
            return Err(invalid(
                "too many functions including array callback wrappers",
            ));
        }
        *self.next_function += 1;
        let iteration = Iteration {
            method,
            array: base.ty,
            item,
            callback: callback.ty,
            span: call.span,
            initial: initial.is_some(),
            this_arg: this_arg.as_ref().map(|v| v.ty),
        };
        let mut child = Lower {
            source: self.source,
            types: self.types,
            functions: self.functions,
            scopes: vec![BTreeMap::new()],
            next: 0,
            local_types: BTreeMap::new(),
            refinements: BTreeMap::new(),
            result,
            infer_returns: false,
            entry: self.entry,
            loops: 0,
            labels: Vec::new(),
            next_label: 0,
            switches: 0,
            depth: self.depth,
            repeat_slots: None,
            generated: self.generated,
            next_function: self.next_function,
            index,
            outer: BTreeMap::new(),
            captures: BTreeMap::new(),
            cells: Default::default(),
            guarded: Default::default(),
        };
        let body = child.array_iteration(&iteration, signature.params.len(), signature.result)?;
        let mut operands = vec![base, callback];
        operands.extend(initial);
        operands.extend(this_arg);
        let function = ir::Function {
            shape: ir::ParameterShape::fixed(operands.len()),
            self_slot: None,
            receiver: None,
            source: child.source.clone(),
            span: call.span,
            params: operands.iter().map(|value| value.ty).collect(),
            result,
            locals: (0..child.next)
                .map(|slot| child.local_types[&slot])
                .collect(),
            body,
            cells: Default::default(),
            captures: vec![],
            environment: false,
        };
        child.generated.insert(index, function);
        Ok(Expr {
            span: call.span,
            ty: result,
            kind: E::Call(index, operands),
        })
    }
    fn array_callback_value(
        &mut self,
        expr: &ast::Expr,
        params: &[Type],
        method: Method,
        accumulator: Type,
    ) -> Result<Expr> {
        let result = match expr {
            ast::Expr::Paren(p) => self.array_callback_value(&p.expr, params, method, accumulator),
            ast::Expr::Arrow(arrow) => self.arrow_context(
                arrow,
                functions::LiftContext {
                    params: Some(params.to_vec()),
                    shape: None,
                    result: (method == Method::Reduce).then_some(accumulator),
                    infer: true,
                    receiver: None,
                    lexical: true,
                    self_name: None,
                },
            ),
            ast::Expr::Fn(function) => self.function_expression_context(
                function,
                Some(params.to_vec()),
                (method == Method::Reduce).then_some(accumulator),
                None,
                None,
            ),
            _ => self.expr(expr),
        };
        result.map_err(|error| self.source.locate(error, expr.span()))
    }
}
