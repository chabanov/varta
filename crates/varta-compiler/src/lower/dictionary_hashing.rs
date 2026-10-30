//! Hashed dictionaries and mixed records (v38 `dictionary_hashing`, value
//! layout 18).
//!
//! The storage record `{ holes: number; keys: string[]; table: number[];
//! values: T[] }` keeps the entries in own-key order in its two vectors,
//! exactly as value15 does, and adds an open-addressing table of entry
//! positions (layout in `varta_common::value::HASHED_DICTIONARY_FIELDS`):
//!
//! - **Unindexed** (`table` is the zero handle): dense vectors searched
//!   linearly. Literals, spreads, conversions and checked parsing create
//!   dictionaries unindexed, so small objects never pay for a table.
//! - **Indexed**: a `number[]` of a power-of-two capacity whose holes are
//!   empty slots, present `+0` slots tombstones and present `p + 1` slots
//!   references to entry position `p`, probed linearly from the key's FNV-1a
//!   hash by the allocation-free guest helpers behind
//!   `ir::Intrinsic::DictionaryFind`, `DictionaryUnlink` and
//!   `DictionaryPlace`. The first keyed access of a dictionary with more
//!   than `value::DICTIONARY_LINEAR_ENTRIES` entries indexes it.
//!
//! An insertion of a named key, or of an array-index key above the last key,
//! appends and places one slot. When one more entry would pass half the
//! capacity, the holes are dropped and the table rebuilt for three slots per
//! entry, so rebuilds are amortized and probes stay short. A deletion of an
//! indexed dictionary tombstones its slot, leaves a hole in both vectors and
//! counts it in `holes`. Every reader of the vectors first drops the holes
//! (`DictionaryHelper::Compact`, which leaves the dictionary unindexed until
//! its next keyed access), so enumeration, spreads and conversions see dense,
//! own-key-ordered entries. An array-index key below an existing key is
//! inserted at its own-key position after the same compaction, shifting the
//! tail (linear, as in value15).
use super::*;
use dictionaries::Block;
use ir::{DictionaryHelper, DictionaryType};
use varta_common::value::{DICTIONARY_LINEAR_ENTRIES, DICTIONARY_TABLE_MIN, ErrorClass};

/// A rebuild sizes the table for this many slots per entry, so at least half
/// as many entries again can be appended before the load passes one half.
const SLOTS_PER_ENTRY: f64 = 3.0;
const GROWTH: f64 = 2.0;
/// The load bound: entries (live and deleted) at most half the slots.
const SLOTS_PER_ENTRY_AT_LOAD: f64 = 2.0;

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
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn binary(op: ast::BinaryOp, left: Expr, right: Expr) -> Expr {
    let ty = match op {
        ast::BinaryOp::Add
        | ast::BinaryOp::Sub
        | ast::BinaryOp::Mul
        | ast::BinaryOp::ZeroFillRShift => Type::Float64,
        _ => Type::Boolean,
    };
    Expr {
        span: left.span,
        ty,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}
fn length(array: Expr) -> Expr {
    Expr {
        span: array.span,
        ty: Type::Float64,
        kind: E::Length(Box::new(array)),
    }
}
fn element(array: Expr, index: Expr, ty: Type) -> Expr {
    Expr {
        span: array.span,
        ty,
        kind: E::Index(Box::new(array), Box::new(index)),
    }
}
fn present(array: Expr, index: Expr) -> Expr {
    Expr {
        span: array.span,
        ty: Type::Boolean,
        kind: E::ArrayHasOwn(Box::new(array), Box::new(index)),
    }
}
fn index_key(key: Expr) -> Expr {
    Expr {
        span: key.span,
        ty: Type::Float64,
        kind: E::IndexKey(Box::new(key)),
    }
}
fn zero(span: swc_common::Span, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Zero,
    }
}
fn field(object: Expr, slot: u32, ty: Type) -> Expr {
    Expr {
        span: object.span,
        ty,
        kind: E::Field(Box::new(object), slot),
    }
}
fn increment(span: swc_common::Span, slot: u32) -> S {
    S::Set(
        slot,
        binary(
            ast::BinaryOp::Add,
            local(span, slot, Type::Float64),
            number(span, 1.0),
        ),
    )
}
/// Reference entry `position` from the first empty slot of `key`'s probe.
fn place(table: Expr, key: Expr, position: Expr) -> S {
    S::Expr(Expr {
        span: table.span,
        ty: Type::Float64,
        kind: E::Intrinsic(ir::Intrinsic::DictionaryPlace, vec![table, key, position]),
    })
}
fn block(
    span: swc_common::Span,
    output: u32,
    marker: u32,
    roots: Vec<u32>,
    body: Vec<S>,
    ty: Type,
) -> Expr {
    Expr {
        span,
        ty,
        kind: E::ValueBlock {
            output,
            marker,
            roots,
            body,
        },
    }
}

