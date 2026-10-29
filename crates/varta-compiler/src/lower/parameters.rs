//! Separate call ABI parameters from left-to-right initialized source bindings.
use super::*;

fn locate(source: Option<&Source>, error: CompilerError, span: swc_common::Span) -> CompilerError {
    if let Some(source) = source {
        source.locate(error, span)
    } else {
        error
    }
}
pub(super) struct Parameter {
    pub name: Option<String>,
    pub binding: ast::Pat,
    pub body: Type,
    pub incoming: Type,
    pub optional: bool,
    pub rest: bool,
    pub default: Option<Box<ast::Expr>>,
    pub span: swc_common::Span,
}
pub(super) fn shape(plans: &[Parameter]) -> ir::ParameterShape {
    ir::ParameterShape {
        optional: plans.iter().map(|p| p.optional).collect(),
        rest: plans.last().is_some_and(|p| p.rest),
    }
}
fn needs_initialization(plans: &[Parameter]) -> bool {
    shape(plans).extended()
        || plans
            .iter()
            .any(|p| p.default.is_some() || p.name.is_none())
}
fn literal(expr: &ast::Expr, types: &ir::ValueTypes) -> Option<Type> {
    match expr {
        ast::Expr::Paren(p) => literal(&p.expr, types),
        ast::Expr::Lit(ast::Lit::Num(_)) => Some(types.number()),
        ast::Expr::Lit(ast::Lit::Str(_)) => Some(Type::String),
        ast::Expr::Tpl(_) if types.profile.template_literals() => Some(Type::String),
        ast::Expr::Lit(ast::Lit::Bool(_)) => Some(Type::Boolean),
        ast::Expr::Lit(ast::Lit::Null(_)) => Some(Type::Null),
        ast::Expr::Unary(u)
            if u.op == ast::UnaryOp::Minus && literal(&u.arg, types) == Some(types.number()) =>
        {
            Some(types.number())
        }
        _ => None,
    }
}
pub(super) fn analyze(
    patterns: &[ast::Pat],
    context: Option<&[Type]>,
    expected: Option<&ir::ParameterShape>,
    types: &mut ir::ValueTypes,
    source: Option<&Source>,
) -> Result<Vec<Parameter>> {
    if expected.is_some() && context.is_some_and(|p| p.len() != patterns.len()) {
        return Err(type_error("function parameter count mismatch"));
    }
    super::patterns::names(patterns).map_err(|(e, span)| locate(source, e, span))?;
    let mut result = Vec::new();
    let mut seen_optional = false;
    for (index, pattern) in patterns.iter().enumerate() {
        let (binding, default, rest, rest_annotation) = match pattern {
            ast::Pat::Assign(assign) => (&*assign.left, Some(assign.right.clone()), false, None),
            ast::Pat::Rest(rest) => (&*rest.arg, None, true, rest.type_ann.as_deref()),
            binding => (binding, None, false, None),
        };
        let (name, question, binding_annotation) = match binding {
            ast::Pat::Ident(id) => (
                Some(id.id.sym.to_string()),
                id.optional,
                id.type_ann.as_deref(),
            ),
            ast::Pat::Object(object) => (None, object.optional, object.type_ann.as_deref()),
            ast::Pat::Array(array) => (None, array.optional, array.type_ann.as_deref()),
            _ => {
                return Err(locate(
                    source,
                    unsupported("function parameter pattern"),
                    binding.span(),
                ));
            }
        };
        if name.is_none() && !types.profile.closures() {
            return Err(locate(
                source,
                unsupported("parameter destructuring requires checked-f64-utf16-v12"),
                binding.span(),
            ));
        }
        let ann = rest_annotation.or(binding_annotation);
        let at = pattern.span();
        let annotation_span = ann.map_or(at, |annotation| annotation.type_ann.span());
        let explicit = annotation(ann, types).map_err(|e| locate(source, e, annotation_span))?;
        let mut contextual = context.and_then(|p| p.get(index)).copied();
        if rest && !expected.is_some_and(|s| s.rest && index == s.fixed_count()) {
            contextual = None;
        }
        if default.is_some()
            && explicit.is_none()
            && let Some(ty) = contextual
            && ty.is_union()
        {
            let members: Vec<_> = unions::members(ty, types)
                .into_iter()
                .filter(|t| *t != Type::Undefined)
                .collect();
            if !members.is_empty() {
                contextual = Some(unions::intern(members, types)?);
            }
        }
        let inherited_optional = explicit.is_none()
            && default.is_none()
            && !rest
            && expected.is_some_and(|s| s.optional[index]);
        let optional = question || default.is_some() || inherited_optional;
        if name.as_deref() == Some("this")
            || rest && (index + 1 != patterns.len() || question)
            || question && (default.is_some() || name.is_none())
        {
            return Err(locate(
                source,
                unsupported("invalid this/optional/rest parameter position"),
                at,
            ));
        }
        if (optional || rest) && !types.profile.closures() {
            return Err(locate(
                source,
                unsupported("optional/default/rest parameters require checked-f64-utf16-v12"),
                at,
            ));
        }
        if !optional && !rest && seen_optional {
            return Err(locate(
                source,
                invalid("required parameter follows an optional parameter"),
                at,
            ));
        }
        seen_optional |= question;
        let mut body = explicit
            .or(contextual)
            .or_else(|| default.as_deref().and_then(|e| literal(e, types)))
            .ok_or_else(|| {
                locate(
                    source,
                    invalid(
                        "parameter needs an annotation, function context or scalar literal default",
                    ),
                    at,
                )
            })?;
        value_type(body).map_err(|e| locate(source, e, at))?;
        if optional && default.is_none() {
            body = unions::intern(vec![body, Type::Undefined], types)
                .map_err(|e| locate(source, e, at))?;
        }
        if name.is_none() {
            let present: Vec<_> = unions::members(body, types)
                .into_iter()
                .filter(|t| !matches!(t, Type::Null | Type::Undefined))
                .collect();
            let compatible = matches!(present.as_slice(), [Type::Record(_)])
                && matches!(binding, ast::Pat::Object(_))
                || matches!(present.as_slice(), [Type::Array(_)])
                    && matches!(binding, ast::Pat::Array(_));
            if !compatible {
                return Err(locate(
                    source,
                    type_error("parameter pattern does not match its checked aggregate type"),
                    at,
                ));
            }
        }
        if rest && !matches!(body, Type::Array(_)) {
            return Err(locate(
                source,
                type_error("rest parameter requires a checked array type"),
                at,
            ));
        }
        let incoming = if optional {
            unions::intern(vec![body, Type::Undefined], types).map_err(|e| locate(source, e, at))?
        } else {
            body
        };
        if expected.is_some()
            && let Some(expected) = context.and_then(|p| p.get(index)).copied()
            && expected != incoming
        {
            return Err(locate(
                source,
                type_error(format!(
                    "function parameter mismatch: expected {expected}, found {incoming}"
                )),
                at,
            ));
        } else if expected.is_none()
            && !optional
            && !rest
            && let Some(expected) = contextual
            && expected != body
        {
            return Err(locate(
                source,
                type_error(format!(
                    "arrow parameter type mismatch: expected {expected}, found {body}"
                )),
                at,
            ));
        }
        result.push(Parameter {
            name,
            binding: binding.clone(),
            body,
            incoming,
            optional,
            rest,
            default,
            span: at,
        });
    }
    if let Some(last_required) = result.iter().rposition(|p| !p.optional && !p.rest) {
        for plan in result.iter_mut().take(last_required) {
            if plan.default.is_some() {
                plan.optional = false;
            }
        }
    }
    if let Some(expected) = expected
        && *expected != shape(&result)
    {
        return Err(type_error(
            "function optional/rest mode does not match context",
        ));
    }
    Ok(result)
}
impl Lower<'_> {
    /// Reserve all ABI slots before source bindings/private self. Extended source
    /// bindings stay uninitialized until the prologue reaches their position.
    pub(super) fn reserve_parameters(&mut self, plans: &[Parameter]) -> Result<()> {
        let first = self.next;
        if needs_initialization(plans) {
            for plan in plans {
                self.hidden(plan.incoming);
            }
        } else {
            for plan in plans {
                self.declare(
                    plan.name
                        .as_deref()
                        .ok_or_else(|| invalid("missing parameter name"))?,
                    Some(plan.body),
                    true,
                    true,
                )
                .map_err(|e| self.source.locate(e, plan.span))?;
            }
        }
        for (index, plan) in plans.iter().enumerate() {
            self.flow_parameter_slot(first + index as u32, plan.incoming);
        }
        Ok(())
    }
    pub(super) fn initialize_parameters(
        &mut self,
        plans: &[Parameter],
        receiver: bool,
    ) -> Result<Vec<S>> {
        if !needs_initialization(plans) {
            return Ok(vec![]);
        }
        let patterns: Vec<_> = plans.iter().map(|p| p.binding.clone()).collect();
        for (name, span) in
            super::patterns::names(&patterns).map_err(|(e, span)| self.source.locate(e, span))?
        {
            self.declare(&name, None, false, true)
                .map_err(|e| self.source.locate(e, span))?;
        }
        let mut body = Vec::new();
        let first = usize::from(receiver);
        for (index, plan) in plans.iter().enumerate() {
            let incoming = Expr {
                span: plan.span,
                ty: plan.incoming,
                kind: E::Local((first + index) as u32),
            };
            body.extend(self.initialize_binding_pattern(
                &plan.binding,
                incoming,
                plan.default.as_deref(),
                Some(plan.body),
            )?);
            // A future optional/default Function may replace an Undefined
            // input; only a definite incoming Function keeps argument identity.
            if matches!(plan.incoming, Type::Function(_))
                && let Some(name) = &plan.name
            {
                self.flow_parameter_binding(&self.binding(name)?, (first + index) as u32);
            }
        }
        Ok(vec![S::Scope(
            (first..first + plans.len()).map(|s| s as u32).collect(),
            body,
        )])
    }
}
