use super::*;

pub(super) fn check(declaration: &ast::FnDecl, index: usize, source: &Source) -> Result<Signature> {
    let function = &declaration.function;
    if function.is_async
        || function.is_generator
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
    let params: Vec<_> = function
        .params
        .iter()
        .map(|param| parameter(param, source).map_err(|e| source.locate(e, param.span())))
        .collect::<Result<_>>()?;
    if params.iter().copied().map(Type::width).sum::<u32>() as usize
        > varta_common::abi::MAX_WASM_PARAMETERS
    {
        return Err(invalid("function parameters exceed 32 Wasm value slots"));
    }
    let result_span = function
        .return_type
        .as_ref()
        .map_or(declaration.ident.span, |a| a.type_ann.span());
    let result = annotation(function.return_type.as_deref())
        .map_err(|e| source.locate(e, result_span))?
        .unwrap_or_else(|| {
            if contains_value_return(&body.stmts) {
                Type::Number
            } else {
                Type::Void
            }
        });
    Ok(Signature {
        index,
        names: function
            .params
            .iter()
            .map(|param| {
                // parameter() above has already rejected every other pattern.
                match &param.pat {
                    ast::Pat::Ident(binding) => binding.id.sym.to_string(),
                    _ => unreachable!(),
                }
            })
            .collect(),
        params,
        result,
    })
}

fn parameter(param: &ast::Param, source: &Source) -> Result<Type> {
    if !param.decorators.is_empty() {
        return Err(unsupported("parameter decorator"));
    }
    let ast::Pat::Ident(binding) = &param.pat else {
        return Err(unsupported("parameter pattern"));
    };
    if binding.id.optional {
        return Err(unsupported("optional parameter"));
    }
    let span = binding
        .type_ann
        .as_ref()
        .map_or(binding.span(), |a| a.type_ann.span());
    let ty = annotation(binding.type_ann.as_deref())
        .map_err(|e| source.locate(e, span))?
        .ok_or_else(|| invalid("parameters require primitive or VartaResult annotations"))?;
    value_type(ty).map_err(|e| source.locate(e, span))?;
    Ok(ty)
}
