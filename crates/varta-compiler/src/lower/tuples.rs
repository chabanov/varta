//! Tuple types (v37 `collections`): `[A, B]`, `readonly [A, B]` and trailing
//! optional elements `[A, B?]`. A tuple is physically the dense array of the
//! union of its element types (`ir::TupleType::array`), so logging, JSON and
//! `unknown` see an ordinary array; the static type keeps every position's
//! type. Reads at a literal index and destructuring yield the position's type
//! through a checked union unbox; other indexes read the physical element.
use super::*;

/// Element bound of one tuple type (a static layout, far below the array bound).
pub(super) const MAX_TUPLE_ELEMENTS: usize = 32;
/// The element count of a pair: an `Object.entries` or `Map` entry.
pub(super) const PAIR: usize = 2;

pub(super) fn intern(
    elements: Vec<Type>,
    required: usize,
    readonly: bool,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    if elements.len() > MAX_TUPLE_ELEMENTS {
        return Err(invalid("tuple element count exceeds limit"));
    }
    if elements.is_empty() {
        return Err(unsupported("empty tuple type"));
    }
    if let Some(id) = types
        .tuples
        .iter()
        .position(|t| t.elements == elements && t.required == required && t.readonly == readonly)
    {
        return Ok(Type::Tuple(id as u32));
    }
    let mut members: Vec<Type> = elements.iter().map(|ty| physical(*ty, types)).collect();
    if required < elements.len() {
        members.push(Type::Undefined);
    }
    let element = unions::intern(members, types).map_err(|_| {
        unsupported("tuple whose element types do not form one admitted union (at most one record shape, array type and function type)")
    })?;
    let array = arrays::intern(element, types)?;
    let id = types.tuples.len() as u32;
    types.tuples.push(ir::TupleType {
        elements,
        required,
        readonly,
        array,
    });
    Ok(Type::Tuple(id))
}

/// The physical type a tuple element is stored as.
fn physical(ty: Type, types: &ir::ValueTypes) -> Type {
    match ty {
        Type::Tuple(id) => types.tuples[id as usize].array,
        other => other,
    }
}

/// `[A, B?]` and (`readonly`) `readonly [A, B]`.
pub(super) fn annotation(
    tuple: &ast::TsTupleType,
    readonly: bool,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let mut elements = Vec::new();
    let mut required = 0;
    for element in &tuple.elem_types {
        match &*element.ty {
            ast::TsType::TsRestType(_) => {
                return Err(unsupported("tuple rest element"));
            }
            ast::TsType::TsOptionalType(optional) => {
                let ty = keyword_type(&optional.type_ann, types)?;
                elements.push(unions::intern(vec![ty, Type::Undefined], types)?);
            }
            ty => {
                if required < elements.len() {
                    return Err(invalid(
                        "a required tuple element cannot follow an optional element",
                    ));
                }
                elements.push(keyword_type(ty, types)?);
                required += 1;
            }
        }
    }
    intern(elements, required, readonly, types)
}

/// The integer value of a non-negative numeric literal index.
fn literal_index(expression: &ast::Expr) -> Option<usize> {
    let ast::Expr::Lit(ast::Lit::Num(number)) = stmt::unparen(expression) else {
        return None;
    };
    (number.value >= 0.0 && number.value.fract() == 0.0 && number.value < MAX_TUPLE_ELEMENTS as f64)
        .then_some(number.value as usize)
}

