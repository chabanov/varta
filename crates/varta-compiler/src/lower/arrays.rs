use super::*;
use varta_common::value::{MAX_ARRAY_ELEMENTS, MAX_ARRAY_TYPES};

pub(super) fn intern(element: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    if element.value_kind().is_none() {
        return Err(unsupported("array element type"));
    }
    if let Some(index) = types.arrays.iter().position(|ty| *ty == element) {
        return Ok(Type::Array(index as u32));
    }
    if types.arrays.len() >= MAX_ARRAY_TYPES {
        return Err(invalid("array type count exceeds limit"));
    }
    let index = types.arrays.len() as u32;
    types.arrays.push(element);
    Ok(Type::Array(index))
}

impl Lower<'_> {
    pub(super) fn array(&mut self, array: &ast::ArrayLit, expected: Option<Type>) -> Result<Expr> {
        if array.elems.len() > MAX_ARRAY_ELEMENTS {
            return Err(invalid("array element count exceeds limit"));
        }
        let mut element = match expected {
            Some(Type::Array(index)) => Some(self.types.arrays[index as usize]),
            _ => None,
        };
        let mut values = Vec::new();
        for entry in &array.elems {
            let Some(entry) = entry.as_ref() else {
                if !self.types.profile.sparse_arrays() {
                    return Err(unsupported(
                        "sparse array literal requires checked-f64-utf16-v13",
                    ));
                }
                values.push(None);
                continue;
            };
            if entry.spread.is_some() {
                return Err(unsupported("array spread"));
            }
            let value = self.expr_expected(&entry.expr, element)?;
            if element.is_some_and(|ty| ty != value.ty) {
                return Err(self.source.locate(
                    type_error(format!(
                        "array element type mismatch: expected {}, found {}",
                        element.unwrap_or(Type::Void),
                        value.ty
                    )),
                    value.span,
                ));
            }
            element = Some(value.ty);
            values.push(Some(value));
        }
        let element =
            element.ok_or_else(|| invalid("empty array requires an element type annotation"))?;
        let ty = intern(element, self.types)?;
        Ok(Expr {
            span: array.span,
            ty,
            kind: E::Array(values),
        })
    }

    pub(super) fn array_member(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
        read: bool,
    ) -> Result<Expr> {
        let Type::Array(id) = value.ty else {
            return Err(invalid("missing array type"));
        };
        let (ty, kind) = match &member.prop {
            ast::MemberProp::Ident(name) if name.sym == "length" => {
                (self.types.number(), E::Length(Box::new(value)))
            }
            ast::MemberProp::Computed(key)
                if self.types.profile.array_mutations()
                    && matches!(&*key.expr,ast::Expr::Lit(ast::Lit::Str(text)) if text.value.as_str()==Some("length")) =>
            {
                (self.types.number(), E::Length(Box::new(value)))
            }
            ast::MemberProp::Computed(key) => {
                if read && self.types.profile.missing() {
                    return self.maybe_index(
                        member,
                        value,
                        &key.expr,
                        self.types.arrays[id as usize],
                    );
                }
                let index = self.expr(&key.expr)?;
                if index.ty != self.types.number() {
                    return Err(self
                        .source
                        .locate(type_error("array index must be a number"), index.span));
                }
                (
                    self.types.arrays[id as usize],
                    E::Index(Box::new(value), Box::new(index)),
                )
            }
            _ => {
                return Err(unsupported(
                    "array property other than length or numeric index",
                ));
            }
        };
        Ok(Expr {
            span: member.span,
            ty,
            kind,
        })
    }
}

