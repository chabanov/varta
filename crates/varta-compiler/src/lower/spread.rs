//! Eager array snapshots preserve spread iteration before later operand effects.
use super::*;
mod body;

pub(super) struct Segment {
    slot: u32,
    ty: Type,
    spread: bool,
    span: swc_common::Span,
}
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn number(span: swc_common::Span, value: usize) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value as f64),
    }
}
fn undefined(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}
fn binary(span: swc_common::Span, op: ast::BinaryOp, a: Expr, b: Expr, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Binary(op, Box::new(a), Box::new(b)),
    }
}
fn empty_literal(mut expr: &ast::Expr) -> bool {
    while let ast::Expr::Paren(p) = expr {
        expr = &p.expr;
    }
    matches!(expr,ast::Expr::Array(a) if a.elems.iter().all(Option::is_none))
}
fn common_context(
    start: usize,
    signature: &ir::FunctionType,
    types: &ir::ValueTypes,
) -> Option<Type> {
    let mut values = signature
        .params
        .iter()
        .skip(start)
        .take(signature.shape.fixed_count().saturating_sub(start))
        .copied()
        .collect::<Vec<_>>();
    if signature.shape.rest {
        values.push(arguments::context(
            signature.shape.fixed_count(),
            &signature.params,
            &signature.shape,
            types,
        )?);
    }
    let first = *values.first()?;
    values.iter().all(|t| *t == first).then_some(first)
}
impl Lower<'_> {
    pub(super) fn spread_call(&mut self, call: &ast::CallExpr, callee: &ast::Expr) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported("spread calls require checked-f64-utf16-v12"));
        }
        let (target, receiver, args) = if let ast::Expr::Member(member) = callee {
            if !matches!(&member.prop,ast::MemberProp::Ident(id) if id.sym=="call") {
                return Err(unsupported("spread to a host or non-function method"));
            }
            let first = call
                .args
                .first()
                .filter(|arg| arg.spread.is_none())
                .ok_or_else(|| {
                    unsupported("spread thisArg; pass an explicit receiver before spread arguments")
                })?;
            let target = self.expr(&member.obj)?;
            let Type::Function(id) = target.ty else {
                return Err(type_error("spread call requires a typed function"));
            };
            let receiver =
                self.expr_expected(&first.expr, self.types.functions[id as usize].receiver)?;
            (target, Some(receiver), &call.args[1..])
        } else {
            (self.expr(callee)?, None, call.args.as_slice())
        };
        let Type::Function(id) = target.ty else {
            return Err(type_error("spread call requires a typed function"));
        };
        let signature = self.types.functions[id as usize].clone();
        let receiver_slot = receiver.as_ref().map(|_| 1);
        let mut operands = vec![target];
        operands.extend(receiver);
        let mut segments = Vec::new();
        let mut known = Some(0usize);
        let mut lower_bound = 0usize;
        for arg in args {
            let context = if let Some(index) = known {
                arguments::context(index, &signature.params, &signature.shape, self.types)
            } else {
                common_context(lower_bound, &signature, self.types)
            };
            let value = if arg.spread.is_some() {
                let expected = if empty_literal(&arg.expr) {
                    Some(arrays::intern(
                        context
                            .filter(|t| t.value_kind().is_some())
                            .unwrap_or(Type::Undefined),
                        self.types,
                    )?)
                } else {
                    None
                };
                let value = self.expr_expected(&arg.expr, expected)?;
                if !matches!(value.ty, Type::Array(_)) {
                    return Err(self.source.locate(
                        type_error("spread operand requires a narrowed checked array"),
                        value.span,
                    ));
                }
                Expr {
                    span: value.span,
                    ty: value.ty,
                    kind: E::ArraySlice(
                        Box::new(value),
                        Box::new(number(call.span, 0)),
                        Box::new(undefined(call.span)),
                    ),
                }
            } else {
                self.expr_expected(&arg.expr, context)?
            };
            segments.push(Segment {
                slot: operands.len() as u32,
                ty: value.ty,
                spread: arg.spread.is_some(),
                span: value.span,
            });
            operands.push(value);
            if arg.spread.is_some() {
                known = None;
            } else {
                lower_bound += 1;
                known = known.map(|n| n + 1);
            }
        }
        if operands.iter().map(|v| v.ty.width()).sum::<u32>() as usize
            > varta_common::abi::MAX_WASM_PARAMETERS
        {
            return Err(invalid(
                "spread operand segments exceed physical parameter limit",
            ));
        }
        let index = *self.next_function;
        if index >= varta_common::abi::MAX_USER_FUNCTIONS {
            return Err(invalid("too many functions including spread wrappers"));
        }
        *self.next_function += 1;
        let mut child = Lower {
            source: self.source,
            types: self.types,
            functions: self.functions,
            scopes: vec![BTreeMap::new()],
            next: 0,
            local_types: BTreeMap::new(),
            refinements: BTreeMap::new(),
            writes: Default::default(),
            call_writes: Default::default(),
            call_unknown: false,
            direct_writes: Default::default(),
            call_parameters: Default::default(),
            parameter_values: self.parameter_values.clone(),
            parameter_roots: Default::default(),
            known_calls: self.known_calls.clone(),
            call_effects: self.call_effects,
            reachable: true,
            control: Default::default(),
            guard_blocked: Default::default(),
            outer_narrows: Default::default(),
            result: signature.result,
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
            generator: None,
            asynchronous: None,
        };
        for value in &operands {
            child.hidden(value.ty);
        }
        let body = child.spread_body(&signature, receiver_slot, &segments, call.span)?;
        let function = ir::Function {
            shape: ir::ParameterShape::fixed(operands.len()),
            self_slot: None,
            receiver: None,
            source: child.source.clone(),
            span: call.span,
            params: operands.iter().map(|v| v.ty).collect(),
            result: signature.result,
            locals: (0..child.next)
                .map(|slot| child.local_types[&slot])
                .collect(),
            body,
            cells: Default::default(),
            captures: vec![],
            environment: false,
            generator: None,
        };
        child.generated.insert(index, function);
        Ok(Expr {
            span: call.span,
            ty: signature.result,
            kind: E::Call(index, operands),
        })
    }
}
