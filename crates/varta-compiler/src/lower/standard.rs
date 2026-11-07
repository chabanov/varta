//! The ECMAScript 2025 standard-library methods of v38 (`standard_library`,
//! K9 D), implemented by the standard prelude (`modules::engine::STANDARD`)
//! and, for `Set` and `Map.groupBy`, by the v38 collections prelude. The
//! receiver is lowered once; a class instance receiver keeps ordinary class
//! dispatch and a collection receiver its collection methods.
use super::*;
use crate::modules::engine::{COLLECTIONS, DATE, DATE_CLASS, STANDARD, TEXT};
use ir::CollectionKind as K;

/// The member names this module dispatches on arrays and strings.
pub(crate) const METHODS: &[&str] = &[
    "sort",
    "toSorted",
    "toReversed",
    "with",
    "toSpliced",
    "splice",
    "copyWithin",
    "concat",
    "shift",
    "unshift",
    "flat",
    "flatMap",
    "keys",
    "values",
    "entries",
    "at",
    "substr",
    "isWellFormed",
    "toWellFormed",
    "normalize",
    "toLocaleUpperCase",
    "toLocaleLowerCase",
    "hasOwnProperty",
];
/// The ES2025 `Set.prototype` methods and their collections-prelude functions.
const SET_METHODS: [(&str, &str); 7] = [
    ("union", "setUnion"),
    ("intersection", "setIntersection"),
    ("difference", "setDifference"),
    ("symmetricDifference", "setSymmetricDifference"),
    ("isSubsetOf", "setIsSubsetOf"),
    ("isSupersetOf", "setIsSupersetOf"),
    ("isDisjointFrom", "setIsDisjointFrom"),
];
/// The ES2025 iterator helper methods (`Iterator.prototype`, §27.1.4).
const ITERATOR_METHODS: [&str; 11] = [
    "map", "filter", "take", "drop", "flatMap", "reduce", "toArray", "forEach", "some", "every",
    "find",
];
/// The iterator helpers whose result is again an iterator.
const ITERATOR_RESULTS: [&str; 5] = ["map", "filter", "take", "drop", "flatMap"];
/// Methods that write their array receiver.
const MUTATING: [&str; 5] = ["sort", "splice", "copyWithin", "shift", "unshift"];
/// Global objects whose static members are not receiver methods here.
const NAMESPACES: [&str; 9] = [
    "Object", "Array", "Reflect", "Promise", "JSON", "Math", "Number", "String", "Symbol",
];

fn number(span: swc_common::Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}