impl Lower<'_> {
    pub(super) fn array_method(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
        method: &str,
        expected: Option<Type>,
    ) -> Result<Expr> {
        if matches!(
            method,
            "map" | "filter" | "forEach" | "find" | "reduce" | "some" | "every"
        ) {
            return self.array_callback(call, member, method, expected);
        }
        let base = self.expr(&member.obj)?;
        self.array_plain_method(call, base, method)
    }
    pub(super) fn array_plain_method(
        &mut self,
        call: &ast::CallExpr,
        base: Expr,
        method: &str,
    ) -> Result<Expr> {
        let Type::Array(id) = base.ty else {
            return Err(type_error(
                "array method requires a narrowed array receiver",
            ));
        };
        let element = self.types.arrays[id as usize];
        if matches!(method, "includes" | "indexOf") {
            if call.args.len() > 2 {
                return Err(invalid(format!(
                    "array.{method} expects at most two arguments"
                )));
            }
            let needle = if let Some(arg) = call.args.first() {
                self.expr(&arg.expr)?
            } else {
                Expr {
                    span: call.span,
                    ty: Type::Undefined,
                    kind: E::Integer(0),
                }
            };
            if needle.ty.value_kind().is_none() {
                return Err(self.source.locate(
                    unsupported("array search value other than a checked value"),
                    needle.span,
                ));
            }
            let from = self.array_bound(call.args.get(1), call.span)?;
            let (mode, ty) = if method == "includes" {
                (ir::ArraySearch::Includes, Type::Boolean)
            } else {
                (ir::ArraySearch::IndexOf, self.types.number())
            };
            return Ok(Expr {
                span: call.span,
                ty,
                kind: E::ArraySearch(mode, Box::new(base), Box::new(needle), Box::new(from)),
            });
        }
        if matches!(method, "at" | "slice") {
            let maximum = if method == "at" { 1 } else { 2 };
            if call.args.len() > maximum {
                return Err(invalid(format!(
                    "array.{method} expects at most {maximum} arguments"
                )));
            }
            let start = self.array_bound(call.args.first(), call.span)?;
            let (ty, kind) = if method == "at" {
                (
                    unions::intern(vec![element, Type::Undefined], self.types)?,
                    E::ArrayAt(Box::new(base), Box::new(start)),
                )
            } else {
                let end = self.array_bound(call.args.get(1), call.span)?;
                (
                    base.ty,
                    E::ArraySlice(Box::new(base), Box::new(start), Box::new(end)),
                )
            };
            return Ok(Expr {
                span: call.span,
                ty,
                kind,
            });
        }
        let (ty, kind) = if method == "pop" {
            if !call.args.is_empty() {
                return Err(invalid("array.pop expects no arguments"));
            }
            let result = unions::intern(vec![element, Type::Undefined], self.types)?;
            (result, E::ArrayPop(Box::new(base)))
        } else if call.args.is_empty() {
            (self.types.number(), E::Length(Box::new(base)))
        } else {
            if call.args.len() > MAX_ARRAY_ELEMENTS {
                return Err(invalid("array.push argument limit"));
            }
            let mut values = Vec::new();
            for arg in &call.args {
                let value = self.expr_expected(&arg.expr, Some(element))?;
                if value.ty != element {
                    return Err(self
                        .source
                        .locate(type_error("array.push element type mismatch"), value.span));
                }
                values.push(Some(value));
            }
            let items = Expr {
                span: call.span,
                ty: base.ty,
                kind: E::Array(values),
            };
            (
                self.types.number(),
                E::ArrayPush(Box::new(base), Box::new(items)),
            )
        };
        Ok(Expr {
            span: call.span,
            ty,
            kind,
        })
    }
}

impl Lower<'_> {
    fn array_bound(
        &mut self,
        arg: Option<&ast::ExprOrSpread>,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let Some(arg) = arg else {
            return Ok(Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            });
        };
        let value = self.expr(&arg.expr)?;
        if unions::members(value.ty, self.types)
            .iter()
            .any(|ty| !matches!(ty, Type::Float64 | Type::Undefined))
        {
            return Err(self.source.locate(
                type_error(format!(
                    "array bound requires number or undefined, found {}",
                    value.ty
                )),
                value.span,
            ));
        }
        Ok(value)
    }
}
