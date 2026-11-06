//! The iteration protocol (v38 `iteration_protocol`). A class becomes
//! iterable by declaring `[Symbol.iterator]()` as a method without
//! parameters that returns a synchronous generator object; `for...of`, array
//! literal spreads, `Array.from` and `yield*` call it once and iterate the
//! generator it returns. Array literals spread checked arrays (and tuples),
//! strings (code points), synchronous generators, `Map`/`Set`/`Uint8Array` values and their
//! iterators, and iterable class instances, each iterated to completion
//! where it appears in source order. Iterator objects other than generators
//! (a hand-written `{ next() {...} }`) are not modelled.
use super::*;
use ir::GeneratorOutcome as Outcome;
use varta_common::function::frame::Completion;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn number(span: swc_common::Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn undefined(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}
fn rejected(what: &str, detail: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: what.into(),
        detail: detail.into(),
    }
}

/// One entry of an array literal or `Array.from` source, lowered in source
/// order: a single element or an iterated source.
enum Segment {
    Element(Expr),
    Spread(Iterated),
}

/// An iterated source after normalization.
enum Iterated {
    /// A checked array; `packed` storage (a tuple or a collection's fresh
    /// snapshot) has no holes.
    Array { value: Expr, packed: bool },
    /// A synchronous generator object.
    Generator(Expr),
}
impl Iterated {
    fn element(&self, types: &ir::ValueTypes) -> Result<Type> {
        match self {
            Self::Array { value, .. } => match value.ty {
                Type::Array(id) => Ok(types.arrays[id as usize]),
                _ => Err(CompilerError::Internal("missing spread array type".into())),
            },
            Self::Generator(value) => match value.ty {
                Type::Generator(id) => Ok(types.generators[id as usize].yielded),
                _ => Err(CompilerError::Internal(
                    "missing spread generator type".into(),
                )),
            },
        }
    }
}