impl Lower<'_> {
    /// Whether `member` is a standard-library method call this module lowers.
    pub(super) fn standard_receiver(&self, member: &ast::MemberExpr) -> bool {
        let ast::MemberProp::Ident(method) = &member.prop else {
            return false;
        };
        let name = method.sym.as_ref();
        if ITERATOR_METHODS.contains(&name) && self.iterator_receiver(&member.obj, 0) {
            return true;
        }
        if !METHODS.contains(&name) {
            return false;
        }
        !self.namespace(&member.obj)
    }

    /// Whether `expression` names an unshadowed global namespace object.
    fn namespace(&self, expression: &ast::Expr) -> bool {
        matches!(
            stmt::unparen(expression),
            ast::Expr::Ident(id) if NAMESPACES.contains(&id.sym.as_ref())
                && self.binding(id.sym.as_ref()).is_err()
        )
    }

    /// Whether `expression` statically is a synchronous iterator: a
    /// generator binding, field or call, an array or collection's `keys`,
    /// `values` or `entries`, `Iterator.from(...)` or an iterator helper of
    /// one of these.
    fn iterator_receiver(&self, expression: &ast::Expr, depth: usize) -> bool {
        if depth >= super::MAX_ANALYSIS_DEPTH {
            return false;
        }
        if matches!(self.peek_type(expression, depth), Some(Type::Generator(_))) {
            return true;
        }
        let ast::Expr::Call(call) = stmt::unparen(expression) else {
            return false;
        };
        let ast::Callee::Expr(callee) = &call.callee else {
            return false;
        };
        match stmt::unparen(callee) {
            ast::Expr::Ident(name) => self
                .functions
                .get(name.sym.as_ref())
                .is_some_and(|f| matches!(f.result, Type::Generator(_))),
            ast::Expr::Member(member) => {
                let ast::MemberProp::Ident(method) = &member.prop else {
                    return false;
                };
                let method = method.sym.as_ref();
                if method == "from"
                    && matches!(stmt::unparen(&member.obj), ast::Expr::Ident(id) if id.sym == "Iterator" && self.binding("Iterator").is_err())
                {
                    return true;
                }
                if ITERATOR_RESULTS.contains(&method) {
                    return self.iterator_receiver(&member.obj, depth + 1);
                }
                matches!(method, "keys" | "values" | "entries")
                    && !self.namespace(&member.obj)
                    && (matches!(stmt::unparen(&member.obj), ast::Expr::Array(_))
                        || matches!(
                            self.peek_type(&member.obj, depth + 1),
                            Some(
                                Type::Array(_)
                                    | Type::Tuple(_)
                                    | Type::Collection(K::Map | K::Set, _)
                            )
                        )
                        || self.collection_receiver(&member.obj, depth + 1))
            }
            _ => false,
        }
    }

    /// `receiver.method(...)` for a v38 standard-library method name.
    pub(super) fn standard_method(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let ast::MemberProp::Ident(method) = &member.prop else {
            return Err(invalid("missing standard method name"));
        };
        let name = method.sym.as_ref();
        if name == "hasOwnProperty" {
            return self.has_own_property(call, member);
        }
        if MUTATING.contains(&name) {
            self.reject_readonly_write(&member.obj)?;
        }
        let receiver = self.expr(&member.obj)?;
        match receiver.ty {
            Type::Class(_) => self.class_method_call(call, member, Some(receiver), expected),
            Type::Collection(kind, id) => self.collection_method(call, receiver, (kind, id), name),
            Type::Array(_) | Type::Tuple(_) => {
                if call.type_args.is_some() {
                    return Err(self
                        .source
                        .locate(invalid(format!("{name} has no type arguments")), call.span));
                }
                let array = self.physical_array(receiver)?;
                self.standard_array_method(call, array, name)
            }
            Type::String => self.standard_string_method(call, receiver, name),
            Type::Generator(id) if ITERATOR_METHODS.contains(&name) => {
                self.iterator_helper(call, receiver, id, name)
            }
            ty => Err(self.source.locate(
                type_error(format!(
                    "{name} requires an array or string receiver, found {ty}"
                )),
                receiver.span,
            )),
        }
    }

    /// `value.hasOwnProperty(key)` (§20.1.3.2) of an object without its own
    /// `hasOwnProperty`: the same test as `Object.hasOwn(value, key)`, with
    /// the same evaluation order.
    fn has_own_property(&mut self, call: &ast::CallExpr, member: &ast::MemberExpr) -> Result<Expr> {
        if self.binding("Object").is_ok() || self.functions.contains_key("Object") {
            return Err(self.source.locate(
                unsupported("hasOwnProperty while Object is shadowed"),
                call.span,
            ));
        }
        let mut args = vec![ast::ExprOrSpread {
            spread: None,
            expr: member.obj.clone(),
        }];
        args.extend(call.args.iter().cloned());
        let own = ast::CallExpr {
            callee: ast::Callee::Expr(Box::new(ast::Expr::Member(ast::MemberExpr {
                span: member.span,
                obj: Box::new(ast::Expr::Ident(ast::Ident::new_no_ctxt(
                    "Object".into(),
                    member.span,
                ))),
                prop: ast::MemberProp::Ident(ast::IdentName::new("hasOwn".into(), member.span)),
            }))),
            args,
            ..call.clone()
        };
        self.expr(&ast::Expr::Call(own))
    }

    fn standard_string_method(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        name: &str,
    ) -> Result<Expr> {
        let function = match name {
            "concat" => {
                return self.engine_call(
                    (&TEXT, "stringConcat", &[]),
                    vec![receiver],
                    &call.args,
                    call.span,
                );
            }
            // Without a locale argument the root-locale case mapping is the
            // locale-independent one.
            "toLocaleUpperCase" if call.args.is_empty() => "stringToLocaleUpperCase",
            "toLocaleLowerCase" if call.args.is_empty() => "stringToLocaleLowerCase",
            "at" => "stringAt",
            "substr" => "stringSubstr",
            "isWellFormed" => "stringIsWellFormed",
            "normalize" => "stringNormalize",
            "toWellFormed" => "stringToWellFormed",
            _ => {
                return Err(self
                    .source
                    .locate(unsupported(&format!("String.prototype.{name}")), call.span));
            }
        };
        self.engine_call(
            (&STANDARD, function, &[]),
            vec![receiver],
            &call.args,
            call.span,
        )
    }

    fn standard_array_method(
        &mut self,
        call: &ast::CallExpr,
        array: Expr,
        name: &str,
    ) -> Result<Expr> {
        let Type::Array(id) = array.ty else {
            return Err(invalid("missing standard array type"));
        };
        let element = self.types.arrays[id as usize];
        let span = call.span;
        if call.args.iter().any(|arg| arg.spread.is_some()) && name != "concat" {
            return Err(self
                .source
                .locate(unsupported("spread argument to a built-in method"), span));
        }
        match name {
            "at" => self.array_plain_method(call, array, "at"),
            "sort" | "toSorted" => self.array_sort(call, array, element, name == "sort"),
            "toReversed" | "keys" | "values" | "entries" | "shift" => {
                if !call.args.is_empty() {
                    return Err(self
                        .source
                        .locate(invalid(format!("array.{name} expects no arguments")), span));
                }
                let function = match name {
                    "toReversed" => "arrayToReversed",
                    "keys" => "arrayKeys",
                    "values" => "arrayValues",
                    "entries" => "arrayEntries",
                    _ => "arrayShift",
                };
                self.engine_apply((&STANDARD, function, &[element]), vec![array], span)
            }
            "with" | "unshift" | "copyWithin" => {
                let function = match name {
                    "with" => "arrayWith",
                    "unshift" => "arrayUnshift",
                    _ => "arrayCopyWithin",
                };
                self.engine_call(
                    (&STANDARD, function, &[element]),
                    vec![array],
                    &call.args,
                    span,
                )
            }
            "toSpliced" | "splice" => {
                // The argument count decides the delete count (§23.1.3.31).
                let supplied = number(span, call.args.len() as f64);
                let function = if name == "splice" {
                    "arraySplice"
                } else {
                    "arrayToSpliced"
                };
                self.engine_call(
                    (&STANDARD, function, &[element]),
                    vec![array, supplied],
                    &call.args,
                    span,
                )
            }
            "concat" => self.array_concat(call, array, element),
            "flat" => self.array_flat(call, array),
            "flatMap" => self.array_flat_map(call, array, element),
            _ => Err(self
                .source
                .locate(unsupported(&format!("Array.prototype.{name}")), span)),
        }
    }

    /// Whether every member of `element` is a primitive whose String form
    /// the default sort order compares.
    fn primitive_elements(&self, element: Type) -> bool {
        unions::members(element, self.types).iter().all(|ty| {
            matches!(
                ty.erased(),
                Type::String
                    | Type::Float64
                    | Type::Number
                    | Type::Boolean
                    | Type::BigInt
                    | Type::Undefined
                    | Type::Null
            )
        })
    }

    /// `array.sort(compare?)` and `array.toSorted(compare?)`.
    fn array_sort(
        &mut self,
        call: &ast::CallExpr,
        array: Expr,
        element: Type,
        in_place: bool,
    ) -> Result<Expr> {
        let span = call.span;
        if call.args.len() > 1 {
            return Err(self
                .source
                .locate(invalid("array.sort expects at most one comparator"), span));
        }
        let comparator = call
            .args
            .first()
            .filter(|arg| !matches!(stmt::unparen(&arg.expr), ast::Expr::Ident(id) if id.sym == "undefined"));
        let Some(comparator) = comparator else {
            if !self.primitive_elements(element) {
                return Err(self.source.locate(
                    unsupported("Array.prototype.sort without a comparator of non-primitive elements (object to string conversion is not modelled)"),
                    span,
                ));
            }
            let function = if in_place {
                "arraySortDefault"
            } else {
                "arrayToSortedDefault"
            };
            // A present `undefined` comparator is still evaluated.
            let mut value =
                self.engine_apply((&STANDARD, function, &[element]), vec![array], span)?;
            if let Some(arg) = call.args.first() {
                let ignored = self.expr(&arg.expr)?;
                value = Expr {
                    span,
                    ty: value.ty,
                    kind: E::Sequence(Box::new(ignored), Box::new(value)),
                };
            }
            return Ok(value);
        };
        let function = if in_place {
            "arraySort"
        } else {
            "arrayToSorted"
        };
        self.engine_call(
            (&STANDARD, function, &[element]),
            vec![array],
            std::slice::from_ref(comparator),
            span,
        )
    }

    /// `array.concat(...items)`: an argument of the receiver's array type
    /// spreads, one of its element type is appended.
    fn array_concat(&mut self, call: &ast::CallExpr, array: Expr, element: Type) -> Result<Expr> {
        let span = call.span;
        let nested = unions::members(element, self.types)
            .iter()
            .any(|ty| matches!(ty, Type::Array(_) | Type::Tuple(_)));
        let mut parts = Vec::new();
        for arg in &call.args {
            let spread = arg.spread.is_some();
            let value = self.expr_expected(&arg.expr, Some(array.ty))?;
            let value = if matches!(value.ty, Type::Tuple(_)) {
                self.physical_array(value)?
            } else {
                value
            };
            if value.ty == array.ty && !spread {
                parts.push(Some(value));
                continue;
            }
            if nested {
                return Err(self.source.locate(
                    unsupported("Array.prototype.concat of an array whose elements are arrays with an argument that is not of the receiver's type (JavaScript spreads every array argument)"),
                    value.span,
                ));
            }
            if spread {
                // `concat(...xs)`: each element of `xs` is one argument.
                return Err(self.source.locate(
                    unsupported("spread argument to Array.prototype.concat"),
                    value.span,
                ));
            }
            let value = unions::widen(value, Some(element), self.types)?;
            if value.ty != element {
                return Err(self.source.locate(
                    type_error(format!(
                        "concat argument type mismatch: expected {} or {element}, found {}",
                        array.ty, value.ty
                    )),
                    value.span,
                ));
            }
            parts.push(Some(Expr {
                span: value.span,
                ty: array.ty,
                kind: E::Array(vec![Some(value)]),
            }));
        }
        let ty = arrays::intern(array.ty, self.types)?;
        let parts = Expr {
            span,
            ty,
            kind: E::Array(parts),
        };
        self.engine_apply(
            (&STANDARD, "arrayConcat", &[element]),
            vec![array, parts],
            span,
        )
    }

    /// The flattening depth of `array.flat(depth?)`: a literal number or
    /// `Infinity`, since the result type depends on it.
    fn flat_depth(&mut self, call: &ast::CallExpr) -> Result<f64> {
        let Some(arg) = call.args.first() else {
            return Ok(1.0);
        };
        if call.args.len() > 1 {
            return Err(self
                .source
                .locate(invalid("array.flat expects at most one depth"), call.span));
        }
        match stmt::unparen(&arg.expr) {
            ast::Expr::Lit(ast::Lit::Num(value)) => Ok(value.value),
            ast::Expr::Ident(id) if id.sym == "Infinity" && self.binding("Infinity").is_err() => {
                Ok(f64::INFINITY)
            }
            ast::Expr::Ident(id) if id.sym == "undefined" => Ok(1.0),
            _ => Err(self.source.locate(
                unsupported("Array.prototype.flat with a depth other than a number literal or Infinity (the result type depends on it)"),
                arg.span(),
            )),
        }
    }

    /// `array.flat(depth?)`: one prelude call per flattened level.
    fn array_flat(&mut self, call: &ast::CallExpr, array: Expr) -> Result<Expr> {
        let span = call.span;
        let depth = self.flat_depth(call)?;
        let mut value = array;
        // Depth 0 (and a negative or NaN depth) still copies without holes.
        let mut remaining = if depth.is_nan() { 0.0 } else { depth.trunc() };
        let mut flattened = false;
        loop {
            let Type::Array(id) = value.ty else {
                return Err(invalid("missing flat array type"));
            };
            let element = self.types.arrays[id as usize];
            let members = unions::members(element, self.types);
            let inner: Vec<Type> = members
                .iter()
                .filter_map(|ty| match ty {
                    Type::Array(inner) => Some(self.types.arrays[*inner as usize]),
                    Type::Tuple(inner) => {
                        let array = self.types.tuples[*inner as usize].array;
                        match array {
                            Type::Array(inner) => Some(self.types.arrays[inner as usize]),
                            _ => None,
                        }
                    }
                    _ => None,
                })
                .collect();
            if remaining < 1.0 || inner.is_empty() {
                if flattened {
                    return Ok(value);
                }
                // No array level to flatten: a hole-free copy.
                return self.engine_apply(
                    (&STANDARD, "arrayCompact", &[element]),
                    vec![value],
                    span,
                );
            }
            let values: Vec<Type> = members
                .iter()
                .copied()
                .filter(|ty| !matches!(ty, Type::Array(_) | Type::Tuple(_)))
                .collect();
            if values.is_empty() && inner.len() == 1 && members.len() == 1 {
                let inner = inner[0];
                let physical = self.flat_physical(value, inner)?;
                value =
                    self.engine_apply((&STANDARD, "arrayFlat", &[inner]), vec![physical], span)?;
            } else {
                let mut union = values;
                for ty in &inner {
                    for member in unions::members(*ty, self.types) {
                        if !union.contains(&member) {
                            union.push(member);
                        }
                    }
                }
                let target = if union.len() == 1 {
                    union[0]
                } else {
                    unions::intern(union, self.types)?
                };
                // `(T | T[])[]` is the only mixed shape the prelude flattens.
                let expected_element = {
                    let nested = arrays::intern(target, self.types)?;
                    let mut members = unions::members(target, self.types);
                    members.push(nested);
                    unions::intern(members, self.types)?
                };
                if expected_element != element {
                    return Err(self.source.locate(
                        unsupported(&format!(
                            "Array.prototype.flat of {element} elements (only T, T[] and their union flatten)"
                        )),
                        span,
                    ));
                }
                value =
                    self.engine_apply((&STANDARD, "arrayFlatMixed", &[target]), vec![value], span)?;
            }
            flattened = true;
            remaining -= 1.0;
        }
    }

    /// An array of tuples or arrays viewed as an array of arrays of `inner`.
    fn flat_physical(&mut self, value: Expr, inner: Type) -> Result<Expr> {
        let ty = arrays::intern(arrays::intern(inner, self.types)?, self.types)?;
        if value.ty == ty {
            return Ok(value);
        }
        Err(self.source.locate(
            unsupported("Array.prototype.flat of tuple elements"),
            value.span,
        ))
    }

    /// `array.flatMap(callback)`: the callback's result decides between the
    /// array and the value form.
    fn array_flat_map(&mut self, call: &ast::CallExpr, array: Expr, element: Type) -> Result<Expr> {
        let span = call.span;
        let [callback] = call.args.as_slice() else {
            return Err(self.source.locate(
                unsupported("Array.prototype.flatMap with arguments other than one callback"),
                span,
            ));
        };
        let params = vec![element, Type::Float64, array.ty];
        let callback = self.array_callback_value(
            &callback.expr,
            &params,
            array_callbacks::Method::Map,
            element,
        )?;
        let Type::Function(signature) = callback.ty else {
            return Err(self.source.locate(
                type_error("flatMap callback must be a typed function"),
                callback.span,
            ));
        };
        let result = self.types.functions[signature as usize].result;
        let members = unions::members(result, self.types);
        let arrays: Vec<Type> = members
            .iter()
            .filter_map(|ty| match ty {
                Type::Array(id) => Some(self.types.arrays[*id as usize]),
                _ => None,
            })
            .collect();
        let (function, inner) = match (arrays.as_slice(), members.len()) {
            ([inner], 1) => ("arrayFlatMap", *inner),
            ([], _) => ("arrayFlatMapValue", result),
            _ => {
                return Err(self.source.locate(
                    unsupported("Array.prototype.flatMap with a callback returning both arrays and other values"),
                    callback.span,
                ));
            }
        };
        let signature = self.engine_signature(&STANDARD, function, &[element, inner])?;
        let callback = self.adapt_callback(callback, signature.params[1])?;
        self.engine_apply(
            (&STANDARD, function, &[element, inner]),
            vec![array, callback],
            span,
        )
    }

    /// `callback` as a value of function type `expected`: a callback that
    /// declares fewer parameters is wrapped by an adapter.
    fn adapt_callback(&mut self, callback: Expr, expected: Type) -> Result<Expr> {
        if callback.ty == expected {
            return Ok(callback);
        }
        let converted = subtyping::coerce(callback, Some(expected), self.types)?;
        if converted.ty != expected {
            return Err(self.source.locate(
                type_error(format!(
                    "callback type mismatch: expected {expected}, found {}",
                    converted.ty
                )),
                converted.span,
            ));
        }
        Ok(converted)
    }
}

