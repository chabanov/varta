//! Validate original data before producing boxed JSON graph views.
use super::*;

impl Lower<'_> {
    pub(in crate::lower) fn snapshot_array(
        &mut self,
        value: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let array = arrays::intern(Type::Unknown, self.types)?;
        let index = match self.types.snapshots.array {
            Some(index) => index,
            None => {
                let index = self.reserve_function("too many snapshot array helpers")?;
                self.types.snapshots.array = Some(index);
                self.define_function(index, span, vec![Type::Unknown], array, |child, args| {
                    let cell = args
                        .into_iter()
                        .next()
                        .ok_or_else(|| invalid("snapshot array argument"))?;
                    let raw = child.hidden(array);
                    let at = child.hidden(Type::Float64);
                    let view = || local(span, raw, array);
                    let position = || local(span, at, Type::Float64);
                    let error = child.array_error(
                        varta_common::value::ErrorClass::TypeError,
                        "snapshot requires dense JSON arrays",
                        span,
                    )?;
                    let copied = child.unknown_array(cell.clone())?;
                    Ok(vec![
                        S::Init(
                            raw,
                            Expr {
                                span,
                                ty: array,
                                kind: E::UnboxUnion(Box::new(cell)),
                            },
                        ),
                        S::Init(at, number(span, 0.0)),
                        S::While(
                            binary(
                                span,
                                ast::BinaryOp::Lt,
                                position(),
                                Expr {
                                    span,
                                    ty: Type::Float64,
                                    kind: E::Length(Box::new(view())),
                                },
                                Type::Boolean,
                            ),
                            vec![
                                S::If(
                                    Expr {
                                        span,
                                        ty: Type::Boolean,
                                        kind: E::Not(Box::new(Expr {
                                            span,
                                            ty: Type::Boolean,
                                            kind: E::ArrayHasOwn(
                                                Box::new(view()),
                                                Box::new(position()),
                                            ),
                                        })),
                                    },
                                    vec![S::Throw(error)],
                                    Vec::new(),
                                ),
                                S::Set(
                                    at,
                                    binary(
                                        span,
                                        ast::BinaryOp::Add,
                                        position(),
                                        number(span, 1.0),
                                        Type::Float64,
                                    ),
                                ),
                            ],
                        ),
                        S::Return(Some(copied)),
                    ])
                })?;
                index
            }
        };
        Ok(Expr {
            span,
            ty: array,
            kind: E::Call(index, vec![value]),
        })
    }
    pub(in crate::lower) fn snapshot_object(
        &mut self,
        value: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let graph = dictionaries::intern(Type::Unknown, self.types)?;
        let index = match self.types.snapshots.object {
            Some(index) => index,
            None => {
                let index = self.reserve_function("too many snapshot helpers")?;
                self.types.snapshots.object = Some(index);
                self.define_function(index, span, vec![Type::Unknown], graph, |child, args| {
                    let cell = args
                        .into_iter()
                        .next()
                        .ok_or_else(|| invalid("snapshot object argument"))?;
                    let Type::Dictionary(id) = graph else {
                        return Err(invalid("snapshot dictionary"));
                    };
                    let dictionary = child.types.dictionaries[id as usize];
                    let typed = Expr {
                        span,
                        ty: graph,
                        kind: E::UnboxUnion(Box::new(cell.clone())),
                    };
                    let keys = Expr {
                        span,
                        ty: dictionary.keys,
                        kind: E::Field(Box::new(typed.clone()), dictionary.keys_slot()),
                    };
                    let values = Expr {
                        span,
                        ty: dictionary.values,
                        kind: E::Field(Box::new(typed), dictionary.values_slot()),
                    };
                    // The vector's runtime element kind, rather than this
                    // stand-in dictionary's static value type, selects boxing.
                    let boxed = unknown::widen(values, child.types)?;
                    let values = child.unknown_array(boxed)?;
                    let key_slot = child.hidden(dictionary.keys);
                    let value_slot = child.hidden(dictionary.values);
                    let output = child.hidden(graph);
                    let position = child.hidden(Type::Float64);
                    let at = || local(span, position, Type::Float64);
                    let key = Expr {
                        span,
                        ty: Type::String,
                        kind: E::Index(
                            Box::new(local(span, key_slot, dictionary.keys)),
                            Box::new(at()),
                        ),
                    };
                    let value = Expr {
                        span,
                        ty: Type::Unknown,
                        kind: E::Index(
                            Box::new(local(span, value_slot, dictionary.values)),
                            Box::new(at()),
                        ),
                    };
                    let stored = child.dictionary_helper_call(
                        ir::DictionaryHelper::Define,
                        vec![local(span, output, graph), key, value],
                    )?;
                    let copy = vec![
                        S::Init(key_slot, keys),
                        S::Init(value_slot, values),
                        S::Init(output, child.snapshot_empty_graph(span)?),
                        S::Init(position, number(span, 0.0)),
                        S::While(
                            binary(
                                span,
                                ast::BinaryOp::Lt,
                                at(),
                                Expr {
                                    span,
                                    ty: Type::Float64,
                                    kind: E::Length(Box::new(local(
                                        span,
                                        key_slot,
                                        dictionary.keys,
                                    ))),
                                },
                                Type::Boolean,
                            ),
                            vec![
                                S::If(
                                    Expr {
                                        span,
                                        ty: Type::Boolean,
                                        kind: E::ArrayHasOwn(
                                            Box::new(local(span, key_slot, dictionary.keys)),
                                            Box::new(at()),
                                        ),
                                    },
                                    vec![S::Expr(stored)],
                                    Vec::new(),
                                ),
                                S::Set(
                                    position,
                                    binary(
                                        span,
                                        ast::BinaryOp::Add,
                                        at(),
                                        number(span, 1.0),
                                        Type::Float64,
                                    ),
                                ),
                            ],
                        ),
                        S::Return(Some(local(span, output, graph))),
                    ];
                    let dictionary_test = Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsUnionMember(Box::new(cell.clone()), graph),
                    };
                    let any = records::intern(Vec::new(), child.types)?;
                    let record_test = Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsUnionMember(Box::new(cell.clone()), any),
                    };
                    let record = child.plain_record_graph(cell, span)?;
                    let error = child.array_error(
                        varta_common::value::ErrorClass::TypeError,
                        "snapshot requires plain JSON objects",
                        span,
                    )?;
                    Ok(vec![
                        S::If(dictionary_test, copy, Vec::new()),
                        S::If(record_test, vec![S::Return(Some(record))], Vec::new()),
                        S::Throw(error),
                    ])
                })?;
                index
            }
        };
        Ok(Expr {
            span,
            ty: graph,
            kind: E::Call(index, vec![value]),
        })
    }
}
