//! V38 tagged templates (`expression_forms`).
//!
//! `String.raw` joins the raw text of the template parts with the primitive
//! ToString of each substitution, in source order. Any other tag is called
//! with the cooked strings as a fresh `TemplateStringsArray` (a readonly
//! `string[]`) followed by the substitutions, exactly like the call
//! `tag(["a", "b"], x)`. The strings array's `raw` property is refused
//! (`raw_property`), as is a part whose cooked text is `undefined` (an
//! invalid escape, legal only in tagged templates).
use super::*;

/// The diagnostic of a `raw` read on a `TemplateStringsArray`.
pub(super) const RAW_REFUSED: &str =
    "TemplateStringsArray.raw outside String.raw (the strings array carries cooked text only)";

impl Lower<'_> {
    pub(super) fn tagged_template(&mut self, tagged: &ast::TaggedTpl) -> Result<Expr> {
        let template = &tagged.tpl;
        if template.quasis.len() != template.exprs.len() + 1 {
            return Err(invalid("malformed template literal"));
        }
        if tagged.type_params.is_some() {
            return Err(self.source.locate(
                unsupported("type arguments on a tagged template"),
                tagged.span,
            ));
        }
        if self.string_raw_tag(&tagged.tag) {
            return self.string_raw(template, tagged.span);
        }
        let mut strings = Vec::with_capacity(template.quasis.len());
        for quasi in &template.quasis {
            let Some(cooked) = &quasi.cooked else {
                return Err(self.source.locate(
                    unsupported("tagged template part with an invalid escape (its cooked string is undefined)"),
                    quasi.span,
                ));
            };
            strings.push(Some(ast::ExprOrSpread {
                spread: None,
                expr: Box::new(ast::Expr::Lit(ast::Lit::Str(ast::Str {
                    span: quasi.span,
                    value: cooked.clone(),
                    raw: None,
                }))),
            }));
        }
        let mut args = vec![ast::ExprOrSpread {
            spread: None,
            expr: Box::new(ast::Expr::Array(ast::ArrayLit {
                span: template.span,
                elems: strings,
            })),
        }];
        args.extend(template.exprs.iter().map(|expr| ast::ExprOrSpread {
            spread: None,
            expr: expr.clone(),
        }));
        let call = ast::Expr::Call(ast::CallExpr {
            span: tagged.span,
            ctxt: tagged.ctxt,
            callee: ast::Callee::Expr(tagged.tag.clone()),
            args,
            type_args: None,
        });
        self.expr(&call)
    }

    /// The tag is the built-in `String.raw` (not a shadowed `String`).
    fn string_raw_tag(&self, tag: &ast::Expr) -> bool {
        matches!(stmt::unparen(tag), ast::Expr::Member(member)
            if matches!(&*member.obj, ast::Expr::Ident(id) if id.sym == "String" && self.binding("String").is_err())
                && matches!(&member.prop, ast::MemberProp::Ident(name) if name.sym == "raw"))
    }

    /// `String.raw\`...\``: the raw parts (line terminators normalized to
    /// LF, as the TRV of a template does) and the ToString of each
    /// substitution, concatenated left to right.
    fn string_raw(&mut self, template: &ast::Tpl, span: swc_common::Span) -> Result<Expr> {
        let mut joined: Option<Expr> = None;
        for (index, quasi) in template.quasis.iter().enumerate() {
            if let Some(index) = index.checked_sub(1) {
                let value = self.expr(&template.exprs[index])?;
                let text = self.template_text(value)?;
                joined = Some(concatenate(joined, text));
            }
            let raw = quasi.raw.replace("\r\n", "\n").replace('\r', "\n");
            if !raw.is_empty() {
                let bytes = if self.types.profile.utf16() {
                    raw.encode_utf16().flat_map(u16::to_le_bytes).collect()
                } else {
                    raw.into_bytes()
                };
                joined = Some(concatenate(
                    joined,
                    Expr {
                        span: quasi.span,
                        ty: Type::String,
                        kind: E::String(bytes),
                    },
                ));
            }
        }
        Ok(match joined {
            Some(value) => Expr { span, ..value },
            None => Expr {
                span,
                ty: Type::String,
                kind: E::String(Vec::new()),
            },
        })
    }
}

fn concatenate(left: Option<Expr>, right: Expr) -> Expr {
    match left {
        None => right,
        Some(left) => Expr {
            span: left.span,
            ty: Type::String,
            kind: E::Binary(ast::BinaryOp::Add, Box::new(left), Box::new(right)),
        },
    }
}
