//! V38 tagged templates (`expression_forms`).
//!
//! `String.raw` joins the raw text of the template parts with the primitive
//! ToString of each substitution, in source order. Any other tag is called
//! with the site's template object followed by the substitutions, exactly
//! like the call `tag(strings, x)`.
//!
//! The template object (GetTemplateObject, §13.2.8.4) is one array per site:
//! every evaluation of a site, in every call and every instantiation of a
//! generic function, passes the same `TemplateStringsArray`, whose `raw`
//! property is the same array of raw strings. `TemplateStringsArray` is a
//! `string[]` array type of its own: a site's cooked and raw arrays are built
//! once per instance by a synthesized initializer into two permanent root
//! slots, and `strings.raw` finds the raw array of the site whose cooked
//! array it is (a value of the type comes only from a site). Every other use
//! reads the strings array as the `string[]` it is. A part whose cooked text
//! is `undefined` (an invalid escape, legal only in tagged templates) is
//! refused.
use super::*;

/// Module-constant indices of template objects while bodies are lowered:
/// site `k` takes `BASE + 2k` (cooked) and `BASE + 2k + 1` (raw); `finish`
/// moves them behind the program's module constants.
const BASE: u32 = 1 << 30;

/// The encoded strings of a template's parts.
type Parts = Vec<Vec<u8>>;

/// The template objects of a program (v38 `expression_forms`).
#[derive(Default)]
pub(crate) struct TemplateObjects {
    /// The `TemplateStringsArray` array type: a `string[]` of its own.
    array: Option<u32>,
    /// Site number by (module, span start, span end).
    sites: BTreeMap<(String, u32, u32), u32>,
    /// Cooked and raw strings of each site, in site order.
    parts: Vec<(Parts, Parts)>,
    /// The reserved `(strings) -> string[]` function of `strings.raw` and
    /// the `TypeError` it throws for a strings array no site created.
    raw: Option<(usize, Expr)>,
    /// The source and span of the first site, for synthesized functions.
    origin: Option<(Source, swc_common::Span)>,
}

/// The `TemplateStringsArray` type. `string[]` is interned first, so array
/// interning keeps returning the ordinary `string[]` for `string` elements.
pub(super) fn array(types: &mut ir::ValueTypes) -> Result<Type> {
    if let Some(id) = types.templates.array {
        return Ok(Type::Array(id));
    }
    arrays::intern(Type::String, types)?;
    if types.arrays.len() >= types.profile.limits().array_types {
        return Err(invalid("array type count exceeds limit"));
    }
    let id = types.arrays.len() as u32;
    types.arrays.push(Type::String);
    types.templates.array = Some(id);
    Ok(Type::Array(id))
}

/// Whether `ty` is the `TemplateStringsArray` type.
pub(super) fn is_strings(ty: Type, types: &ir::ValueTypes) -> bool {
    matches!(ty, Type::Array(id) if types.templates.array == Some(id))
}

/// A `TemplateStringsArray` value read as the `string[]` it is (the same
/// array); any other value unchanged.
pub(super) fn plain(value: Expr, types: &ir::ValueTypes) -> Expr {
    if !is_strings(value.ty, types) {
        return value;
    }
    match types.arrays.iter().position(|ty| *ty == Type::String) {
        Some(id) => Expr {
            ty: Type::Array(id as u32),
            ..value
        },
        None => value,
    }
}

/// Operands of a binary operator: a tagged template's strings array is the
/// `string[]` it is when compared with another array type.
pub(super) fn operands(left: Expr, right: Expr, types: &ir::ValueTypes) -> (Expr, Expr) {
    if left.ty == right.ty {
        return (left, right);
    }
    (plain(left, types), plain(right, types))
}