impl Lower<'_> {
    /// An ES2025 iterator helper on a synchronous generator that returns no
    /// value and takes no `next` input (every built-in iterator of Varta).
    fn iterator_helper(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        id: u32,
        name: &str,
    ) -> Result<Expr> {
        let span = call.span;
        let generator = self.types.generators[id as usize];
        if generator.asynchronous.is_some()
            || generator.body
            || !matches!(generator.returned, Type::Void | Type::Undefined)
            || generator.next != Type::Undefined
        {
            return Err(self.source.locate(
                unsupported(&format!(
                    "Iterator.prototype.{name} of a generator that returns a value or takes next() input"
                )),
                span,
            ));
        }
        if call.type_args.is_some() {
            return Err(self
                .source
                .locate(invalid(format!("{name} has no type arguments")), span));
        }
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(self
                .source
                .locate(unsupported("spread argument to an iterator helper"), span));
        }
        let item = generator.yielded;
        let arity = |count: usize| -> Result<()> {
            if call.args.len() == count {
                Ok(())
            } else {
                Err(invalid(format!(
                    "Iterator.prototype.{name} expects {count} argument(s)"
                )))
            }
        };
        match name {
            "toArray" => {
                arity(0).map_err(|e| self.source.locate(e, span))?;
                self.engine_apply(
                    (&STANDARD, "iteratorToArray", &[item]),
                    vec![receiver],
                    span,
                )
            }
            "take" | "drop" => {
                arity(1).map_err(|e| self.source.locate(e, span))?;
                let function = if name == "take" {
                    "iteratorTake"
                } else {
                    "iteratorDrop"
                };
                self.engine_call(
                    (&STANDARD, function, &[item]),
                    vec![receiver],
                    &call.args,
                    span,
                )
            }
            "map" | "flatMap" => {
                arity(1).map_err(|e| self.source.locate(e, span))?;
                let callback =
                    self.standard_callback(&call.args[0].expr, &[item, Type::Float64], None)?;
                let result = self.callback_result(&callback)?;
                let (function, output) = if name == "map" {
                    ("iteratorMap", result)
                } else if let Type::Array(inner) = result {
                    ("iteratorFlatMapArrays", self.types.arrays[inner as usize])
                } else {
                    return Err(self.source.locate(
                        unsupported("Iterator.prototype.flatMap with a callback that does not return an array"),
                        callback.span,
                    ));
                };
                self.helper_apply(function, &[item, output], receiver, callback, span)
            }
            "filter" | "forEach" | "some" | "every" | "find" => {
                arity(1).map_err(|e| self.source.locate(e, span))?;
                let result = if name == "forEach" {
                    None
                } else {
                    Some(Type::Boolean)
                };
                let callback =
                    self.standard_callback(&call.args[0].expr, &[item, Type::Float64], result)?;
                let function = match name {
                    "filter" => "iteratorFilter",
                    "forEach" => "iteratorForEach",
                    "some" => "iteratorSome",
                    "every" => "iteratorEvery",
                    _ => "iteratorFind",
                };
                self.helper_apply(function, &[item], receiver, callback, span)
            }
            _ => {
                // reduce(reducer, initial?)
                if call.args.is_empty() || call.args.len() > 2 {
                    return Err(self.source.locate(
                        invalid("Iterator.prototype.reduce expects a reducer and an optional initial value"),
                        span,
                    ));
                }
                if let Some(initial) = call.args.get(1) {
                    let initial = self.expr(&initial.expr)?;
                    let accumulator = initial.ty;
                    let callback = self.standard_callback(
                        &call.args[0].expr,
                        &[accumulator, item, Type::Float64],
                        Some(accumulator),
                    )?;
                    let signature =
                        self.engine_signature(&STANDARD, "iteratorReduce", &[item, accumulator])?;
                    let callback = self.adapt_callback(callback, signature.params[1])?;
                    return self.engine_apply(
                        (&STANDARD, "iteratorReduce", &[item, accumulator]),
                        vec![receiver, callback, initial],
                        span,
                    );
                }
                let callback = self.standard_callback(
                    &call.args[0].expr,
                    &[item, item, Type::Float64],
                    Some(item),
                )?;
                self.helper_apply("iteratorReduceFirst", &[item], receiver, callback, span)
            }
        }
    }

    /// Call iterator helper `function` with its receiver and callback.
    fn helper_apply(
        &mut self,
        function: &str,
        type_args: &[Type],
        receiver: Expr,
        callback: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let signature = self.engine_signature(&STANDARD, function, type_args)?;
        let callback = self.adapt_callback(callback, signature.params[1])?;
        self.engine_apply(
            (&STANDARD, function, type_args),
            vec![receiver, callback],
            span,
        )
    }

    /// `Iterator.from(source)`: a generator as is, the values of an array,
    /// the values of a `Set` and the entries of a `Map`.
    fn iterator_from(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let span = call.span;
        let [source] = call.args.as_slice() else {
            return Err(self
                .source
                .locate(invalid("Iterator.from expects one argument"), span));
        };
        if source.spread.is_some() {
            return Err(self
                .source
                .locate(unsupported("spread argument to Iterator.from"), span));
        }
        let value = self.expr(&source.expr)?;
        match value.ty {
            Type::Generator(_) => Ok(value),
            Type::Array(_) | Type::Tuple(_) => {
                let array = self.physical_array(value)?;
                let Type::Array(id) = array.ty else {
                    return Err(invalid("missing Iterator.from array type"));
                };
                let element = self.types.arrays[id as usize];
                self.engine_apply((&STANDARD, "arrayValues", &[element]), vec![array], span)
            }
            Type::Collection(K::Map | K::Set, _) => Ok(self.collection_iteration(value)?.0),
            ty => Err(self.source.locate(
                unsupported(&format!(
                    "Iterator.from of {ty} (a generator, array, Set or Map is required)"
                )),
                span,
            )),
        }
    }
}