/// The value18 parts of one dictionary type: its vector and table types and
/// canonical slots.
#[derive(Clone, Copy)]
pub(super) struct Hashed {
    pub(super) keys: Type,
    pub(super) values: Type,
    pub(super) table: Type,
    pub(super) value: Type,
    pub(super) keys_slot: u32,
    pub(super) values_slot: u32,
    pub(super) table_slot: u32,
    pub(super) holes_slot: u32,
}
impl Hashed {
    pub(super) fn of(dictionary: DictionaryType) -> Option<Self> {
        Some(Self {
            keys: dictionary.keys,
            values: dictionary.values,
            table: dictionary.table?,
            value: dictionary.value,
            keys_slot: dictionary.keys_slot(),
            values_slot: dictionary.values_slot(),
            table_slot: dictionary.table_slot()?,
            holes_slot: dictionary.holes_slot()?,
        })
    }
}

impl Lower<'_> {
    /// `array[array.length] = item`: an indexed extension, which grows the
    /// vector geometrically without the argument array of `push`.
    fn append(&mut self, array: Expr, item: Expr) -> S {
        let at = length(array.clone());
        self.set_index(array, at, item)
    }
    /// A fresh dictionary of type `ty` holding the `keys` and `values`
    /// vectors, unindexed under value18 (no holes, the zero table handle).
    pub(super) fn dictionary_storage(
        &self,
        span: swc_common::Span,
        ty: Type,
        keys: Expr,
        values: Expr,
    ) -> Result<Expr> {
        let Type::Dictionary(id) = ty else {
            return Err(invalid("missing dictionary type"));
        };
        let dictionary = self.types.dictionaries[id as usize];
        Ok(Expr {
            span,
            ty,
            kind: E::Record(storage_fields(span, dictionary, keys, values)),
        })
    }
    /// The statement that drops the holes of a value18 dictionary held by
    /// `value` (a rooted local read) before its vectors are read directly;
    /// `None` before value18, whose vectors are always dense.
    pub(super) fn dictionary_compact(&mut self, value: Expr) -> Result<Option<S>> {
        let Type::Dictionary(id) = value.ty else {
            return Err(invalid("missing dictionary type"));
        };
        if self.types.dictionaries[id as usize].table.is_none() {
            return Ok(None);
        }
        Ok(Some(S::Expr(self.dictionary_helper_call(
            DictionaryHelper::Compact,
            vec![value],
        )?)))
    }
    /// The entry position of the block's key, or -1: under value18 the block
    /// first indexes an unindexed dictionary past the linear bound, then the
    /// table is probed (with `unlink`, the found slot becomes a tombstone),
    /// or the dense keys searched linearly; before, the linear key search.
    pub(super) fn dictionary_search(&mut self, block: &mut Block, unlink: bool) -> Result<Expr> {
        let Some(hashed) = Hashed::of(block.dictionary) else {
            return Ok(block.search());
        };
        let span = block.span;
        let (receiver, key) = (block.receiver(), block.key_value());
        let keys = || field(receiver.clone(), hashed.keys_slot, hashed.keys);
        let table = || field(receiver.clone(), hashed.table_slot, hashed.table);
        let count = self.hidden(Type::Float64);
        let (sizing, cap) = self.table_capacity(span, number(span, 0.0), count);
        let mut index = vec![S::Init(count, length(keys()))];
        index.extend(sizing);
        let built = self.shared_call(
            DictionaryHelper::Index,
            vec![keys(), local(span, cap, Type::Float64)],
            hashed.table,
        )?;
        index.push(S::SetField(receiver.clone(), hashed.table_slot, built));
        let unindexed = || binary(ast::BinaryOp::EqEqEq, table(), zero(span, hashed.table));
        block.body.push(S::If(
            binary(
                ast::BinaryOp::LogicalAnd,
                unindexed(),
                binary(
                    ast::BinaryOp::Gt,
                    length(keys()),
                    number(span, DICTIONARY_LINEAR_ENTRIES as f64),
                ),
            ),
            index,
            vec![],
        ));
        let probe = Expr {
            span,
            ty: Type::Float64,
            kind: E::Intrinsic(
                if unlink {
                    ir::Intrinsic::DictionaryUnlink
                } else {
                    ir::Intrinsic::DictionaryFind
                },
                vec![keys(), table(), key],
            ),
        };
        let linear = block.search();
        let linear = Expr {
            kind: match linear.kind {
                E::ArraySearch(op, _, needle, from) => {
                    E::ArraySearch(op, Box::new(keys()), needle, from)
                }
                other => other,
            },
            ..linear
        };
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::Conditional(Box::new(unindexed()), Box::new(linear), Box::new(probe)),
        })
    }
    /// The function index of a shared, type-independent value18 helper.
    fn shared_dictionary_helper(
        &mut self,
        helper: DictionaryHelper,
        span: swc_common::Span,
    ) -> Result<usize> {
        if let Some(index) = self
            .types
            .dictionary_helpers
            .get(&(helper, DictionaryHelper::SHARED))
        {
            return Ok(*index);
        }
        let keys = arrays::intern(Type::String, self.types)?;
        let table = arrays::intern(self.types.number(), self.types)?;
        let (params, result) = match helper {
            DictionaryHelper::Index => (vec![keys, Type::Float64], table),
            _ => return Err(invalid("not a shared dictionary helper")),
        };
        let index = self.reserve_function("too many functions including dictionary operations")?;
        self.define_function(index, span, params, result, |child, arguments| {
            let [keys, capacity] = arguments.as_slice() else {
                return Err(invalid("missing dictionary helper argument"));
            };
            let value = child.index_body(span, keys.clone(), capacity.clone(), result)?;
            Ok(vec![S::Return(Some(value))])
        })?;
        self.types
            .dictionary_helpers
            .insert((helper, DictionaryHelper::SHARED), index);
        Ok(index)
    }
    fn shared_call(&mut self, helper: DictionaryHelper, args: Vec<Expr>, ty: Type) -> Result<Expr> {
        let span = args
            .first()
            .map_or_else(swc_common::Span::default, |arg| arg.span);
        let index = self.shared_dictionary_helper(helper, span)?;
        Ok(Expr {
            span,
            ty,
            kind: E::Call(index, args),
        })
    }
    /// `Index(keys, capacity)`: a fresh table of `capacity` empty slots
    /// (holes) referencing every entry position of the dense `keys` (callers
    /// index only dictionaries without holes) from the first empty slot of
    /// its probe.
    fn index_body(
        &mut self,
        span: swc_common::Span,
        keys: Expr,
        capacity: Expr,
        table_ty: Type,
    ) -> Result<Expr> {
        let keys_ty = keys.ty;
        let [k, t] = [self.hidden(keys_ty), self.hidden(table_ty)];
        let cap = self.hidden(Type::Float64);
        let marker = self.hidden(Type::Number);
        let f64 = |slot| local(span, slot, Type::Float64);
        let reserve = Expr {
            span,
            ty: table_ty,
            kind: E::ArrayReserve {
                length: Box::new(f64(cap)),
                count: self.hidden(Type::Number),
                bytes: self.hidden(Type::Number),
            },
        };
        let placed = Expr {
            span,
            ty: Type::Float64,
            kind: E::Intrinsic(
                ir::Intrinsic::DictionaryIndex,
                vec![local(span, k, keys_ty), local(span, t, table_ty)],
            ),
        };
        let body = vec![
            S::Init(k, keys),
            S::Init(cap, capacity),
            S::Init(t, reserve),
            S::Expr(placed),
        ];
        Ok(block(span, t, marker, vec![k, t], body, table_ty))
    }
    /// Statements setting the returned slot to the table capacity for the
    /// entries in local `need`: at least `start` (and
    /// [`DICTIONARY_TABLE_MIN`]), doubled while below [`SLOTS_PER_ENTRY`]
    /// slots per entry, at most the element limit.
    fn table_capacity(&mut self, span: swc_common::Span, start: Expr, need: u32) -> (Vec<S>, u32) {
        let cap = self.hidden(Type::Float64);
        let f64 = |slot| local(span, slot, Type::Float64);
        let max = self.types.profile.data().array_elements as f64;
        let min = DICTIONARY_TABLE_MIN as f64;
        (
            vec![
                S::Init(cap, start),
                S::If(
                    binary(ast::BinaryOp::Lt, f64(cap), number(span, min)),
                    vec![S::Set(cap, number(span, min))],
                    vec![],
                ),
                S::While(
                    binary(
                        ast::BinaryOp::LogicalAnd,
                        binary(
                            ast::BinaryOp::Gt,
                            binary(ast::BinaryOp::Mul, f64(need), number(span, SLOTS_PER_ENTRY)),
                            f64(cap),
                        ),
                        binary(ast::BinaryOp::Lt, f64(cap), number(span, max)),
                    ),
                    vec![S::Set(
                        cap,
                        binary(ast::BinaryOp::Mul, f64(cap), number(span, GROWTH)),
                    )],
                ),
            ],
            cap,
        )
    }
    /// `Compact(d)`: when an indexed dictionary has holes, replace its
    /// vectors by dense copies of the present entries, clear the count and
    /// drop the table. `true` when it compacted.
    pub(super) fn compact_body(&mut self, receiver: Expr, hashed: Hashed) -> Result<Expr> {
        let span = receiver.span;
        let ty = receiver.ty;
        let source = self.hidden(ty);
        let [keys, values] = [self.hidden(hashed.keys), self.hidden(hashed.values)];
        let [fresh_keys, fresh_values] = [self.hidden(hashed.keys), self.hidden(hashed.values)];
        let output = self.hidden(Type::Boolean);
        let [count, position] = [(); 2].map(|()| self.hidden(Type::Float64));
        let marker = self.hidden(Type::Number);
        let f64 = |slot| local(span, slot, Type::Float64);
        let held = |slot, ty| local(span, slot, ty);
        let copy_key = self.append(
            held(fresh_keys, hashed.keys),
            element(held(keys, hashed.keys), f64(position), Type::String),
        );
        let copy_value = self.append(
            held(fresh_values, hashed.values),
            element(held(values, hashed.values), f64(position), hashed.value),
        );
        let copy = vec![
            S::Init(
                keys,
                field(local(span, source, ty), hashed.keys_slot, hashed.keys),
            ),
            S::Init(
                values,
                field(local(span, source, ty), hashed.values_slot, hashed.values),
            ),
            S::Init(count, length(held(keys, hashed.keys))),
            S::Init(
                fresh_keys,
                Expr {
                    span,
                    ty: hashed.keys,
                    kind: E::Array(vec![]),
                },
            ),
            S::Init(
                fresh_values,
                Expr {
                    span,
                    ty: hashed.values,
                    kind: E::Array(vec![]),
                },
            ),
            S::Init(position, number(span, 0.0)),
            S::While(
                binary(ast::BinaryOp::Lt, f64(position), f64(count)),
                vec![
                    S::If(
                        present(held(keys, hashed.keys), f64(position)),
                        vec![copy_key, copy_value],
                        vec![],
                    ),
                    increment(span, position),
                ],
            ),
            S::SetField(
                local(span, source, ty),
                hashed.keys_slot,
                held(fresh_keys, hashed.keys),
            ),
            S::SetField(
                local(span, source, ty),
                hashed.values_slot,
                held(fresh_values, hashed.values),
            ),
            S::SetField(
                local(span, source, ty),
                hashed.table_slot,
                zero(span, hashed.table),
            ),
            S::SetField(
                local(span, source, ty),
                hashed.holes_slot,
                number(span, 0.0),
            ),
            S::Set(output, boolean(span, true)),
        ];
        let body = vec![
            S::Init(source, receiver),
            S::Init(output, boolean(span, false)),
            S::If(
                binary(
                    ast::BinaryOp::Gt,
                    field(local(span, source, ty), hashed.holes_slot, Type::Float64),
                    number(span, 0.0),
                ),
                copy,
                vec![],
            ),
        ];
        Ok(block(
            span,
            output,
            marker,
            vec![source, keys, values, fresh_keys, fresh_values, output],
            body,
            Type::Boolean,
        ))
    }
    /// The absent-key branch of a value18 store: the `__proto__` check, a
    /// rebuild when one more entry would pass half load, the entry limit, then
    /// an append with one placed slot, or for an array-index key below the
    /// last key an ordered insertion: the holes dropped, the own-key position
    /// found by binary search over the sorted array-index prefix, the entry
    /// rotated there (`ir::Intrinsic::DictionaryInsertAt`) and placed.
    pub(super) fn hashed_insert(
        &mut self,
        block: &Block,
        output: u32,
        hashed: Hashed,
        proto: S,
    ) -> Result<Vec<S>> {
        let span = block.span;
        let receiver = || block.receiver();
        let keys = || field(block.receiver(), hashed.keys_slot, hashed.keys);
        let values = || field(block.receiver(), hashed.values_slot, hashed.values);
        let table = || field(block.receiver(), hashed.table_slot, hashed.table);
        let indexed = || binary(ast::BinaryOp::NotEqEq, table(), zero(span, hashed.table));
        let [count, need, rank, last] = [(); 4].map(|()| self.hidden(Type::Float64));
        let f64 = |slot| local(span, slot, Type::Float64);
        let full = self.array_error(
            ErrorClass::RangeError,
            "Dictionary entry limit exceeded",
            span,
        )?;
        let limit = varta_common::value::dictionary_entry_limit(
            self.types.profile.value_layout(),
            self.types.profile.data(),
        ) as f64;
        // Rebuild at the grown capacity (from the old one) when one more entry
        // would pass half load, after dropping the holes.
        let (sizing, cap) = self.table_capacity(span, length(table()), need);
        let compact = self.dictionary_helper_call(DictionaryHelper::Compact, vec![receiver()])?;
        let mut rebuild = vec![
            S::Set(count, length(table())),
            S::Expr(compact),
            S::Set(
                need,
                binary(ast::BinaryOp::Add, length(keys()), number(span, 1.0)),
            ),
        ];
        // The capacity starts from the old table's, read before compaction
        // may drop it.
        let mut sizing = sizing;
        if let Some(S::Init(_, start)) = sizing.first_mut() {
            *start = f64(count);
        }
        rebuild.extend(sizing);
        let built = self.shared_call(
            DictionaryHelper::Index,
            vec![keys(), f64(cap)],
            hashed.table,
        )?;
        rebuild.push(S::SetField(receiver(), hashed.table_slot, built));
        let append = vec![
            self.append(keys(), block.key_value()),
            self.append(values(), local(span, output, hashed.value)),
            S::If(
                indexed(),
                vec![place(table(), block.key_value(), f64(count))],
                vec![],
            ),
        ];
        let ordered = self.ordered_insert(block, output, hashed, count, rank)?;
        // A named key, or an array index above the last key, appends: array
        // index keys form the sorted prefix of the own-key order. `last` is
        // the index key of a present last entry, else -1 (a trailing hole
        // takes the ordered path, which compacts first).
        let previous = || binary(ast::BinaryOp::Sub, f64(count), number(span, 1.0));
        let appends = binary(
            ast::BinaryOp::LogicalOr,
            binary(ast::BinaryOp::Lt, f64(rank), number(span, 0.0)),
            binary(
                ast::BinaryOp::LogicalOr,
                binary(ast::BinaryOp::EqEqEq, f64(count), number(span, 0.0)),
                binary(
                    ast::BinaryOp::LogicalAnd,
                    binary(ast::BinaryOp::GtEq, f64(last), number(span, 0.0)),
                    binary(ast::BinaryOp::Lt, f64(last), f64(rank)),
                ),
            ),
        );
        Ok(vec![
            proto,
            S::Init(count, number(span, 0.0)),
            S::Init(need, number(span, 0.0)),
            S::Init(last, number(span, -1.0)),
            S::If(
                binary(
                    ast::BinaryOp::LogicalAnd,
                    indexed(),
                    binary(
                        ast::BinaryOp::Gt,
                        binary(
                            ast::BinaryOp::Mul,
                            binary(ast::BinaryOp::Add, length(keys()), number(span, 1.0)),
                            number(span, SLOTS_PER_ENTRY_AT_LOAD),
                        ),
                        length(table()),
                    ),
                ),
                rebuild,
                vec![],
            ),
            S::Set(count, length(keys())),
            S::If(
                binary(ast::BinaryOp::GtEq, f64(count), number(span, limit)),
                vec![S::Throw(full)],
                vec![],
            ),
            S::Init(rank, index_key(block.key_value())),
            S::If(
                binary(
                    ast::BinaryOp::LogicalAnd,
                    binary(ast::BinaryOp::GtEq, f64(rank), number(span, 0.0)),
                    binary(
                        ast::BinaryOp::LogicalAnd,
                        binary(ast::BinaryOp::Gt, f64(count), number(span, 0.0)),
                        present(keys(), previous()),
                    ),
                ),
                vec![S::Set(
                    last,
                    index_key(element(keys(), previous(), Type::String)),
                )],
                vec![],
            ),
            S::If(appends, append, ordered),
        ])
    }
    /// The ordered insertion of an absent array-index key of index `rank`
    /// below the last key (`count` is a scratch local).
    fn ordered_insert(
        &mut self,
        block: &Block,
        output: u32,
        hashed: Hashed,
        count: u32,
        rank: u32,
    ) -> Result<Vec<S>> {
        let span = block.span;
        let keys = || field(block.receiver(), hashed.keys_slot, hashed.keys);
        let values = || field(block.receiver(), hashed.values_slot, hashed.values);
        let table = || field(block.receiver(), hashed.table_slot, hashed.table);
        let [low, high, middle, key] = [(); 4].map(|()| self.hidden(Type::Float64));
        let f64 = |slot| local(span, slot, Type::Float64);
        let compact =
            self.dictionary_helper_call(DictionaryHelper::Compact, vec![block.receiver()])?;
        let holes = field(block.receiver(), hashed.holes_slot, Type::Float64);
        let search = vec![
            S::Set(
                middle,
                binary(
                    ast::BinaryOp::ZeroFillRShift,
                    binary(ast::BinaryOp::Add, f64(low), f64(high)),
                    number(span, 1.0),
                ),
            ),
            S::Set(key, index_key(element(keys(), f64(middle), Type::String))),
            S::If(
                binary(
                    ast::BinaryOp::LogicalAnd,
                    binary(ast::BinaryOp::GtEq, f64(key), number(span, 0.0)),
                    binary(ast::BinaryOp::Lt, f64(key), f64(rank)),
                ),
                vec![S::Set(
                    low,
                    binary(ast::BinaryOp::Add, f64(middle), number(span, 1.0)),
                )],
                vec![S::Set(high, f64(middle))],
            ),
        ];
        let rotate = Expr {
            span,
            ty: Type::Float64,
            kind: E::Intrinsic(
                ir::Intrinsic::DictionaryInsertAt,
                vec![keys(), values(), table(), f64(low)],
            ),
        };
        Ok(vec![
            S::If(
                binary(ast::BinaryOp::Gt, holes, number(span, 0.0)),
                vec![S::Expr(compact)],
                vec![],
            ),
            S::Set(count, length(keys())),
            S::Init(low, number(span, 0.0)),
            S::Init(high, f64(count)),
            S::Init(middle, number(span, 0.0)),
            S::Init(key, number(span, 0.0)),
            S::While(binary(ast::BinaryOp::Lt, f64(low), f64(high)), search),
            self.append(keys(), block.key_value()),
            self.append(values(), local(span, output, hashed.value)),
            S::Expr(rotate),
            S::If(
                binary(ast::BinaryOp::NotEqEq, table(), zero(span, hashed.table)),
                vec![place(table(), block.key_value(), f64(low))],
                vec![],
            ),
        ])
    }
    /// The present-key branch of a value18 delete: under an index, a hole in
    /// both vectors (the probe already tombstoned its slot) counted in
    /// `holes`; otherwise the ordered removal `shift`.
    pub(super) fn hashed_remove(
        &mut self,
        block: &Block,
        index: u32,
        hashed: Hashed,
        shift: Vec<S>,
    ) -> Vec<S> {
        let span = block.span;
        let hole = |array: Expr| {
            S::Expr(Expr {
                span,
                ty: Type::Boolean,
                kind: E::ArrayDelete {
                    array: Box::new(array),
                    index: Some(Box::new(local(span, index, Type::Float64))),
                    error: None,
                },
            })
        };
        let holes = || field(block.receiver(), hashed.holes_slot, Type::Float64);
        vec![S::If(
            binary(
                ast::BinaryOp::EqEqEq,
                field(block.receiver(), hashed.table_slot, hashed.table),
                zero(span, hashed.table),
            ),
            shift,
            vec![
                hole(field(block.receiver(), hashed.keys_slot, hashed.keys)),
                hole(field(block.receiver(), hashed.values_slot, hashed.values)),
                S::SetField(
                    block.receiver(),
                    hashed.holes_slot,
                    binary(ast::BinaryOp::Add, holes(), number(span, 1.0)),
                ),
            ],
        )]
    }
}

/// The canonical storage slots of a fresh `dictionary` of `keys` and
/// `values`: under value18 also no holes and the zero table handle.
pub(super) fn storage_fields(
    span: swc_common::Span,
    dictionary: DictionaryType,
    keys: Expr,
    values: Expr,
) -> Vec<(u32, Expr)> {
    let mut fields = Vec::new();
    if let Some(slot) = dictionary.holes_slot() {
        fields.push((slot, number(span, 0.0)));
    }
    fields.push((dictionary.keys_slot(), keys));
    if let (Some(slot), Some(table)) = (dictionary.table_slot(), dictionary.table) {
        fields.push((slot, zero(span, table)));
    }
    fields.push((dictionary.values_slot(), values));
    fields
}
