//! Tuple types of v38: named members (`[name: string, age?: number]`),
//! optional and rest elements (`[A, B?, ...C[]]`), the empty tuple and
//! variadic spreads of tuple types (`[...A, ...B]`), which flatten at
//! compile time. A rest element must be the last element after flattening:
//! a leading or middle rest position has no static index for later
//! elements.
use super::*;

/// One element as declared: its type and whether it is optional.
enum Element {
    Fixed(Type, bool),
    /// A spread of a tuple or array type.
    Spread(Type),
}

pub(super) fn annotation(
    tuple: &ast::TsTupleType,
    readonly: bool,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let mut elements = Vec::new();
    for element in &tuple.elem_types {
        let labelled_optional =
            matches!(&element.label, Some(ast::Pat::Ident(id)) if id.id.optional);
        let labelled_rest = matches!(&element.label, Some(ast::Pat::Rest(_)));
        elements.push(match &*element.ty {
            ast::TsType::TsRestType(rest) => Element::Spread(keyword_type(&rest.type_ann, types)?),
            ast::TsType::TsOptionalType(optional) => {
                Element::Fixed(keyword_type(&optional.type_ann, types)?, true)
            }
            ty if labelled_rest => Element::Spread(keyword_type(ty, types)?),
            ty => Element::Fixed(keyword_type(ty, types)?, labelled_optional),
        });
    }
    build(elements, readonly, types)
}

#[derive(Default)]
struct Flattened {
    elements: Vec<Type>,
    required: usize,
    optional_seen: bool,
    rest: Option<Type>,
}
impl Flattened {
    fn push(&mut self, ty: Type, optional: bool, types: &mut ir::ValueTypes) -> Result<()> {
        if self.rest.is_some() {
            return Err(unsupported(
                "tuple element after a rest element (a middle or leading rest position has no static index for later elements)",
            ));
        }
        if optional {
            self.optional_seen = true;
            self.elements.push(union_with_undefined(ty, types)?);
        } else {
            if self.optional_seen {
                return Err(invalid(
                    "a required tuple element cannot follow an optional element",
                ));
            }
            self.elements.push(ty);
            self.required += 1;
        }
        Ok(())
    }
    fn rest(&mut self, element: Type) -> Result<()> {
        if self.rest.replace(element).is_some() {
            return Err(unsupported("tuple type with two rest elements"));
        }
        Ok(())
    }
}

fn build(declared: Vec<Element>, readonly: bool, types: &mut ir::ValueTypes) -> Result<Type> {
    let mut flat = Flattened::default();
    for element in declared {
        match element {
            Element::Fixed(ty, optional) => flat.push(ty, optional, types)?,
            Element::Spread(Type::Tuple(id)) => {
                let tuple = types.tuples[id as usize].clone();
                for (index, ty) in tuple.elements.iter().enumerate() {
                    let optional = index >= tuple.required;
                    let ty = if optional {
                        strip_undefined(*ty, types)?
                    } else {
                        *ty
                    };
                    flat.push(ty, optional, types)?;
                }
                if let Some(element) = tuple.rest {
                    flat.rest(element)?;
                }
            }
            Element::Spread(Type::Array(id)) => flat.rest(types.arrays[id as usize])?,
            Element::Spread(Type::Never) => return Ok(Type::Never),
            Element::Spread(other) => {
                return Err(type_error(format!(
                    "a rest element type must be an array or tuple type, found {}",
                    subtyping::describe(other, types)
                )));
            }
        }
    }
    tuples::intern_rest(flat.elements, flat.required, flat.rest, readonly, types)
}

/// `ty | undefined`.
pub(super) fn union_with_undefined(ty: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    if unions::members(ty, types).contains(&Type::Undefined) {
        return Ok(ty);
    }
    unions::intern(vec![ty, Type::Undefined], types)
}

/// `ty` without its `undefined` member.
pub(super) fn strip_undefined(ty: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    let members = unions::members(ty, types);
    if !members.contains(&Type::Undefined) {
        return Ok(ty);
    }
    union_of(
        members
            .into_iter()
            .filter(|member| *member != Type::Undefined)
            .collect(),
        types,
    )
}

/// A tuple of `elements` (the first `required` required), with `rest`.
pub(super) fn tuple_of(
    elements: Vec<Type>,
    required: usize,
    rest: Option<Type>,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    tuples::intern_rest(elements, required, rest, false, types)
}