impl Lower<'_> {
    /// The tuple type an array literal in `context` is checked against: the
    /// context itself or its only tuple member.
    pub(super) fn tuple_context(&self, context: Option<Type>) -> Option<u32> {
        let tuples: Vec<u32> = unions::members(context?, self.types)
            .into_iter()
            .filter_map(|ty| match ty {
                Type::Tuple(id) => Some(id),
                _ => None,
            })
            .collect();
        match tuples.as_slice() {
            [id] => Some(*id),
            _ => None,
        }
    }
    /// A tuple literal `[a, b]` checked against tuple type `id`.
    pub(super) fn tuple_literal(&mut self, array: &ast::ArrayLit, id: u32) -> Result<Expr> {
        let tuple = self.types.tuples[id as usize].clone();
        let Type::Array(physical) = tuple.array else {
            return Err(invalid("missing tuple array"));
        };
        let element = self.types.arrays[physical as usize];
        if array.elems.len() < tuple.required || array.elems.len() > tuple.elements.len() {
            return Err(self.source.locate(
                type_error(format!(
                    "tuple literal has {} elements, the tuple type expects {}",
                    array.elems.len(),
                    tuple.elements.len()
                )),
                array.span,
            ));
        }
        let mut values = Vec::new();
        for (entry, expected) in array.elems.iter().zip(&tuple.elements) {
            let Some(entry) = entry else {
                return Err(self
                    .source
                    .locate(unsupported("hole in a tuple literal"), array.span));
            };
            if entry.spread.is_some() {
                return Err(self
                    .source
                    .locate(unsupported("spread in a tuple literal"), entry.expr.span()));
            }
            let value = self.expr_expected(&entry.expr, Some(*expected))?;
            let value = unions::widen(value, Some(*expected), self.types)?;
            if value.ty != *expected {
                return Err(self.source.locate(
                    type_error(format!(
                        "tuple element type mismatch: expected {expected}, found {}",
                        value.ty
                    )),
                    value.span,
                ));
            }
            let value = self.physical_array(value)?;
            values.push(Some(unions::widen(value, Some(element), self.types)?));
        }
        Ok(Expr {
            span: array.span,
            ty: Type::Tuple(id),
            kind: E::Array(values),
        })
    }
    /// Position `index` of tuple `value` (a pure local read) as its own type.
    pub(super) fn tuple_element(&mut self, value: Expr, index: usize) -> Result<Expr> {
        let Type::Tuple(id) = value.ty else {
            return Err(invalid("missing tuple type"));
        };
        let tuple = self.types.tuples[id as usize].clone();
        let span = value.span;
        let Some(&ty) = tuple.elements.get(index) else {
            return Err(self.source.locate(
                type_error(format!(
                    "tuple index {index} is out of range for a tuple of length {}",
                    tuple.elements.len()
                )),
                span,
            ));
        };
        let Type::Array(physical) = tuple.array else {
            return Err(invalid("missing tuple array"));
        };
        let stored = self.types.arrays[physical as usize];
        let array = Expr {
            span,
            ty: tuple.array,
            kind: E::Reinterpret(Box::new(value)),
        };
        let index = Expr {
            span,
            ty: Type::Float64,
            kind: E::Float64(index as f64),
        };
        // An absent optional element reads as `undefined`.
        let (read, read_ty) = if index_required(&tuple, &index) {
            (E::Index(Box::new(array), Box::new(index)), stored)
        } else {
            let read_ty = unions::intern(vec![stored, Type::Undefined], self.types)?;
            (E::MaybeIndex(Box::new(array), Box::new(index)), read_ty)
        };
        let read = Expr {
            span,
            ty: read_ty,
            kind: read,
        };
        Ok(self.narrow_element(read, ty))
    }
    /// The stored element `read` (a union cell or the exact type) as `ty`.
    fn narrow_element(&mut self, read: Expr, ty: Type) -> Expr {
        let want = match ty {
            Type::Tuple(id) => self.types.tuples[id as usize].array,
            other => other,
        };
        let narrowed = if read.ty == want {
            read
        } else if want.is_union() {
            // Union cells share one physical form: only the static type narrows.
            Expr { ty: want, ..read }
        } else {
            Expr {
                span: read.span,
                ty: want,
                kind: E::UnboxUnion(Box::new(read)),
            }
        };
        if want == ty {
            narrowed
        } else {
            Expr {
                span: narrowed.span,
                ty,
                kind: E::Reinterpret(Box::new(narrowed)),
            }
        }
    }
    /// `tuple.length`, `tuple[0]` and `tuple[i]`.
    pub(super) fn tuple_member(&mut self, member: &ast::MemberExpr, value: Expr) -> Result<Expr> {
        if let ast::MemberProp::Computed(key) = &member.prop
            && let Some(index) = literal_index(&key.expr)
        {
            let slot = self.hidden(value.ty);
            let ty = value.ty;
            let element = self.tuple_element(
                Expr {
                    span: value.span,
                    ty,
                    kind: E::Local(slot),
                },
                index,
            )?;
            let marker = self.hidden(Type::Number);
            let output = self.hidden(element.ty);
            return Ok(Expr {
                span: member.span,
                ty: element.ty,
                kind: E::ValueBlock {
                    output,
                    marker,
                    roots: vec![slot, output],
                    body: vec![S::Init(slot, value), S::Init(output, element)],
                },
            });
        }
        let array = self.physical_array(value)?;
        self.array_member(member, array, true)
    }
}

/// Whether a literal position is a required element (always present).
fn index_required(tuple: &ir::TupleType, index: &Expr) -> bool {
    matches!(index.kind, E::Float64(at) if (at as usize) < tuple.required)
}
