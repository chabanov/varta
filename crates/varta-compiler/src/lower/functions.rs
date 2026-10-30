//! Exact callback types, lexical capture discovery and arrow lifting.
use super::*;
fn required(types: &ir::ValueTypes) -> Result<()> {
    if types.profile.function_values() {
        Ok(())
    } else {
        Err(unsupported("function values require checked-f64-utf16-v11"))
    }
}
pub(super) fn intern_full(
    receiver: Option<Type>,
    params: Vec<Type>,
    shape: ir::ParameterShape,
    result: Type,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    required(types)?;
    if receiver
        .into_iter()
        .chain(params.iter().copied())
        .map(Type::width)
        .sum::<u32>() as usize
        > varta_common::abi::MAX_WASM_PARAMETERS
    {
        return Err(invalid("callback parameters exceed Wasm slot limit"));
    }
    let signature = ir::FunctionType {
        shape,
        receiver,
        params,
        result,
    };
    if let Some(index) = types.functions.iter().position(|ty| ty == &signature) {
        return Ok(Type::Function(index as u32));
    }
    if types.functions.len() >= varta_common::function::MAX_TYPES {
        return Err(invalid("function type count exceeds limit"));
    }
    let index = types.functions.len() as u32;
    types.functions.push(signature);
    Ok(Type::Function(index))
}
pub(super) fn type_annotation(
    function: &ast::TsFnType,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    required(types)?;
    if function.type_params.is_some() {
        return Err(unsupported("generic callback type"));
    }
    let mut patterns = Vec::new();
    let mut receiver = None;
    for parameter in &function.params {
        match parameter {
            ast::TsFnParam::Ident(id) if id.id.sym == "this" => {
                if id.optional
                    || receiver.is_some()
                    || !patterns.is_empty()
                    || !types.profile.closures()
                {
                    return Err(unsupported(
                        "this must be the first function-type parameter in v12 or later",
                    ));
                }
                receiver = Some(function_expressions::receiver_type(
                    id.type_ann.as_deref(),
                    types,
                )?);
            }
            ast::TsFnParam::Ident(id) => patterns.push(ast::Pat::Ident(id.clone())),
            ast::TsFnParam::Rest(rest) => patterns.push(ast::Pat::Rest(rest.clone())),
            ast::TsFnParam::Array(array) => patterns.push(ast::Pat::Array(array.clone())),
            ast::TsFnParam::Object(object) => patterns.push(ast::Pat::Object(object.clone())),
        }
    }
    let plans = parameters::analyze(&patterns, None, None, types, None)?;
    let params = plans.iter().map(|p| p.incoming).collect();
    let shape = parameters::shape(&plans);
    let result = annotation(Some(&function.type_ann), types)?
        .ok_or_else(|| invalid("callback result annotation required"))?;
    intern_full(receiver, params, shape, result, types)
}
pub(super) struct LiftContext {
    pub params: Option<Vec<Type>>,
    pub shape: Option<ir::ParameterShape>,
    pub result: Option<Type>,
    pub infer: bool,
    pub receiver: Option<Type>,
    pub lexical: bool,
    pub self_name: Option<ast::Ident>,
}
impl Lower<'_> {
    pub(super) fn function_reference(
        &mut self,
        signature: &Signature,
        span: swc_common::Span,
    ) -> Result<Expr> {
        required(self.types)?;
        if signature.index == self.entry {
            return Err(unsupported("main as a function value"));
        }
        let ty = intern_full(
            signature.receiver,
            signature.params.clone(),
            signature.shape.clone(),
            signature.result,
            self.types,
        )?;
        Ok(Expr {
            span,
            ty,
            kind: E::FunctionRef {
                index: signature.index,
                fresh: false,
                captures: vec![],
            },
        })
    }
    pub(super) fn function_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &ast::Expr,
    ) -> Result<Expr> {
        let callee = self.expr(callee)?;
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
    pub(super) fn invoke_value(
        &mut self,
        callee: Expr,
        args: Vec<Expr>,
        span: swc_common::Span,
    ) -> Result<Expr> {
        self.invoke_receiver(callee, None, args, span)
    }
    pub(super) fn invoke_receiver(
        &mut self,
        callee: Expr,
        receiver: Option<Expr>,
        args: Vec<Expr>,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let Type::Function(index) = callee.ty else {
            return Err(type_error("called value is not a typed function"));
        };
        let signature = self.types.functions[index as usize].clone();
        let args = self.bind_arguments(args, &signature.params, &signature.shape, span)?;
        self.invoke_bound_receiver(callee, receiver, args, span)
    }
    pub(super) fn invoke_bound_receiver(
        &mut self,
        callee: Expr,
        receiver: Option<Expr>,
        mut args: Vec<Expr>,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let Type::Function(index) = callee.ty else {
            return Err(type_error("called value is not a typed function"));
        };
        let signature = self.types.functions[index as usize].clone();
        let ignored_receiver = if let Some(ty) = signature.receiver {
            let value = receiver.unwrap_or(Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            });
            let value = unions::widen(value, Some(ty), self.types)?;
            if value.ty != ty {
                return Err(self.source.locate(
                    type_error(format!(
                        "this type mismatch: expected {ty}, found {}",
                        value.ty
                    )),
                    value.span,
                ));
            }
            args.insert(0, value);
            None
        } else {
            receiver.map(Box::new)
        };
        let physical_params = signature.physical_params();
        let slot = self.hidden(callee.ty);
        let mut arguments = Vec::new();
        for (value, ty) in args.iter().zip(&physical_params) {
            if value.ty != *ty {
                return Err(self.source.locate(
                    type_error(format!(
                        "callback argument mismatch: expected {ty}, found {}",
                        value.ty
                    )),
                    value.span,
                ));
            }
            arguments.push(self.hidden(*ty));
        }
        let output = (signature.result != Type::Void).then(|| self.hidden(signature.result));
        Ok(Expr {
            span,
            ty: signature.result,
            kind: E::FunctionCall {
                callee: Box::new(callee),
                ignored_receiver,
                args,
                slot,
                arguments,
                output,
            },
        })
    }
    pub(super) fn arrow(&mut self, arrow: &ast::ArrowExpr, expected: Option<Type>) -> Result<Expr> {
        required(self.types)?;
        let asynchronous = arrow.is_async && self.types.profile.async_methods();
        if arrow.is_async && !asynchronous && self.types.profile.async_functions() {
            return Err(promises::rejected("async arrow function"));
        }
        if arrow.is_async && !asynchronous || arrow.is_generator || arrow.type_params.is_some() {
            return Err(unsupported("async/generic/generator arrow"));
        }
        let expected = match self.function_context(expected).or(expected) {
            Some(Type::Function(id)) => Some(self.types.functions[id as usize].clone()),
            // An annotated literal stored in `unknown` (v36) has no context.
            Some(Type::Unknown) if self.types.profile.function_fields() => None,
            Some(_) => return Err(type_error("arrow requires a function context")),
            None => None,
        };
        if expected.as_ref().is_some_and(|ty| ty.receiver.is_some()) {
            return Err(unsupported(
                "dynamic this context for an arrow; use a function expression",
            ));
        }
        self.arrow_context(
            arrow,
            LiftContext {
                params: expected.as_ref().map(|ty| ty.params.clone()),
                shape: expected.as_ref().map(|ty| ty.shape.clone()),
                result: expected.map(|ty| ty.result),
                infer: false,
                receiver: None,
                lexical: true,
                self_name: None,
            },
        )
    }
    pub(super) fn arrow_context(
        &mut self,
        arrow: &ast::ArrowExpr,
        context: LiftContext,
    ) -> Result<Expr> {
        let LiftContext {
            params: expected,
            shape: expected_shape,
            result: expected_result,
            infer: infer_block,
            receiver,
            lexical,
            self_name,
        } = context;
        required(self.types)?;
        // Async arrows and function expressions are split into a wrapper and
        // a scheduled body over the closure's own captures (v35).
        let asynchronous = arrow.is_async && self.types.profile.async_methods();
        if arrow.is_async && !asynchronous || arrow.is_generator || arrow.type_params.is_some() {
            return Err(unsupported("async/generic/generator arrow"));
        }
        let padded = ignored_parameters(
            arrow,
            expected.as_deref(),
            expected_shape.as_ref(),
            self.types,
        );
        let plans = parameters::analyze(
            padded.as_deref().unwrap_or(&arrow.params),
            expected.as_deref(),
            expected_shape.as_ref(),
            self.types,
            Some(self.source),
        )?;
        let params: Vec<_> = plans.iter().map(|p| p.incoming).collect();
        let shape = parameters::shape(&plans);
        let annotated = annotation(arrow.return_type.as_deref(), self.types)?;
        // V37 (`void_functions`): an annotated result in a `void` context
        // keeps its own type; the value is converted to the context after.
        let expected_result = if self.types.profile.language_core()
            && expected_result == Some(Type::Void)
            && annotated.is_some_and(|result| result != Type::Void)
        {
            None
        } else {
            expected_result
        };
        let result = annotated.or(expected_result);
        if annotated
            .is_some_and(|result| expected_result.is_some_and(|expected| expected != result))
        {
            return Err(type_error("arrow return type mismatch"));
        }
        let self_type = if let Some(name) = &self_name {
            if matches!(name.sym.as_ref(), "varta" | "console" | "log" | "JSON") {
                return Err(self.source.locate(
                    invalid("reserved named function expression binding"),
                    name.span,
                ));
            }
            let result=result.ok_or_else(||self.source.locate(invalid("named function expression requires a return annotation or function context"),name.span))?;
            Some(intern_full(
                receiver,
                params.clone(),
                shape.clone(),
                result,
                self.types,
            )?)
        } else {
            None
        };
        // An async closure's body returns the promise's `T`; without an
        // annotation or context a block body infers it from its returns.
        let promise = match result {
            Some(Type::Promise(id)) if asynchronous => Some(id),
            Some(other) if asynchronous => {
                return Err(type_error(format!(
                    "async function must return Promise<T>, not {other}"
                )));
            }
            _ => None,
        };
        let result = match promise {
            Some(id) => Some(self.types.promises[id as usize].value),
            None => result,
        };
        let block = matches!(&*arrow.body, ast::ArrowFunctionBody::FunctionBody(_));
        let infer_returns = (infer_block || asynchronous) && result.is_none() && block;
        let index = *self.next_function;
        if index >= varta_common::abi::MAX_USER_FUNCTIONS {
            return Err(invalid("too many functions including arrows"));
        }
        *self.next_function += 1;
        let mut outer = self.outer.clone();
        for scope in &self.scopes {
            outer.extend(scope.clone());
        }
        self.unassigned_outer(&mut outer);
        if !lexical {
            outer.remove("this");
        }
        let inferred = match &*arrow.body {
            ast::ArrowFunctionBody::FunctionBody(block) if contains_value_return(&block.stmts) => {
                self.types.number()
            }
            _ => Type::Void,
        };
        let outer_narrows = self.capture_narrows();
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
            outer_narrows,
            result: if infer_returns {
                Type::Void
            } else {
                result.unwrap_or(inferred)
            },
            infer_returns,
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
            outer,
            captures: Default::default(),
            cells: Default::default(),
            guarded: Default::default(),
            generator: None,
            // An unknown promise type until inference fixes it.
            asynchronous: asynchronous.then_some(promise.unwrap_or(u32::MAX)),
            core: Default::default(),
        };
        if let Some(ty) = receiver {
            child.declare("this", Some(ty), true, false)?;
            child.flow_parameter_binding(&child.binding("this")?, 0);
        }
        child.reserve_parameters(&plans)?;
        let self_slot = if let Some(name) = self_name {
            let ty = self_type.ok_or_else(|| invalid("missing self function type"))?;
            let slot = child.hidden(ty);
            let binding = Binding {
                identity: (index, slot),
                slot,
                ty: Some(ty),
                initialized: true,
                mutable: false,
                tdz: None,
                global: None,
                readonly: false,
            };
            child
                .scopes
                .insert(0, BTreeMap::from([(name.sym.to_string(), binding)]));
            Some(slot)
        } else {
            None
        };
        // The async body takes its promise right after the parameters.
        let promise_slot = asynchronous.then(|| child.hidden(Type::Void));
        let mut prologue = child.initialize_parameters(&plans, receiver.is_some())?;
        let body = match &*arrow.body {
            ast::ArrowFunctionBody::FunctionBody(block) => child.block(&block.stmts, false)?,
            ast::ArrowFunctionBody::Expr(expr) if asynchronous => {
                child.async_expression_body(expr, result)?
            }
            ast::ArrowFunctionBody::Expr(expr) => {
                let value = child.expr_expected(expr, result)?;
                // V36 (`timers`): as in TypeScript, a void context discards
                // the value of an expression body, so `(resolve) =>
                // setTimeout(resolve, ms)` is a void executor.
                let discarded = child.types.profile.timers() && value.ty != Type::Void;
                if result == Some(Type::Void) && (value.ty == Type::Undefined || discarded) {
                    vec![S::Expr(value), S::Return(None)]
                } else {
                    if result.is_some_and(|ty| ty != value.ty) {
                        return Err(type_error("arrow expression result mismatch"));
                    }
                    child.result = value.ty;
                    vec![S::Return(Some(value))]
                }
            }
        };
        prologue.extend(body);
        let mut body = prologue;
        if infer_returns {
            child.infer_callback_returns(&mut body, arrow.span, asynchronous)?;
            if let Some(Type::Promise(id)) = asynchronous
                .then(|| promises::intern(child.result, child.types))
                .transpose()?
            {
                child.asynchronous = Some(id);
            }
        }
        super::link::finish_body(&mut child, &mut body, arrow.span)?;
        let callable_result = match promise_slot {
            Some(slot) => {
                let ty = promises::intern(child.result, child.types)
                    .map_err(|error| child.source.locate(error, arrow.span))?;
                child.local_types.insert(slot, ty);
                ty
            }
            None => child.result,
        };
        let ty = intern_full(
            receiver,
            params.clone(),
            shape.clone(),
            callable_result,
            child.types,
        )?;
        let mut physical_params: Vec<_> =
            receiver.into_iter().chain(params.iter().copied()).collect();
        physical_params.extend(self_type);
        if child.types.profile.closures()
            && physical_params
                .iter()
                .copied()
                .map(Type::width)
                .sum::<u32>() as usize
                >= varta_common::abi::MAX_WASM_PARAMETERS
        {
            return Err(invalid(
                "arrow parameters plus environment exceed Wasm slot limit",
            ));
        }
        let captured: Vec<_> = child.captures.values().cloned().collect();
        let effects = child.flow_summary();
        child.call_effects.insert(index, effects);
        let function = if let Some(promise) = promise_slot {
            let source = child.source;
            child.split_async(
                promises::AsyncSplit {
                    incoming: physical_params,
                    shape,
                    receiver,
                    self_slot,
                    captures: captured.iter().map(|binding| binding.slot).collect(),
                    environment: child.types.profile.closures(),
                    promise,
                    entry: false,
                    span: arrow.span,
                    source,
                },
                std::mem::take(&mut body),
            )?
        } else {
            ir::Function {
                shape,
                self_slot,
                receiver,
                cells: std::mem::take(&mut child.cells),
                captures: captured.iter().map(|binding| binding.slot).collect(),
                environment: child.types.profile.closures(),
                generator: None,
                source: child.source.clone(),
                span: arrow.span,
                params: physical_params,
                result: child.result,
                locals: (0..child.next)
                    .map(|slot| {
                        child
                            .local_types
                            .get(&slot)
                            .copied()
                            .unwrap_or(Type::Number)
                    })
                    .collect(),
                body: std::mem::take(&mut body),
            }
        };
        child.generated.insert(index, function);
        let captures = captured
            .iter()
            .map(|binding| self.capture_binding(binding))
            .collect::<Result<Vec<_>>>()?;
        Ok(Expr {
            span: arrow.span,
            ty,
            kind: E::FunctionRef {
                index,
                fresh: true,
                captures,
            },
        })
    }
}