impl Lower<'_> {
    /// Whether `member` is `date.toString`, `valueOf` or `toJSON` of an engine
    /// `Date` instance (the class cannot declare those names).
    pub(super) fn date_conversion(&self, member: &ast::MemberExpr) -> bool {
        matches!(&member.prop, ast::MemberProp::Ident(id) if matches!(id.sym.as_ref(), "toString" | "valueOf" | "toJSON"))
            && self.date_receiver(&member.obj)
    }

    fn date_receiver(&self, expression: &ast::Expr) -> bool {
        if let ast::Expr::New(new) = stmt::unparen(expression) {
            return self
                .class_callee(&new.callee)
                .is_some_and(|class| self.engine_date(class));
        }
        matches!(self.peek_type(expression, 0), Some(Type::Class(class)) if self.engine_date(class))
    }

    fn engine_date(&self, class: u32) -> bool {
        let class = self.types.classes.get(class);
        class.prelude && class.name == DATE_CLASS
    }

    /// `date.toString()`, `valueOf()` and `toJSON()` through the date prelude.
    pub(super) fn date_conversion_call(
        &mut self,
        call: &ast::CallExpr,
        member: &ast::MemberExpr,
    ) -> Result<Expr> {
        if !call.args.is_empty() || call.type_args.is_some() {
            return Err(self.source.locate(
                invalid("Date.prototype.toString, valueOf and toJSON take no arguments here"),
                call.span,
            ));
        }
        let receiver = self.expr(&member.obj)?;
        let function = match &member.prop {
            ast::MemberProp::Ident(id) if id.sym == "valueOf" => "dateValueOf",
            ast::MemberProp::Ident(id) if id.sym == "toJSON" => "dateToJson",
            _ => "dateToString",
        };
        self.engine_apply((&DATE, function, &[]), vec![receiver], call.span)
    }
}

