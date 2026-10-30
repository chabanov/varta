//! Generic method signatures in object types (v38 `structural_subtyping`).
//!
//! `m<T = D>(params): R` in an interface or object type is a function field
//! of its *erased* signature: every type parameter is `unknown` there, and
//! the field's function type carries a marker naming the declaration. An
//! object literal method with type parameters implementing such a member is
//! lowered once with its parameters as `unknown`. A call `x.m<A>(args)`
//! instantiates the declaration (explicit arguments, then defaults, then
//! `unknown`), checks the arguments against the instantiated parameters,
//! passes them as `unknown`, and converts the `unknown` result back to the
//! instantiated result with a checked assertion (`TypeError` on mismatch).
//! A type parameter may only stand for a whole parameter or the whole
//! result type, where the erased type is `unknown`.
use super::*;
use std::rc::Rc;

/// One generic method signature declaration.
pub(crate) struct ErasedMethod {
    pub name: String,
    params: Rc<ast::TsTypeParamDecl>,
    function: Rc<ast::TsFnType>,
    module: usize,
    source: Option<Source>,
}

#[derive(Default)]
pub(crate) struct ErasedMethods {
    list: Vec<Rc<ErasedMethod>>,
}

/// A frame binding every type parameter of `params` to `unknown`, over the
/// visible frame (a generic declaration's own parameters stay visible).
fn erased_frame(params: &ast::TsTypeParamDecl, types: &ir::ValueTypes) -> generics::Frame {
    let mut frame = generics::visible(types);
    for param in &params.params {
        frame.insert(param.name.sym.to_string(), Type::Unknown);
    }
    frame
}

/// The erased, marked function type of a generic method signature `m<…>(…): R`
/// declared in the current module.
pub(super) fn signature(
    name: &str,
    method: &ast::TsMethodSignature,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let params = method
        .type_params
        .clone()
        .ok_or_else(|| CompilerError::Internal("erased method without type parameters".into()))?;
    let result = method.type_ann.clone().ok_or_else(|| {
        invalid(format!(
            "method signature {name} requires a return type annotation"
        ))
    })?;
    let function = ast::TsFnType {
        span: method.span,
        params: method.params.clone(),
        type_params: None,
        type_ann: result,
    };
    for param in &params.params {
        if param.is_in || param.is_out || param.is_const {
            return Err(unsupported("in/out/const type parameter modifier"));
        }
    }
    let frame = erased_frame(&params, types);
    let erased = generics::with_frame(types, frame, |t| functions::type_annotation(&function, t))?;
    let Type::Function(id) = erased else {
        return Err(CompilerError::Internal("erased method type".into()));
    };
    let index = types.erased.list.len() as u32;
    types.erased.list.push(Rc::new(ErasedMethod {
        name: name.to_owned(),
        params: Rc::new(*params),
        function: Rc::new(function),
        module: named::current(types),
        source: named::current_source(types),
    }));
    let plain = types.functions[id as usize].clone();
    functions::intern_marked(
        plain.receiver,
        plain.params,
        plain.shape,
        plain.result,
        Some(index),
        types,
    )
}