/// Name prefix of a synthesized ignored parameter; `#` cannot start a source
/// binding, so the name never shadows or captures user code.
const IGNORED_PARAMETER: &str = "#ignored";

/// V35 (`web_headers`): a function literal whose context is an exact function
/// type may declare fewer parameters than that type, as in TypeScript, like
/// `headers.forEach((value, name) => …)` for a three-parameter callback. The
/// missing trailing parameters are synthesized as ignored bindings, so the
/// literal has the context's exact function type. Earlier profiles keep the
/// exact parameter count.
fn ignored_parameters(
    arrow: &ast::ArrowExpr,
    expected: Option<&[Type]>,
    shape: Option<&ir::ParameterShape>,
    types: &ir::ValueTypes,
) -> Option<Vec<ast::Pat>> {
    let expected = expected?;
    // A rest parameter of the context keeps its exact shape.
    if !types.profile.web_headers()
        || shape.is_none_or(|shape| shape.rest)
        || arrow.params.len() >= expected.len()
        || arrow
            .params
            .last()
            .is_some_and(|param| matches!(param, ast::Pat::Rest(_)))
    {
        return None;
    }
    let mut params = arrow.params.clone();
    for index in arrow.params.len()..expected.len() {
        let name = format!("{IGNORED_PARAMETER}{index}");
        params.push(ast::Pat::Ident(ast::BindingIdent {
            id: ast::Ident::new(name.into(), arrow.span, Default::default()),
            type_ann: None,
        }));
    }
    Some(params)
}