/// Whether `expression` reads values without effects (literals, bindings,
/// field reads, array literals of those), so evaluating it after the value
/// was serialized is unobservable.
fn effect_free(expression: &ast::Expr) -> bool {
    match stmt::unparen(expression) {
        ast::Expr::Lit(_)
        | ast::Expr::Ident(_)
        | ast::Expr::This(_)
        | ast::Expr::Arrow(_)
        | ast::Expr::Fn(_) => true,
        ast::Expr::Unary(unary) => {
            matches!(unary.op, ast::UnaryOp::Minus | ast::UnaryOp::Plus) && effect_free(&unary.arg)
        }
        ast::Expr::Tpl(template) => template.exprs.iter().all(|e| effect_free(e)),
        ast::Expr::Member(member) => {
            matches!(member.prop, ast::MemberProp::Ident(_)) && effect_free(&member.obj)
        }
        ast::Expr::Array(array) => array.elems.iter().all(|element| {
            element
                .as_ref()
                .is_some_and(|e| e.spread.is_none() && effect_free(&e.expr))
        }),
        _ => false,
    }
}

impl Lower<'_> {
    /// `JSON.stringify(value, replacer, space?)` (§25.5.2.1) with a `null`
    /// or `undefined` replacer or a property list, and a number or string
    /// `space`: the compact serialization rewritten by the standard prelude.
    pub(super) fn json_stringify_options(
        &mut self,
        call: &ast::CallExpr,
        checked: bool,
    ) -> Result<Expr> {
        let span = call.span;
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(self
                .source
                .locate(unsupported("spread argument to JSON.stringify"), span));
        }
        for arg in &call.args[1..] {
            if !effect_free(&arg.expr) {
                return Err(self.source.locate(
                    unsupported("JSON.stringify replacer or space argument other than a literal, binding or field read"),
                    arg.expr.span(),
                ));
            }
        }
        let single = ast::CallExpr {
            args: vec![call.args[0].clone()],
            ..call.clone()
        };
        // A discarded result: only the serialization's own effects remain
        // (the other arguments are effect-free).
        if !checked {
            return self.json_stringify(&single, false);
        }
        let mut text = self.expr(&ast::Expr::Call(single))?;
        if text.ty != Type::String {
            return Err(self.source.locate(
                unsupported("JSON.stringify options of a value that may have no JSON text"),
                span,
            ));
        }
        let replacer = &call.args[1].expr;
        let nullish = |expression: &ast::Expr| {
            matches!(stmt::unparen(expression), ast::Expr::Lit(ast::Lit::Null(_)))
                || matches!(stmt::unparen(expression), ast::Expr::Ident(id) if id.sym == "undefined")
        };
        if !nullish(replacer) {
            let list = self.expr(replacer)?;
            let list = self.physical_array(list)?;
            let element = match list.ty {
                Type::Array(id) => Some(self.types.arrays[id as usize]),
                _ => None,
            };
            let Some(element) = element.filter(|element| {
                unions::members(*element, self.types)
                    .iter()
                    .all(|ty| matches!(ty.erased(), Type::String | Type::Float64 | Type::Number))
            }) else {
                return Err(self.source.locate(
                    unsupported(&format!(
                        "JSON.stringify replacer of type {} (a replacer function walks values the host serializes; use a property list array)",
                        list.ty
                    )),
                    list.span,
                ));
            };
            text = self.engine_apply(
                (&STANDARD, "jsonSelect", &[element]),
                vec![text, list],
                span,
            )?;
        }
        if let Some(space) = call.args.get(2)
            && !nullish(&space.expr)
        {
            let space = self.expr(&space.expr)?;
            let function = match space.ty.erased() {
                Type::Float64 | Type::Number => "jsonGapNumber",
                Type::String => "jsonGapString",
                ty => {
                    return Err(self.source.locate(
                        type_error(format!(
                            "JSON.stringify space must be a number or string, found {ty}"
                        )),
                        space.span,
                    ));
                }
            };
            let gap = self.engine_apply((&STANDARD, function, &[]), vec![space], span)?;
            text = self.engine_apply((&STANDARD, "jsonIndent", &[]), vec![text, gap], span)?;
        }
        Ok(text)
    }
}