/// A `TemplateStringsArray` is assignable to `string[]` (TypeScript: to
/// `readonly string[]`; Varta arrays carry no readonly type).
pub(super) fn widens(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    is_strings(actual, types)
        && matches!(expected, Type::Array(id) if types.arrays[id as usize] == Type::String)
}

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
        let site = self.template_object(template)?;
        let ty = site.ty;
        let name = format!("#template{}", self.next);
        self.declare(&name, Some(ty), true, false)?;
        let binding = self.binding(&name)?;
        let mut args = vec![ast::ExprOrSpread {
            spread: None,
            expr: Box::new(ast::Expr::Ident(ast::Ident::new(
                name.as_str().into(),
                template.span,
                Default::default(),
            ))),
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
        let call = self.expr(&call)?;
        let span = tagged.span;
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        let fetched = Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body: vec![
                    S::Init(binding.slot, site),
                    S::Init(
                        output,
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::Integer(1),
                        },
                    ),
                ],
            },
        };
        Ok(Expr {
            span,
            ty: call.ty,
            kind: E::Sequence(Box::new(fetched), Box::new(call)),
        })
    }

    /// The template object of a site (GetTemplateObject): the site's
    /// permanent cooked strings array.
    fn template_object(&mut self, template: &ast::Tpl) -> Result<Expr> {
        let ty = array(self.types)?;
        let key = (
            self.source.file.name.to_string(),
            template.span.lo.0,
            template.span.hi.0,
        );
        let site = match self.types.templates.sites.get(&key) {
            Some(site) => *site,
            None => {
                let mut cooked = Vec::with_capacity(template.quasis.len());
                let mut raw = Vec::with_capacity(template.quasis.len());
                for quasi in &template.quasis {
                    let Some(text) = &quasi.cooked else {
                        return Err(self.source.locate(
                            unsupported("tagged template part with an invalid escape (its cooked string is undefined)"),
                            quasi.span,
                        ));
                    };
                    let literal = self.expr(&ast::Expr::Lit(ast::Lit::Str(ast::Str {
                        span: quasi.span,
                        value: text.clone(),
                        raw: None,
                    })))?;
                    let E::String(bytes) = literal.kind else {
                        return Err(CompilerError::Internal(
                            "template part is not a string constant".into(),
                        ));
                    };
                    cooked.push(bytes);
                    raw.push(self.types.profile.string_bytes(&raw_text(&quasi.raw)));
                }
                let templates = &mut self.types.templates;
                let site = templates.parts.len() as u32;
                templates.parts.push((cooked, raw));
                templates.sites.insert(key, site);
                templates
                    .origin
                    .get_or_insert_with(|| (self.source.clone(), template.span));
                site
            }
        };
        Ok(Expr {
            span: template.span,
            ty,
            kind: E::ModuleConstant(BASE + 2 * site),
        })
    }

    /// `strings.raw` of a `TemplateStringsArray`: the raw strings array of
    /// the site that created `strings`.
    pub(super) fn template_raw(&mut self, strings: Expr, span: swc_common::Span) -> Result<Expr> {
        let result = arrays::intern(Type::String, self.types)?;
        let index = match &self.types.templates.raw {
            Some((index, _)) => *index,
            None => {
                let index = *self.next_function;
                if index >= self.types.function_limit() {
                    return Err(invalid("too many functions including template objects"));
                }
                *self.next_function += 1;
                let error = self.array_error(
                    varta_common::value::ErrorClass::TypeError,
                    "TemplateStringsArray value was not created by a tagged template",
                    span,
                )?;
                let ty = array(self.types)?;
                // The body is written by `finish`, once every site is known.
                self.generated.insert(
                    index,
                    ir::Function {
                        shape: ir::ParameterShape::fixed(1),
                        self_slot: None,
                        receiver: None,
                        cells: Default::default(),
                        captures: vec![],
                        environment: false,
                        generator: None,
                        source: self.source.clone(),
                        span,
                        params: vec![ty],
                        result,
                        locals: vec![ty],
                        body: vec![S::Trap],
                    },
                );
                self.types.templates.raw = Some((index, error));
                index
            }
        };
        Ok(Expr {
            span,
            ty: result,
            kind: E::Call(index, vec![strings]),
        })
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
            let raw = raw_text(&quasi.raw);
            if !raw.is_empty() {
                joined = Some(concatenate(
                    joined,
                    Expr {
                        span: quasi.span,
                        ty: Type::String,
                        kind: E::String(self.types.profile.string_bytes(&raw)),
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

/// The TRV of a template part: its source text with line terminators
/// normalized to LF.
fn raw_text(raw: &str) -> String {
    raw.replace("\r\n", "\n").replace('\r', "\n")
}

/// After every body is lowered: move the template slots behind the
/// `stored` module constants, write the `strings.raw` lookup and return the
/// initializer that builds every site's arrays, to run before the module
/// initializers.
pub(super) fn finish(
    types: &mut ir::ValueTypes,
    functions: &mut [ir::Function],
    generated: &mut BTreeMap<usize, ir::Function>,
    next: &mut usize,
    stored: &mut u32,
) -> Result<Option<usize>> {
    let Some((source, span)) = types.templates.origin.clone() else {
        return Ok(None);
    };
    let base = *stored;
    let sites = types.templates.parts.len() as u32;
    let moved = |index: &mut u32| {
        if *index >= BASE {
            *index = *index - BASE + base;
        }
    };
    for function in functions.iter_mut().chain(generated.values_mut()) {
        prune::each_constant(&mut function.body, &mut |index| moved(index));
    }
    *stored += 2 * sites;
    let strings = array(types)?;
    let plain = arrays::intern(Type::String, types)?;
    let constant = |index: u32, ty: Type| Expr {
        span,
        ty,
        kind: E::ModuleConstant(index),
    };
    if let Some((index, error)) = types.templates.raw.clone() {
        let mut body: Vec<S> = (0..sites)
            .map(|site| {
                S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Binary(
                            ast::BinaryOp::EqEqEq,
                            Box::new(Expr {
                                span,
                                ty: strings,
                                kind: E::Local(0),
                            }),
                            Box::new(constant(base + 2 * site, strings)),
                        ),
                    },
                    vec![S::Return(Some(constant(base + 2 * site + 1, plain)))],
                    vec![],
                )
            })
            .collect();
        body.push(S::Throw(error));
        generated
            .get_mut(&index)
            .ok_or_else(|| CompilerError::Internal("missing template raw lookup".into()))?
            .body = body;
    }
    let index = *next;
    if index >= types.function_limit() {
        return Err(source.locate(
            invalid("too many functions including module initializers"),
            span,
        ));
    }
    *next += 1;
    let mut body = Vec::with_capacity(2 * sites as usize);
    for (site, (cooked, raw)) in types.templates.parts.iter().enumerate() {
        let site = site as u32;
        for (slot, parts, ty) in [(0, cooked, strings), (1, raw, plain)] {
            let elements = parts
                .iter()
                .map(|bytes| {
                    Some(Expr {
                        span,
                        ty: Type::String,
                        kind: E::String(bytes.clone()),
                    })
                })
                .collect();
            body.push(S::InitModuleConstant(
                base + 2 * site + slot,
                Expr {
                    span,
                    ty,
                    kind: E::Array(elements),
                },
            ));
        }
    }
    generated.insert(
        index,
        ir::Function {
            shape: ir::ParameterShape::fixed(0),
            self_slot: None,
            receiver: None,
            cells: Default::default(),
            captures: vec![],
            environment: false,
            generator: None,
            source,
            span,
            params: vec![],
            result: Type::Void,
            locals: vec![],
            body,
        },
    );
    Ok(Some(index))
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