impl Lower<'_> {
    /// Materialize a transitive capture once per activation; stable identities
    /// distinguish shadowed bindings even when their source names coincide.
    pub(super) fn capture_binding(&mut self, binding: &Binding) -> Result<u32> {
        if !self.types.profile.closures() {
            return Err(unsupported(
                "arrow capture of an outer binding requires checked-f64-utf16-v12",
            ));
        }
        // V37 (`language_core`): a binding the TDZ scan reserved a flag for may
        // be captured before its initialization, together with that flag.
        let early =
            !binding.initialized && binding.tdz.is_some() && self.types.profile.language_core();
        if early {
            self.capture_early(binding)?;
        } else if !binding.initialized || binding.ty.is_none() {
            return Err(unsupported("capture before binding initialization"));
        }
        let slot = if binding.identity.0 == self.index {
            binding.identity.1
        } else if let Some(captured) = self.captures.get(&binding.identity) {
            captured.slot
        } else {
            if self.captures.len() >= varta_common::function::MAX_CAPTURES {
                return Err(invalid("closure capture count exceeds limit"));
            }
            let ty = binding
                .ty
                .ok_or_else(|| invalid("missing captured binding type"))?;
            let slot = self.hidden(ty);
            self.captures.insert(
                binding.identity,
                Binding {
                    slot,
                    ..binding.clone()
                },
            );
            slot
        };
        if binding.mutable
            && self.guarded.contains(&slot)
            && !self.types.profile.refinement_writes()
        {
            return Err(unsupported("capture of a mutable guard-refined binding"));
        }
        if self.refinements.contains_key(&slot) && !self.types.profile.refinement_writes() {
            return Err(unsupported("capture of a guard-refined binding"));
        }
        self.cells.insert(slot);
        if self.types.profile.refinement_writes()
            && !binding.mutable
            && let Some(&ty) = self.outer_narrows.get(&binding.identity)
        {
            self.refinements.insert(slot, ty);
        }
        Ok(slot)
    }
    pub(super) fn resolve_binding(&mut self, name: &str) -> Result<Binding> {
        if let Some(binding) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
        {
            return Ok(binding);
        }
        if let Some(binding) = self.outer.get(name).cloned() {
            // A module variable is reached through its root slot, never
            // captured; this function keeps its own slot for guard facts.
            if binding.global.is_some() {
                let slot = self.hidden(binding.ty.ok_or_else(|| invalid("missing type"))?);
                let local = Binding {
                    identity: (self.index, slot),
                    slot,
                    ..binding
                };
                if let Some(scope) = self.scopes.first_mut() {
                    scope.insert(name.to_owned(), local.clone());
                }
                return Ok(local);
            }
            let slot = self.capture_binding(&binding)?;
            return Ok(Binding { slot, ..binding });
        }
        self.binding(name)
    }
}
