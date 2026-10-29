use super::*;

pub(super) fn check(
    declaration: &ast::FnDecl,
    index: usize,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<Signature> {
    let function = &declaration.function;
    let receiver = if let Some(parameter) = &function.this_param {
        if !types.profile.closures() {
            return Err(source.locate(
                unsupported("declaration this requires checked-f64-utf16-v12"),
                parameter.span,
            ));
        }
        Some(
            function_expressions::receiver_type(parameter.type_ann.as_deref(), types)
                .map_err(|e| source.locate(e, parameter.span))?,
        )
    } else {
        None
    };
    let generator = function.is_generator && types.profile.generators();
    if generator && function.is_async {
        return Err(unsupported("async generator function"));
    }
    let asynchronous = function.is_async && types.profile.async_functions();
    if function.is_async && !asynchronous
        || function.is_generator && !generator
        || function.type_params.is_some()
        || !function.decorators.is_empty()
    {
        return Err(unsupported(
            "async, generator, generic or decorated function",
        ));
    }
    let body = function
        .body
        .as_ref()
        .ok_or_else(|| invalid("function body is required"))?;
    for parameter in &function.params {
        if !parameter.decorators.is_empty() {
            return Err(unsupported("parameter decorator"));
        }
    }
    let patterns: Vec<_> = function.params.iter().map(|p| p.pat.clone()).collect();
    let plans = parameters::analyze(&patterns, None, None, types, Some(source))?;
    let shape = parameters::shape(&plans);
    let params: Vec<_> = plans.iter().map(|p| p.incoming).collect();
    if receiver
        .into_iter()
        .chain(params.iter().copied())
        .map(Type::width)
        .sum::<u32>() as usize
        > varta_common::abi::MAX_WASM_PARAMETERS
    {
        return Err(invalid("function parameters exceed 32 Wasm value slots"));
    }
    let result_span = function
        .return_type
        .as_ref()
        .map_or(declaration.ident.span, |a| a.type_ann.span());
    if asynchronous && receiver.is_some() {
        return Err(source.locate(
            promises::rejected("async function with a this parameter"),
            declaration.ident.span,
        ));
    }
    if asynchronous && shape.extended() {
        return Err(source.locate(
            promises::rejected("async function with optional or rest parameters"),
            declaration.ident.span,
        ));
    }
    if asynchronous && function.return_type.is_none() {
        return Err(source.locate(
            type_error("async function requires a Promise<T> return annotation"),
            result_span,
        ));
    }
    if generator && function.return_type.is_none() {
        return Err(source.locate(
            type_error(
                "generator function requires a Generator<T, TReturn, TNext> return annotation",
            ),
            result_span,
        ));
    }
    let result = annotation(function.return_type.as_deref(), types)
        .map_err(|e| source.locate(e, result_span))?
        .unwrap_or_else(|| {
            if contains_value_return(&body.stmts) {
                types.number()
            } else {
                Type::Void
            }
        });
    if generator && !matches!(result, Type::Generator(_)) {
        return Err(source.locate(
            type_error(format!(
                "generator function must return Generator<T, TReturn, TNext>, not {result}"
            )),
            result_span,
        ));
    }
    if asynchronous && !matches!(result, Type::Promise(_)) {
        return Err(source.locate(
            type_error(format!(
                "async function must return Promise<T>, not {result}"
            )),
            result_span,
        ));
    }
    if generator && receiver.is_some() {
        return Err(unsupported("generator function with a this parameter"));
    }
    Ok(Signature {
        receiver,
        index,
        shape,
        names: plans.iter().map(|p| p.name.clone()).collect(),
        params,
        result,
    })
}
