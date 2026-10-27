//! BoundNames is collected before any initializer is lowered, including nested TDZ.
use super::*;
use varta_common::function::{MAX_PARAMETER_PATTERN_DEPTH, MAX_PARAMETER_PATTERN_NODES};
type Failure = (CompilerError, swc_common::Span);
pub(in crate::lower) fn names(
    patterns: &[ast::Pat],
) -> std::result::Result<Vec<(String, swc_common::Span)>, Failure> {
    let mut result = Vec::new();
    let mut remaining = MAX_PARAMETER_PATTERN_NODES;
    for pattern in patterns {
        visit(pattern, 0, &mut remaining, &mut result)?;
    }
    Ok(result)
}
fn tick(
    span: swc_common::Span,
    depth: usize,
    remaining: &mut usize,
) -> std::result::Result<(), Failure> {
    if depth > MAX_PARAMETER_PATTERN_DEPTH || *remaining == 0 {
        return Err((invalid("parameter binding pattern budget exceeded"), span));
    }
    *remaining -= 1;
    Ok(())
}
fn visit(
    pattern: &ast::Pat,
    depth: usize,
    remaining: &mut usize,
    result: &mut Vec<(String, swc_common::Span)>,
) -> std::result::Result<(), Failure> {
    tick(pattern.span(), depth, remaining)?;
    match pattern {
        ast::Pat::Ident(id) => result.push((id.id.sym.to_string(), id.span())),
        ast::Pat::Assign(p) => visit(&p.left, depth + 1, remaining, result)?,
        ast::Pat::Rest(p) => visit(&p.arg, depth + 1, remaining, result)?,
        ast::Pat::Object(p) => {
            for (index, prop) in p.props.iter().enumerate() {
                match prop {
                    ast::ObjectPatProp::Assign(prop) => {
                        tick(prop.span, depth + 1, remaining)?;
                        result.push((prop.key.id.sym.to_string(), prop.key.span()));
                    }
                    ast::ObjectPatProp::KeyValue(prop) => {
                        key(&prop.key)?;
                        visit(&prop.value, depth + 1, remaining, result)?
                    }
                    ast::ObjectPatProp::Rest(prop) => {
                        if index + 1 != p.props.len() || !matches!(&*prop.arg, ast::Pat::Ident(_)) {
                            return Err((
                                unsupported("object rest must be a final identifier"),
                                prop.span,
                            ));
                        }
                        visit(&prop.arg, depth + 1, remaining, result)?;
                    }
                }
            }
        }
        ast::Pat::Array(p) => {
            for (index, entry) in p.elems.iter().enumerate() {
                if let Some(entry) = entry {
                    if matches!(entry, ast::Pat::Rest(_)) && index + 1 != p.elems.len() {
                        return Err((unsupported("array rest must be final"), entry.span()));
                    }
                    visit(entry, depth + 1, remaining, result)?;
                } else {
                    tick(p.span, depth + 1, remaining)?;
                }
            }
        }
        _ => return Err((unsupported("parameter binding pattern"), pattern.span())),
    }
    Ok(())
}