/// Why the ECMAScript 2025 global `name` (a constructor, or a function or
/// static method called by its dotted name) is not admitted yet; the
/// reasons name the missing value-model part (see the K9 D record in
/// docs/development-plan.md).
pub(super) fn refusal(name: &str) -> Option<&'static str> {
    const DESCRIPTORS: &str = "property descriptors and prototypes (closed checked values have fixed data properties and no mutable prototype)";
    Some(match name {
        "WeakMap" | "WeakSet" | "WeakRef" | "FinalizationRegistry" => {
            "weak collections and references (they need weak edges and deterministic cleanup turns in the exact-root collector)"
        }
        "Proxy" | "Proxy.revocable" => {
            "Proxy (traps would intercept property access on closed checked values, which have no exotic objects)"
        }
        "structuredClone" => {
            "structuredClone (a type-directed deep copy preserving shared and cyclic references is not implemented)"
        }
        "Object.defineProperty"
        | "Object.defineProperties"
        | "Object.getOwnPropertyDescriptor"
        | "Object.getOwnPropertyDescriptors"
        | "Object.getPrototypeOf"
        | "Object.setPrototypeOf"
        | "Object.create" => DESCRIPTORS,
        _ if name.starts_with("Reflect.") => {
            "Reflect (it operates on property descriptors, prototypes and receivers the closed value model does not have)"
        }
        _ if name == "Intl" || name.starts_with("Intl.") => {
            "Intl (ECMA-402 needs locale data shipped with the program; not implemented)"
        }
        _ => return None,
    })
}

/// Whether ES2025 `Set` method `name` returns a new `Set`.
pub(super) fn set_result(name: &str) -> bool {
    matches!(
        name,
        "union" | "intersection" | "difference" | "symmetricDifference"
    )
}

/// The collections-prelude function of ES2025 `Set` method `name`.
pub(super) fn set_method(name: &str) -> Option<&'static str> {
    SET_METHODS
        .iter()
        .find(|(method, _)| *method == name)
        .map(|(_, function)| *function)
}