impl Lower<'_> {
    /// `value[Symbol.iterator]()` for an instance of a class that declares
    /// (or inherits) the method; any other value is returned unchanged. A
    /// class instance without the method fails naming `what`.
    pub(super) fn iterable_instance(&mut self, value: Expr, what: &str) -> Result<Expr> {
        let Type::Class(class) = value.ty else {
            return Ok(value);
        };
        if !self.types.profile.iteration_protocol() {
            return Ok(value);
        }
        let span = value.span;
        let kind = format!("{what} over a class instance");
        // A generic class instance whose type first appeared in a signature
        // has no member functions allocated yet (see `async_iterator`).
        if self
            .types
            .classes
            .get(class)
            .methods
            .get(classes::ITERATOR_METHOD)
            .is_some_and(|method| method.target().0 == classes::UNALLOCATED)
        {
            self.class_functions()?;
        }
        let Some(method) = self
            .types
            .classes
            .get(class)
            .methods
            .get(classes::ITERATOR_METHOD)
            .cloned()
        else {
            return Err(self.source.locate(
                rejected(&kind, "the class declares no [Symbol.iterator]() method"),
                span,
            ));
        };
        let synchronous = matches!(
            method.signature.result,
            Type::Generator(id) if self.types.generators[id as usize].asynchronous.is_none()
        );
        if !synchronous || !method.signature.params.is_empty() || method.getter {
            return Err(self.source.locate(
                rejected(
                    &kind,
                    "[Symbol.iterator]() must be a method without parameters returning a synchronous generator",
                ),
                span,
            ));
        }
        let (target, receiver) = method.target();
        let receiver = Expr {
            ty: Type::Class(receiver),
            ..value
        };
        Ok(Expr {
            span,
            ty: method.signature.result,
            kind: E::Call(target, vec![receiver]),
        })
    }

    /// Normalize an iterated value as `for...of` does: a `RegExp` match
    /// array, an iterable class instance, a collection, a tuple and a string
    /// (its code points) become a generator or a checked array.
    fn iterated(&mut self, value: Expr, what: &str) -> Result<Iterated> {
        let span = value.span;
        let value = self.match_array_values(value)?;
        let value = self.iterable_instance(value, what)?;
        let (value, packed) = self.collection_iteration(value)?;
        let packed = packed || matches!(value.ty, Type::Tuple(_));
        let value = self.physical_array(value)?;
        match value.ty {
            Type::Array(_) => Ok(Iterated::Array { value, packed }),
            Type::Generator(id) => {
                let generator = self.types.generators[id as usize];
                if generator.asynchronous.is_some() {
                    return Err(self.source.locate(
                        rejected(what, "an async generator is not iterable synchronously"),
                        span,
                    ));
                }
                if !generators::accepts_undefined(generator.next, self.types) {
                    return Err(self.source.locate(
                        type_error(format!(
                            "{what} over a generator requires TNext to accept undefined"
                        )),
                        span,
                    ));
                }
                Ok(Iterated::Generator(value))
            }
            // A string iterates its code points, as `for...of` does; the
            // string is immutable, so its code points are read up front.
            Type::String => Ok(Iterated::Array {
                value: self.code_points(value)?,
                packed: true,
            }),
            other => Err(self.source.locate(
                type_error(format!(
                    "{what} requires an array, a generator, a Map, Set or Uint8Array or an iterable class instance, not {other}"
                )),
                span,
            )),
        }
    }

    /// A fresh `string[]` of the code points of the string `value`, as the
    /// String iterator yields them: a high surrogate followed by a low one
    /// is one code point, any other code unit is one, lone surrogates
    /// included.
    fn code_points(&mut self, value: Expr) -> Result<Expr> {
        use string_iteration::{HIGH_FIRST, HIGH_LAST, LOW_FIRST, LOW_LAST};
        let span = value.span;
        let array = arrays::intern(Type::String, self.types)?;
        let text = self.hidden(Type::String);
        let output = self.hidden(array);
        let index = self.hidden(Type::Float64);
        let width = self.hidden(Type::Float64);
        let read = |text: u32| local(span, text, Type::String);
        let float = |slot: u32| local(span, slot, Type::Float64);
        let binary = |op: ast::BinaryOp, a: Expr, b: Expr, ty: Type| Expr {
            span,
            ty,
            kind: E::Binary(op, Box::new(a), Box::new(b)),
        };
        let unit = |offset: f64| Expr {
            span,
            ty: Type::Float64,
            kind: E::Intrinsic(
                ir::Intrinsic::CharCodeAt,
                vec![
                    read(text),
                    binary(
                        ast::BinaryOp::Add,
                        float(index),
                        number(span, offset),
                        Type::Float64,
                    ),
                ],
            ),
        };
        let between = |offset: f64, low: f64, high: f64| {
            binary(
                ast::BinaryOp::LogicalAnd,
                binary(
                    ast::BinaryOp::GtEq,
                    unit(offset),
                    number(span, low),
                    Type::Boolean,
                ),
                binary(
                    ast::BinaryOp::LtEq,
                    unit(offset),
                    number(span, high),
                    Type::Boolean,
                ),
                Type::Boolean,
            )
        };
        let length = Expr {
            span,
            ty: Type::Float64,
            kind: E::Length(Box::new(read(text))),
        };
        let pair = binary(
            ast::BinaryOp::LogicalAnd,
            binary(
                ast::BinaryOp::Lt,
                binary(
                    ast::BinaryOp::Add,
                    float(index),
                    number(span, 1.0),
                    Type::Float64,
                ),
                length.clone(),
                Type::Boolean,
            ),
            binary(
                ast::BinaryOp::LogicalAnd,
                between(0.0, HIGH_FIRST, HIGH_LAST),
                between(1.0, LOW_FIRST, LOW_LAST),
                Type::Boolean,
            ),
            Type::Boolean,
        );
        let end = binary(
            ast::BinaryOp::Add,
            float(index),
            float(width),
            Type::Float64,
        );
        let point = Expr {
            span,
            ty: Type::String,
            kind: E::StringRange {
                value: Box::new(read(text)),
                start: Box::new(float(index)),
                end: Box::new(end.clone()),
                substring: false,
            },
        };
        let push = self.pushed(output, array, Type::String, point)?;
        let body = vec![
            S::Init(text, value),
            S::Init(
                output,
                Expr {
                    span,
                    ty: array,
                    kind: E::Array(Vec::new()),
                },
            ),
            S::Init(index, number(span, 0.0)),
            S::Init(width, number(span, 1.0)),
            S::While(
                binary(ast::BinaryOp::Lt, float(index), length, Type::Boolean),
                vec![
                    S::Set(width, number(span, 1.0)),
                    S::If(pair, vec![S::Set(width, number(span, 2.0))], vec![]),
                    push,
                    S::Set(index, end),
                ],
            ),
        ];
        Ok(Expr {
            span,
            ty: array,
            kind: E::ValueBlock {
                output,
                marker: self.hidden(Type::Number),
                roots: vec![text, output],
                body,
            },
        })
    }

    /// An array literal with at least one spread element (v38): every
    /// element and spread source evaluates in source order and each source
    /// is iterated to completion before the next element is evaluated.
    /// `None` when the literal has no spread or the profile does not admit
    /// spreads here.
    pub(super) fn spread_literal(
        &mut self,
        array: &ast::ArrayLit,
        expected: Option<Type>,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.iteration_protocol()
            || !array
                .elems
                .iter()
                .flatten()
                .any(|entry| entry.spread.is_some())
        {
            return Ok(None);
        }
        if array.elems.len() > self.types.profile.data().array_elements {
            return Err(invalid("array element count exceeds limit"));
        }
        let context = match expected {
            Some(Type::Array(id)) => Some(self.types.arrays[id as usize]),
            _ => None,
        };
        let mut segments = Vec::new();
        for entry in &array.elems {
            let Some(entry) = entry else {
                return Err(self.source.locate(
                    unsupported("array literal hole beside a spread element"),
                    array.span,
                ));
            };
            if entry.spread.is_some() {
                let value = self.expr(&entry.expr)?;
                segments.push(Segment::Spread(self.iterated(value, "array spread")?));
            } else {
                segments.push(Segment::Element(self.expr_expected(&entry.expr, context)?));
            }
        }
        self.accumulate(array.span, segments, context).map(Some)
    }

    /// `Array.from(source)` of an iterable (v38): the same fresh array as
    /// `[...source]`. A map function argument is not admitted.
    pub(super) fn array_from_iterable(
        &mut self,
        call: &ast::CallExpr,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let [source] = call.args.as_slice() else {
            return Err(self.source.locate(
                unsupported("Array.from with a map function or arguments other than one iterable"),
                call.span,
            ));
        };
        if source.spread.is_some() {
            return Err(self
                .source
                .locate(unsupported("Array.from with a spread argument"), call.span));
        }
        let context = match expected {
            Some(Type::Array(id)) => Some(self.types.arrays[id as usize]),
            _ => None,
        };
        let value = self.expr(&source.expr)?;
        let source = self.iterated(value, "Array.from")?;
        self.accumulate(call.span, vec![Segment::Spread(source)], context)
    }

    /// The element type of the accumulated array: the context's, or the
    /// union of every element and source element type.
    fn accumulated_element(&mut self, segments: &[Segment], context: Option<Type>) -> Result<Type> {
        if let Some(element) = context {
            return Ok(element);
        }
        let mut members = Vec::new();
        for segment in segments {
            let ty = match segment {
                Segment::Element(value) => value.ty,
                Segment::Spread(source) => source.element(self.types)?,
            };
            for member in unions::members(ty, self.types) {
                if !members.contains(&member) {
                    members.push(member);
                }
            }
        }
        match members.as_slice() {
            [] => Err(invalid("empty array requires an element type annotation")),
            [single] => Ok(*single),
            _ => unions::intern(members, self.types),
        }
    }

    fn accumulate(
        &mut self,
        span: swc_common::Span,
        segments: Vec<Segment>,
        context: Option<Type>,
    ) -> Result<Expr> {
        let element = self.accumulated_element(&segments, context)?;
        let array = arrays::intern(element, self.types).map_err(|e| self.source.locate(e, span))?;
        let output = self.hidden(array);
        let marker = self.hidden(Type::Number);
        let mut roots = vec![output];
        let mut body = vec![S::Init(
            output,
            Expr {
                span,
                ty: array,
                kind: E::Array(Vec::new()),
            },
        )];
        for segment in segments {
            match segment {
                Segment::Element(value) => {
                    let pushed = self.pushed(output, array, element, value)?;
                    body.push(pushed);
                }
                Segment::Spread(Iterated::Array { value, packed }) => {
                    body.extend(
                        self.append_array(output, array, element, value, packed, &mut roots)?,
                    );
                }
                Segment::Spread(Iterated::Generator(value)) => {
                    body.extend(self.append_generator(output, array, element, value, &mut roots)?);
                }
            }
        }
        Ok(Expr {
            span,
            ty: array,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }

    /// `output.push(value)`, widening `value` to the element type.
    fn pushed(&mut self, output: u32, array: Type, element: Type, value: Expr) -> Result<S> {
        let span = value.span;
        let found = value.ty;
        let value = unions::widen(value, Some(element), self.types)
            .ok()
            .filter(|value| value.ty == element)
            .ok_or_else(|| {
                self.source.locate(
                    type_error(format!(
                        "array element type mismatch: expected {element}, found {found}"
                    )),
                    span,
                )
            })?;
        Ok(S::Expr(Expr {
            span,
            ty: Type::Float64,
            kind: E::ArrayPush(
                Box::new(local(span, output, array)),
                Box::new(Expr {
                    span,
                    ty: array,
                    kind: E::Array(vec![Some(value)]),
                }),
            ),
        }))
    }

    /// Push every element of the array `value`, read like `for...of` reads
    /// it: a hole of an element type without `undefined` is a `TypeError`.
    fn append_array(
        &mut self,
        output: u32,
        array: Type,
        element: Type,
        value: Expr,
        packed: bool,
        roots: &mut Vec<u32>,
    ) -> Result<Vec<S>> {
        let span = value.span;
        let source_ty = value.ty;
        let Type::Array(id) = source_ty else {
            return Err(CompilerError::Internal("missing spread array type".into()));
        };
        let item = self.types.arrays[id as usize];
        let source = self.hidden(source_ty);
        let index = self.hidden(Type::Float64);
        roots.push(source);
        let sparse = self.types.profile.sparse_arrays() && !packed;
        let read = if sparse {
            let maybe = Expr {
                span,
                ty: unions::intern(vec![item, Type::Undefined], self.types)?,
                kind: E::MaybeIndex(
                    Box::new(local(span, source, source_ty)),
                    Box::new(local(span, index, Type::Float64)),
                ),
            };
            self.dense_read(maybe, item, dense::ITERATED_HOLE)?
        } else {
            Expr {
                span,
                ty: item,
                kind: E::Index(
                    Box::new(local(span, source, source_ty)),
                    Box::new(local(span, index, Type::Float64)),
                ),
            }
        };
        let push = self.pushed(output, array, element, read)?;
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Binary(
                ast::BinaryOp::Lt,
                Box::new(local(span, index, Type::Float64)),
                Box::new(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Length(Box::new(local(span, source, source_ty))),
                }),
            ),
        };
        let next = S::Set(
            index,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Binary(
                    ast::BinaryOp::Add,
                    Box::new(local(span, index, Type::Float64)),
                    Box::new(number(span, 1.0)),
                ),
            },
        );
        Ok(vec![
            S::Init(source, value),
            S::Init(index, number(span, 0.0)),
            S::While(test, vec![push, next]),
        ])
    }

    /// Resume the generator `value` with `undefined` until it completes,
    /// pushing every yielded value. Like a spread in JavaScript, nothing
    /// closes the generator: no abrupt exit can leave the loop early.
    fn append_generator(
        &mut self,
        output: u32,
        array: Type,
        element: Type,
        value: Expr,
        roots: &mut Vec<u32>,
    ) -> Result<Vec<S>> {
        let span = value.span;
        let ty = value.ty;
        let Type::Generator(id) = ty else {
            return Err(CompilerError::Internal(
                "missing spread generator type".into(),
            ));
        };
        let generator = self.types.generators[id as usize];
        let iterator = self.hidden(ty);
        let outcome = self.hidden(Type::Number);
        roots.push(iterator);
        let input = unions::widen(undefined(span), Some(generator.next), self.types)?;
        let mut step = self.generator_step(
            span,
            local(span, iterator, ty),
            Completion::Normal,
            input,
            outcome,
        )?;
        let yielded = Expr {
            span,
            ty: generator.yielded,
            kind: E::GeneratorValue(Box::new(local(span, iterator, ty))),
        };
        step.push(S::If(
            generators::is(span, outcome, Outcome::Yielded),
            vec![],
            vec![S::Break],
        ));
        step.push(self.pushed(output, array, element, yielded)?);
        Ok(vec![
            S::Init(iterator, value),
            S::While(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(1),
                },
                step,
            ),
        ])
    }
}