impl Lower<'_> {
    /// The marked erased function type `expected` of a generic object type
    /// member, when the context is one.
    pub(super) fn erased_context(&self, expected: Option<Type>) -> Option<(Type, u32)> {
        match expected? {
            Type::Function(id) => self.types.functions[id as usize]
                .generic
                .map(|generic| (Type::Function(id), generic)),
            _ => None,
        }
    }
    /// An object literal method with type parameters implementing the generic
    /// member `expected` (v38): its body is lowered once with every type
    /// parameter as `unknown`, and the value takes the member's marked type.
    pub(super) fn erased_method(
        &mut self,
        method: &ast::MethodProp,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let Some((target, _)) = self.erased_context(expected) else {
            return Err(self.source.locate(
                unsupported(
                    "generic object literal method outside a generic method signature of its \
                     object type",
                ),
                method.function.span,
            ));
        };
        let params =
            method.function.type_params.as_deref().ok_or_else(|| {
                CompilerError::Internal("generic method without parameters".into())
            })?;
        let frame = erased_frame(params, self.types);
        let mut function = method.function.clone();
        function.type_params = None;
        let expression = ast::Expr::Fn(ast::FnExpr {
            ident: None,
            function,
        });
        let plain = {
            let id = match target {
                Type::Function(id) => id,
                _ => return Err(CompilerError::Internal("erased method context".into())),
            };
            let signature = self.types.functions[id as usize].clone();
            functions::intern_full(
                signature.receiver,
                signature.params,
                signature.shape,
                signature.result,
                self.types,
            )?
        };
        generics::push_frame(self.types, frame);
        let value = self.expr_expected(&expression, Some(plain));
        generics::pop_frame(self.types);
        let value = value?;
        if value.ty != plain {
            let (wanted, found) = self.mismatch(plain, value.ty);
            return Err(self.source.locate(
                type_error(format!(
                    "generic method type mismatch: expected {wanted}, found {found}"
                )),
                method.function.span,
            ));
        }
        // Erasure merges the marked type with its plain twin.
        Ok(Expr {
            span: value.span,
            ty: target,
            kind: E::Reinterpret(Box::new(value)),
        })
    }

    /// `x.m<A>(args)` where the field `m` has the erased type of a generic
    /// method signature: instantiate, call the erased function value and
    /// check the result back to the instantiated type.
    pub(super) fn erased_call(
        &mut self,
        call: &ast::CallExpr,
        callee: Expr,
        generic: u32,
    ) -> Result<Expr> {
        let span = call.span;
        let Type::Function(erased_id) = callee.ty else {
            return Err(CompilerError::Internal(
                "erased call of a non-function".into(),
            ));
        };
        let declaration = self.types.erased.list[generic as usize].clone();
        let explicit = match &call.type_args {
            Some(arguments) => generics::arguments(arguments, self.types)
                .map_err(|e| self.source.locate(e, arguments.span))?,
            None => Vec::new(),
        };
        let mut supplied: Vec<Option<Type>> = explicit.into_iter().map(Some).collect();
        // A parameter without an argument or a default is `unknown`, which
        // TypeScript also infers when no argument determines it.
        while supplied.len() < declaration.params.params.len() {
            let position = supplied.len();
            let param = &declaration.params.params[position];
            supplied.push(param.default.is_none().then_some(Type::Unknown));
        }
        let what = format!("method {}", declaration.name);
        let current = named::current(self.types);
        named::enter(self.types, declaration.module);
        let instantiated = generics::bind(
            &declaration.params,
            &supplied,
            &what,
            generics::Missing::Inference,
            declaration.source.as_ref().unwrap_or(self.source),
            self.types,
        )
        .and_then(|frame| {
            generics::with_frame(self.types, frame, |t| {
                functions::type_annotation(&declaration.function, t)
            })
        });
        named::enter(self.types, current);
        let instantiated = instantiated.map_err(|e| self.source.locate(e, span))?;
        let Type::Function(instance_id) = instantiated else {
            return Err(CompilerError::Internal("instantiated method type".into()));
        };
        let instance = self.types.functions[instance_id as usize].clone();
        let erased = self.types.functions[erased_id as usize].clone();
        let args = self.call_arguments(&call.args, &instance.params, &instance.shape)?;
        let args = self.bind_arguments(args, &instance.params, &instance.shape, span)?;
        let mut passed = Vec::new();
        for (value, ty) in args.into_iter().zip(&erased.params) {
            let value = if value.ty == *ty {
                value
            } else if *ty == Type::Unknown {
                unknown::widen(value, self.types)?
            } else {
                return Err(self.source.locate(
                    unsupported(&format!(
                        "type parameter of {what} inside a parameter type; it may only be a \
                         whole parameter or result type"
                    )),
                    value.span,
                ));
            };
            passed.push(value);
        }
        let result = self.invoke_bound_receiver(callee, None, passed, span)?;
        if result.ty == instance.result {
            return Ok(result);
        }
        if result.ty != Type::Unknown {
            return Err(self.source.locate(
                unsupported(&format!(
                    "type parameter of {what} inside its result type; it may only be a whole \
                     parameter or result type"
                )),
                span,
            ));
        }
        let name = subtyping::describe(instance.result, self.types);
        let message = format!("result of {} is not a {name}", declaration.name);
        self.assert_value(result, instance.result, &name, &message, span)
    }
}