impl Lower<'_> {
    /// `set.union(other)` and the other ES2025 `Set` methods of a `Set`
    /// argument with the receiver's element type.
    pub(super) fn standard_set_method(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        id: u32,
        function: &str,
    ) -> Result<Expr> {
        let span = call.span;
        let [other] = call.args.as_slice() else {
            return Err(self
                .source
                .locate(invalid("Set method expects one set argument"), span));
        };
        if other.spread.is_some() {
            return Err(self
                .source
                .locate(unsupported("spread argument to a Set method"), span));
        }
        let other = self.expr_expected(&other.expr, Some(receiver.ty))?;
        if other.ty != receiver.ty {
            return Err(self.source.locate(
                unsupported(&format!(
                    "Set method argument {} (a Set with the receiver's element type {} is required; other set-like objects are not modelled)",
                    other.ty, receiver.ty
                )),
                other.span,
            ));
        }
        let element = self.types.collections[id as usize].key;
        let ty = receiver.ty;
        let storage = self.storage(receiver);
        let other = self.storage(other);
        let result = self.engine_apply(
            (&COLLECTIONS, function, &[element]),
            vec![storage, other],
            span,
        )?;
        if result.ty == Type::Boolean {
            return Ok(result);
        }
        Ok(Expr {
            span,
            ty,
            kind: E::Reinterpret(Box::new(result)),
        })
    }

    /// Static standard-library calls: `Array.of`, `Array.from` with a map
    /// function or an array-like length, `Object.groupBy`,
    /// `Object.fromEntries` and `Map.groupBy`.
    pub(super) fn standard_static(
        &mut self,
        call: &ast::CallExpr,
        name: &str,
        expected: Option<Type>,
    ) -> Result<Option<Expr>> {
        let object = name.split('.').next().unwrap_or_default();
        if self.binding(object).is_ok() || self.functions.contains_key(object) {
            return Ok(None);
        }
        let span = call.span;
        // V38 (`integrity_levels`): freeze, seal, preventExtensions and
        // their predicates.
        if self.types.profile.integrity_levels() && integrity::is_member(name) {
            return self.integrity_static(call, name).map(Some);
        }
        if self.types.profile.symbol_values() && name == "Object.getOwnPropertySymbols" {
            return self.object_own_symbols(call).map(Some);
        }
        if let Some(reason) = refusal(name) {
            return Err(self.source.locate(unsupported(reason), span));
        }
        let handled = matches!(
            name,
            "Array.of"
                | "Object.groupBy"
                | "Object.fromEntries"
                | "Object.getOwnPropertyNames"
                | "Map.groupBy"
                | "Iterator.from"
        ) || name == "Array.from" && call.args.len() == 2
            || matches!(name, "Math.max" | "Math.min")
                && call.args.iter().any(|arg| arg.spread.is_some());
        if !handled {
            return Ok(None);
        }
        if matches!(name, "Math.max" | "Math.min") {
            return self.math_extreme_spread(call, name == "Math.max").map(Some);
        }
        if call.type_args.is_some() && name != "Array.of" {
            return Err(self
                .source
                .locate(invalid(format!("{name} has no type arguments")), span));
        }
        if call.args.iter().any(|arg| arg.spread.is_some())
            && matches!(
                name,
                "Object.groupBy" | "Object.fromEntries" | "Map.groupBy"
            )
        {
            return Err(self
                .source
                .locate(unsupported(&format!("spread argument to {name}")), span));
        }
        match name {
            "Array.of" => {
                // `Array.of(a, b)` creates the same fresh dense array as `[a, b]`.
                let literal = ast::Expr::Array(ast::ArrayLit {
                    span,
                    elems: call.args.iter().cloned().map(Some).collect(),
                });
                let expected = match &call.type_args {
                    Some(explicit) => {
                        let args = generics::arguments(explicit, self.types)
                            .map_err(|e| self.source.locate(e, explicit.span))?;
                        let [element] = args.as_slice() else {
                            return Err(self.source.locate(
                                invalid("Array.of takes one type argument"),
                                explicit.span,
                            ));
                        };
                        Some(arrays::intern(*element, self.types)?)
                    }
                    None => expected,
                };
                self.expr_expected(&literal, expected).map(Some)
            }
            "Array.from" if call.args.len() == 2 => self.array_from_map(call, expected).map(Some),
            "Object.groupBy" | "Map.groupBy" => {
                self.group_by(call, name == "Map.groupBy").map(Some)
            }
            "Object.fromEntries" => self.object_from_entries(call).map(Some),
            "Iterator.from" => self.iterator_from(call).map(Some),
            "Object.getOwnPropertyNames" => self.own_property_names(call).map(Some),
            _ => Ok(None),
        }
    }

    /// `Math.max(...values)` and `Math.min(...values)` (§21.3.2.24–25) with
    /// spread arguments: every argument, in order, as one list of numbers.
    fn math_extreme_spread(&mut self, call: &ast::CallExpr, max: bool) -> Result<Expr> {
        let span = call.span;
        if call.type_args.is_some() {
            return Err(self
                .source
                .locate(invalid("Math functions have no type arguments"), span));
        }
        let number = self.types.number();
        let list = arrays::intern(number, self.types)?;
        let mut parts = Vec::new();
        for arg in &call.args {
            let value = self.expr_expected(
                &arg.expr,
                Some(if arg.spread.is_some() { list } else { number }),
            )?;
            let value = self.physical_array(value)?;
            if arg.spread.is_some() {
                if value.ty != list {
                    return Err(self.source.locate(
                        unsupported(&format!(
                            "spread of {} to a Math function (a number[] is required)",
                            value.ty
                        )),
                        value.span,
                    ));
                }
                parts.push(Some(value));
            } else {
                if value.ty != number {
                    return Err(self.source.locate(
                        type_error(format!(
                            "Math argument must be a number, found {}",
                            value.ty
                        )),
                        value.span,
                    ));
                }
                parts.push(Some(Expr {
                    span: value.span,
                    ty: list,
                    kind: E::Array(vec![Some(value)]),
                }));
            }
        }
        let ty = arrays::intern(list, self.types)?;
        let parts = Expr {
            span,
            ty,
            kind: E::Array(parts),
        };
        let function = if max { "mathMaxOf" } else { "mathMinOf" };
        self.engine_apply((&STANDARD, function, &[]), vec![parts], span)
    }

    /// `Array.from(source, mapFn)`.
    fn array_from_map(&mut self, call: &ast::CallExpr, expected: Option<Type>) -> Result<Expr> {
        let span = call.span;
        let [source, callback] = call.args.as_slice() else {
            return Err(invalid("Array.from expects a source and a map function"));
        };
        if let ast::Expr::Object(object) = stmt::unparen(&source.expr)
            && let [ast::PropOrSpread::Prop(prop)] = object.props.as_slice()
            && let ast::Prop::KeyValue(pair) = &**prop
            && matches!(&pair.key, ast::PropName::Ident(key) if key.sym == "length")
        {
            // `Array.from({ length: n }, fn)`: an array-like without elements.
            let length = self.expr(&pair.value)?;
            if length.ty != Type::Float64 {
                return Err(self.source.locate(
                    type_error("Array.from length must be a number"),
                    length.span,
                ));
            }
            let params = vec![Type::Undefined, Type::Float64];
            let callback =
                self.standard_callback(&callback.expr, &params, expected_element(self, expected))?;
            let result = self.callback_result(&callback)?;
            let signature = self.engine_signature(&STANDARD, "arrayFromLength", &[result])?;
            let callback = self.adapt_callback(callback, signature.params[1])?;
            return self.engine_apply(
                (&STANDARD, "arrayFromLength", &[result]),
                vec![length, callback],
                span,
            );
        }
        // An iterable source: the elements are collected first, then mapped
        // in order (the source's own effects run before any map call).
        let single = ast::CallExpr {
            args: vec![source.clone()],
            ..call.clone()
        };
        let values = self.array_from_iterable(&single, None)?;
        let Type::Array(id) = values.ty else {
            return Err(invalid("missing Array.from element type"));
        };
        let element = self.types.arrays[id as usize];
        let params = vec![element, Type::Float64];
        let callback =
            self.standard_callback(&callback.expr, &params, expected_element(self, expected))?;
        let result = self.callback_result(&callback)?;
        let signature = self.engine_signature(&STANDARD, "arrayMapIndexed", &[element, result])?;
        let callback = self.adapt_callback(callback, signature.params[1])?;
        self.engine_apply(
            (&STANDARD, "arrayMapIndexed", &[element, result]),
            vec![values, callback],
            span,
        )
    }

    /// `Object.groupBy(items, callback)` and `Map.groupBy(items, callback)`.
    fn group_by(&mut self, call: &ast::CallExpr, map: bool) -> Result<Expr> {
        let span = call.span;
        let [items, callback] = call.args.as_slice() else {
            return Err(self
                .source
                .locate(invalid("groupBy expects items and a callback"), span));
        };
        let items = self.standard_items(call, &items.expr)?;
        let Type::Array(id) = items.ty else {
            return Err(invalid("missing groupBy element type"));
        };
        let element = self.types.arrays[id as usize];
        let callback = self.standard_callback(&callback.expr, &[element, Type::Float64], None)?;
        let key = self.callback_result(&callback)?;
        if map {
            let signature = self.engine_signature(&COLLECTIONS, "mapGroupBy", &[key, element])?;
            let callback = self.adapt_callback(callback, signature.params[1])?;
            let storage = self.engine_apply(
                (&COLLECTIONS, "mapGroupBy", &[key, element]),
                vec![items, callback],
                span,
            )?;
            let group = arrays::intern(element, self.types)?;
            let ty = collections::intern(K::Map, key, group, self.types)?;
            return Ok(Expr {
                span,
                ty,
                kind: E::Reinterpret(Box::new(storage)),
            });
        }
        if !unions::members(key, self.types)
            .iter()
            .all(|ty| matches!(ty.erased(), Type::String | Type::Float64 | Type::Number))
        {
            return Err(self.source.locate(
                unsupported("Object.groupBy with keys other than strings and numbers"),
                callback.span,
            ));
        }
        let signature = self.engine_signature(&STANDARD, "objectGroupBy", &[element, key])?;
        let callback = self.adapt_callback(callback, signature.params[1])?;
        self.engine_apply(
            (&STANDARD, "objectGroupBy", &[element, key]),
            vec![items, callback],
            span,
        )
    }

    /// `Object.getOwnPropertyNames(object)` (§20.1.2.10) of a record or
    /// dictionary, whose own properties are all enumerable string keys: the
    /// same list as `Object.keys`. Arrays (`length`), strings and class
    /// instances (whose non-enumerable own properties are not modelled) are
    /// refused.
    fn own_property_names(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let span = call.span;
        let [object] = call.args.as_slice() else {
            return Err(self.source.locate(
                invalid("Object.getOwnPropertyNames expects one object"),
                span,
            ));
        };
        let admitted = matches!(stmt::unparen(&object.expr), ast::Expr::Object(_))
            || matches!(
                self.peek_type(&object.expr, 0),
                Some(Type::Record(_) | Type::Dictionary(_))
            );
        if !admitted || object.spread.is_some() {
            return Err(self.source.locate(
                unsupported("Object.getOwnPropertyNames of a value other than a record or dictionary (arrays own a non-enumerable length; class instances and errors own properties that are not modelled)"),
                span,
            ));
        }
        let ast::Callee::Expr(callee) = &call.callee else {
            return Err(invalid("missing Object.getOwnPropertyNames callee"));
        };
        let ast::Expr::Member(member) = stmt::unparen(callee) else {
            return Err(invalid("missing Object.getOwnPropertyNames member"));
        };
        let keys = ast::CallExpr {
            callee: ast::Callee::Expr(Box::new(ast::Expr::Member(ast::MemberExpr {
                prop: ast::MemberProp::Ident(ast::IdentName::new("keys".into(), member.span)),
                ..member.clone()
            }))),
            ..call.clone()
        };
        self.expr(&ast::Expr::Call(keys))
    }

    /// `Object.fromEntries(entries)` of `[key, value]` pairs or a `Map`.
    fn object_from_entries(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let span = call.span;
        let [entries] = call.args.as_slice() else {
            return Err(self.source.locate(
                invalid("Object.fromEntries expects one iterable of entries"),
                span,
            ));
        };
        let entries = match stmt::unparen(&entries.expr) {
            ast::Expr::Array(literal) => self.entry_literals(literal)?,
            expression => self.expr(expression)?,
        };
        let entries = match entries.ty {
            Type::Collection(K::Map, id) => {
                let storage = self.storage(entries);
                let collection = self.types.collections[id as usize];
                self.engine_apply(
                    (
                        &COLLECTIONS,
                        "mapToArray",
                        &[collection.key, collection.value],
                    ),
                    vec![storage],
                    span,
                )?
            }
            _ => entries,
        };
        let pair = match entries.ty {
            Type::Array(id) => self.types.arrays[id as usize],
            ty => {
                return Err(self.source.locate(
                    unsupported(&format!(
                        "Object.fromEntries of {ty} (an array of [key, value] pairs or a Map)"
                    )),
                    entries.span,
                ));
            }
        };
        let Type::Tuple(tuple) = pair else {
            return Err(self.source.locate(
                unsupported(&format!(
                    "Object.fromEntries of {pair} entries (each entry must be a [key, value] tuple)"
                )),
                entries.span,
            ));
        };
        let elements = self.types.tuples[tuple as usize].elements.clone();
        let [key, value] = elements.as_slice() else {
            return Err(self.source.locate(
                unsupported("Object.fromEntries of entries other than [key, value] pairs"),
                entries.span,
            ));
        };
        if !matches!(key.erased(), Type::String | Type::Float64 | Type::Number) {
            return Err(self.source.locate(
                unsupported("Object.fromEntries with keys other than strings and numbers"),
                entries.span,
            ));
        }
        self.engine_apply(
            (&STANDARD, "objectFromEntries", &[*key, *value]),
            vec![entries],
            span,
        )
    }

    /// The items of a `groupBy`: an array as is, any other iterable
    /// collected by `Array.from` first.
    fn standard_items(&mut self, call: &ast::CallExpr, items: &ast::Expr) -> Result<Expr> {
        let array = matches!(stmt::unparen(items), ast::Expr::Array(_))
            || matches!(
                self.peek_type(items, 0),
                Some(Type::Array(_) | Type::Tuple(_))
            );
        if array {
            let value = self.expr(items)?;
            return self.physical_array(value);
        }
        let single = ast::CallExpr {
            args: vec![ast::ExprOrSpread {
                spread: None,
                expr: Box::new(items.clone()),
            }],
            ..call.clone()
        };
        self.array_from_iterable(&single, None)
    }

    /// A callback lowered with `params`; its result is inferred unless
    /// `result` gives it.
    fn standard_callback(
        &mut self,
        expr: &ast::Expr,
        params: &[Type],
        result: Option<Type>,
    ) -> Result<Expr> {
        let value = match stmt::unparen(expr) {
            ast::Expr::Arrow(arrow) => self.arrow_context(
                arrow,
                functions::LiftContext {
                    params: Some(params.to_vec()),
                    shape: None,
                    result,
                    infer: result.is_none(),
                    receiver: None,
                    lexical: true,
                    self_name: None,
                },
            ),
            ast::Expr::Fn(function) => self.function_expression_context(
                function,
                Some(params.to_vec()),
                result,
                None,
                None,
            ),
            expression => self.expr(expression),
        };
        value.map_err(|error| self.source.locate(error, expr.span()))
    }

    /// The result type of a lowered callback.
    fn callback_result(&self, callback: &Expr) -> Result<Type> {
        let Type::Function(signature) = callback.ty else {
            return Err(self.source.locate(
                type_error("callback must be a typed function"),
                callback.span,
            ));
        };
        let result = self.types.functions[signature as usize].result;
        Ok(if result == Type::Void {
            Type::Undefined
        } else {
            result
        })
    }
}

/// The element type `expected` gives an array result.
fn expected_element(lower: &Lower<'_>, expected: Option<Type>) -> Option<Type> {
    match expected {
        Some(Type::Array(id)) => Some(lower.types.arrays[id as usize]),
        _ => None,
    }
}