/// V8's `TypeError` message when `yield*` forwards a `throw` to an iterator
/// without a `throw` method (an array's).
const NO_THROW_METHOD: &str = "The iterator does not provide a 'throw' method.";

fn integer(span: swc_common::Span, value: i32) -> Expr {
    Expr {
        span,
        ty: Type::Number,
        kind: E::Integer(value),
    }
}
fn zero(span: swc_common::Span, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Zero,
    }
}
fn kind_is(span: swc_common::Span, slot: u32, completion: Completion) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Binary(
            ast::BinaryOp::EqEqEq,
            Box::new(local(span, slot, Type::Number)),
            Box::new(integer(span, completion as i32)),
        ),
    }
}
/// The value a `void` `TReturn` stands for.
fn stored(ty: Type) -> Type {
    if ty == Type::Void {
        Type::Undefined
    } else {
        ty
    }
}

/// The locals one `yield*` keeps across its suspensions.
struct Delegation {
    observer: ir::SuspendObserver,
    outer: ir::GeneratorType,
}

impl Lower<'_> {
    /// `yield* source` in a synchronous generator body (v38
    /// `iteration_protocol`) over a generator, an array or an iterable
    /// class instance. Each value the source produces is yielded as it is;
    /// a `next(v)`, `throw(e)` or `return(v)` of this generator while it is
    /// suspended there is forwarded to a generator source (an array has no
    /// `throw` or `return`: a throw becomes V8's `TypeError` and a return
    /// returns from this generator). The expression is the source
    /// generator's return value (`undefined` for an array).
    pub(super) fn delegate(
        &mut self,
        expression: &ast::YieldExpr,
        outer: ir::GeneratorType,
    ) -> Result<Expr> {
        let arg = expression
            .arg
            .as_deref()
            .ok_or_else(|| invalid("yield* requires an operand"))?;
        let value = self.expr(arg)?;
        let source = self.iterated(value, "yield*")?;
        let span = expression.span;
        let exception = self.exception_type()?;
        let returned_ty = stored(outer.returned);
        let delegation = Delegation {
            observer: ir::SuspendObserver {
                kind: self.hidden(Type::Number),
                thrown: self.hidden(exception),
                returned: self.hidden(returned_ty),
            },
            outer,
        };
        let observer = delegation.observer;
        let mut body = vec![
            S::Init(observer.kind, integer(span, Completion::Normal as i32)),
            S::Init(observer.thrown, zero(span, exception)),
            S::Init(observer.returned, zero(span, returned_ty)),
        ];
        let mut roots = vec![observer.thrown, observer.returned];
        let (output, loop_body) = match source {
            Iterated::Array { value, packed } => {
                self.delegate_array(span, &delegation, value, packed, &mut body, &mut roots)?
            }
            Iterated::Generator(value) => {
                self.delegate_generator(span, &delegation, value, &mut body, &mut roots)?
            }
        };
        body.push(loop_body);
        roots.push(output.0);
        Ok(Expr {
            span,
            ty: output.1,
            kind: E::ValueBlock {
                output: output.0,
                marker: self.hidden(Type::Number),
                roots,
                body,
            },
        })
    }

    /// The suspension of one delegated value: yield `value` (widened to this
    /// generator's `T`) and record how the generator was resumed; a normal
    /// resumption's value goes to `sent` (converted to its type) when given.
    fn delegated_yield(
        &mut self,
        span: swc_common::Span,
        delegation: &Delegation,
        value: Expr,
        sent: Option<(u32, Type)>,
    ) -> Result<Vec<S>> {
        let yielded = delegation.outer.yielded;
        let found = value.ty;
        let value = unions::widen(value, Some(yielded), self.types)
            .ok()
            .filter(|value| value.ty == yielded)
            .ok_or_else(|| {
                self.source.locate(
                    type_error(format!(
                        "yield* source yields {found}, which this generator cannot yield as {yielded}"
                    )),
                    span,
                )
            })?;
        let next = delegation.outer.next;
        let input = self.hidden(next);
        let mut code = vec![S::Init(
            input,
            Expr {
                span,
                ty: next,
                kind: E::Suspend {
                    value: Box::new(value),
                    state: 0,
                    observe: Some(delegation.observer),
                },
            },
        )];
        if let Some((slot, ty)) = sent {
            let received = unions::widen(local(span, input, next), Some(ty), self.types)
                .ok()
                .filter(|value| value.ty == ty)
                .ok_or_else(|| {
                    self.source.locate(
                        type_error(format!(
                            "yield* forwards what this generator receives from next() ({next}) to a source that receives {ty}"
                        )),
                        span,
                    )
                })?;
            code.push(S::Set(slot, received));
        }
        Ok(code)
    }

    /// `return value` from this generator body (`return;` for `void`).
    fn delegated_return(&self, value: Expr) -> S {
        S::Return((self.result != Type::Void).then_some(value))
    }

    /// `yield*` over an array: its elements, read like `for...of` reads
    /// them, at the live length. A forwarded `throw` closes nothing (array
    /// iterators have no `return`) and throws V8's `TypeError`; a forwarded
    /// `return` returns its value from this generator.
    fn delegate_array(
        &mut self,
        span: swc_common::Span,
        delegation: &Delegation,
        value: Expr,
        packed: bool,
        body: &mut Vec<S>,
        roots: &mut Vec<u32>,
    ) -> Result<((u32, Type), S)> {
        let source_ty = value.ty;
        let Type::Array(id) = source_ty else {
            return Err(CompilerError::Internal("missing yield* array type".into()));
        };
        let item = self.types.arrays[id as usize];
        let source = self.hidden(source_ty);
        let index = self.hidden(Type::Float64);
        let output = self.hidden(Type::Undefined);
        roots.push(source);
        body.extend([
            S::Init(source, value),
            S::Init(index, number(span, 0.0)),
            S::Init(output, undefined(span)),
        ]);
        let read = if self.types.profile.sparse_arrays() && !packed {
            let maybe = Expr {
                span,
                ty: unions::intern(vec![item, Type::Undefined], self.types)?,
                kind: E::MaybeIndex(
                    Box::new(local(span, source, source_ty)),
                    Box::new(local(span, index, Type::Float64)),
                ),
            };
            self.dense_read(maybe, item, dense::ITERATED_HOLE)?
        } else {
            Expr {
                span,
                ty: item,
                kind: E::Index(
                    Box::new(local(span, source, source_ty)),
                    Box::new(local(span, index, Type::Float64)),
                ),
            }
        };
        let observer = delegation.observer;
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            NO_THROW_METHOD,
            span,
        )?;
        let returned = local(span, observer.returned, stored(delegation.outer.returned));
        let mut step = self.delegated_yield(span, delegation, read, None)?;
        step.extend([
            S::If(
                kind_is(span, observer.kind, Completion::Throw),
                vec![S::Throw(error)],
                vec![],
            ),
            S::If(
                kind_is(span, observer.kind, Completion::Return),
                vec![self.delegated_return(returned)],
                vec![],
            ),
            S::Set(
                index,
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Binary(
                        ast::BinaryOp::Add,
                        Box::new(local(span, index, Type::Float64)),
                        Box::new(number(span, 1.0)),
                    ),
                },
            ),
        ]);
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Binary(
                ast::BinaryOp::Lt,
                Box::new(local(span, index, Type::Float64)),
                Box::new(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Length(Box::new(local(span, source, source_ty))),
                }),
            ),
        };
        Ok(((output, Type::Undefined), S::While(test, step)))
    }

    /// `yield*` over a generator: resume it with what this generator
    /// receives (`next`, `throw` or `return`) until it completes.
    fn delegate_generator(
        &mut self,
        span: swc_common::Span,
        delegation: &Delegation,
        value: Expr,
        body: &mut Vec<S>,
        roots: &mut Vec<u32>,
    ) -> Result<((u32, Type), S)> {
        let ty = value.ty;
        let Type::Generator(id) = ty else {
            return Err(CompilerError::Internal(
                "missing yield* generator type".into(),
            ));
        };
        let inner = self.types.generators[id as usize];
        let outer = delegation.outer;
        let observer = delegation.observer;
        let outer_returned = stored(outer.returned);
        let inner_returned = stored(inner.returned);
        let convert = |lower: &mut Self, value: Expr, to: Type, what: &str| -> Result<Expr> {
            let from = value.ty;
            unions::widen(value, Some(to), lower.types)
                .ok()
                .filter(|value| value.ty == to)
                .ok_or_else(|| {
                    lower.source.locate(
                        type_error(format!("yield* {what}: {from} does not convert to {to}")),
                        span,
                    )
                })
        };
        let iterator = self.hidden(ty);
        let outcome = self.hidden(Type::Number);
        let output = self.hidden(inner_returned);
        // What the source's next resumption receives: `undefined` first.
        let sent = self.hidden(inner.next);
        roots.extend([iterator, sent]);
        let first = convert(
            self,
            undefined(span),
            inner.next,
            "starts its source with next(undefined), so the source's TNext must accept undefined",
        )?;
        body.extend([
            S::Init(iterator, value),
            S::Init(outcome, integer(span, 0)),
            S::Init(output, zero(span, inner_returned)),
            S::Init(sent, first),
        ]);
        let it = || local(span, iterator, ty);
        let sending = local(span, sent, inner.next);
        let thrown = local(span, observer.thrown, self.exception_type()?);
        // A generator whose `TReturn` is `void` (or `undefined`) returns
        // `undefined` after a forwarded `return()`: when the source's
        // `TReturn` excludes `undefined`, the source is closed with a
        // placeholder it cannot observe (a `finally` block never reads the
        // pending value, and a completed generator answers `undefined`).
        let discard = outer_returned == Type::Undefined;
        let forwarded = match convert(
            self,
            local(span, observer.returned, outer_returned),
            inner_returned,
            "forwards what this generator receives from return() to its source",
        ) {
            Ok(value) => value,
            Err(_) if discard => zero(span, inner_returned),
            Err(error) => return Err(error),
        };
        let completed = if inner.returned == Type::Void {
            undefined(span)
        } else {
            Expr {
                span,
                ty: inner.returned,
                kind: E::GeneratorValue(Box::new(it())),
            }
        };
        let returned_from_source = match convert(
            self,
            completed.clone(),
            outer_returned,
            "returns its source's return value from this generator after return()",
        ) {
            Ok(value) => value,
            Err(_) if discard => undefined(span),
            Err(error) => return Err(error),
        };
        let echoed = local(span, observer.returned, outer_returned);
        let normal = self.generator_step(span, it(), Completion::Normal, sending, outcome)?;
        let mut throwing =
            self.generator_step(span, it(), Completion::Throw, thrown.clone(), outcome)?;
        throwing.push(S::If(
            generators::is(span, outcome, Outcome::ThrowInput),
            vec![S::Throw(thrown)],
            vec![],
        ));
        let mut returning =
            self.generator_step(span, it(), Completion::Return, forwarded, outcome)?;
        returning.extend([
            S::If(
                generators::is(span, outcome, Outcome::Returned),
                vec![self.delegated_return(returned_from_source)],
                vec![],
            ),
            S::If(
                generators::is(span, outcome, Outcome::DoneInput),
                vec![self.delegated_return(echoed)],
                vec![],
            ),
        ]);
        let resumed = S::If(
            kind_is(span, observer.kind, Completion::Normal),
            normal,
            vec![S::If(
                kind_is(span, observer.kind, Completion::Throw),
                throwing,
                returning,
            )],
        );
        let yielded = Expr {
            span,
            ty: inner.yielded,
            kind: E::GeneratorValue(Box::new(it())),
        };
        let suspend = self.delegated_yield(span, delegation, yielded, Some((sent, inner.next)))?;
        let finished = vec![
            S::If(
                generators::is(span, outcome, Outcome::Returned),
                vec![S::Set(output, completed)],
                vec![],
            ),
            S::Break,
        ];
        let step = vec![
            resumed,
            S::If(
                generators::is(span, outcome, Outcome::Yielded),
                suspend,
                finished,
            ),
        ];
        Ok((
            (output, inner_returned),
            S::While(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(1),
                },
                step,
            ),
        ))
    }
}
