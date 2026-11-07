//! V39 checked snapshots: typed declared slots plus retained JSON extras/order.
use super::*;
mod copies;
mod inputs;
use std::collections::BTreeSet;
use varta_common::value::{INDEX_SIGNATURE_KEY, SNAPSHOT_ORDER_KEY};

#[derive(Clone, Copy)]
pub(crate) struct SnapshotRecord {
    pub physical: u32,
    pub extras: u32,
    pub order: u32,
    pub graph: usize,
    pub has: usize,
}
#[derive(Default)]
pub(crate) struct Snapshots {
    pub records: BTreeMap<u32, SnapshotRecord>,
    object: Option<usize>,
    array: Option<usize>,
}

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
fn binary(span: swc_common::Span, op: ast::BinaryOp, a: Expr, b: Expr, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Binary(op, Box::new(a), Box::new(b)),
    }
}

pub(super) fn physical(shape: u32, types: &ir::ValueTypes) -> Option<SnapshotRecord> {
    types
        .snapshots
        .records
        .values()
        .find(|record| record.physical == shape)
        .copied()
}

/// Snapshot mode is needed only where conversion builds a declared record.
/// Plain dictionary/unknown-array assertions keep their existing graph views.
pub(super) fn applies(target: Type, types: &ir::ValueTypes) -> bool {
    if target.is_union()
        && unions::members(target, types)
            .iter()
            .filter(|member| matches!(member, Type::Record(_)))
            .count()
            > 1
    {
        return false;
    }
    fn visit(ty: Type, types: &ir::ValueTypes, seen: &mut BTreeSet<Type>) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        match ty {
            Type::Record(_) => true,
            Type::Array(id) => visit(types.arrays[id as usize], types, seen),
            Type::Dictionary(id) => visit(types.dictionaries[id as usize].value, types, seen),
            Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
                .iter()
                .any(|member| visit(*member, types, seen)),
            _ => false,
        }
    }
    types.profile.checked_snapshots() && visit(target, types, &mut BTreeSet::new())
}

impl Lower<'_> {
    fn snapshot_graph_body(
        &mut self,
        record: SnapshotRecord,
        args: Vec<Expr>,
        graph: Type,
    ) -> Result<Vec<S>> {
        let source = args
            .into_iter()
            .next()
            .ok_or_else(|| invalid("snapshot graph argument"))?;
        let span = source.span;
        let field = |slot, ty| Expr {
            span,
            ty,
            kind: E::Field(Box::new(source.clone()), slot),
        };
        let order_ty = arrays::intern(Type::String, self.types)?;
        let order = self.hidden(order_ty);
        let extras = self.hidden(graph);
        let output = self.hidden(graph);
        let index = self.hidden(Type::Float64);
        let key = self.hidden(Type::String);
        let store = |this: &mut Self, value| {
            this.dictionary_helper_call(
                ir::DictionaryHelper::Define,
                vec![
                    local(span, output, graph),
                    local(span, key, Type::String),
                    value,
                ],
            )
            .map(S::Expr)
        };
        let extra = self.dictionary_helper_call(
            ir::DictionaryHelper::Get,
            vec![local(span, extras, graph), local(span, key, Type::String)],
        )?;
        let mut chain = vec![store(self, extra)?];
        let fields = self.types.records[record.physical as usize].fields.clone();
        for (slot, declared) in fields.iter().enumerate().rev() {
            if declared
                .name
                .starts_with(varta_common::value::PRIVATE_KEY_PREFIX)
            {
                continue;
            }
            let value = unknown::widen(field(slot as u32, declared.ty), self.types)?;
            let present = Expr {
                span,
                ty: Type::Boolean,
                kind: E::HasOwn(Box::new(source.clone()), Some(slot as u32)),
            };
            let branch = if declared.optional {
                vec![S::If(present, vec![store(self, value)?], Vec::new())]
            } else {
                vec![store(self, value)?]
            };
            let name = Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(&declared.name)),
            };
            chain = vec![S::If(
                binary(
                    span,
                    ast::BinaryOp::EqEqEq,
                    local(span, key, Type::String),
                    name,
                    Type::Boolean,
                ),
                branch,
                chain,
            )];
        }
        let mut step = vec![S::Init(
            key,
            Expr {
                span,
                ty: Type::String,
                kind: E::Index(
                    Box::new(local(span, order, order_ty)),
                    Box::new(local(span, index, Type::Float64)),
                ),
            },
        )];
        step.extend(chain);
        step.push(S::Set(
            index,
            binary(
                span,
                ast::BinaryOp::Add,
                local(span, index, Type::Float64),
                number(span, 1.0),
                Type::Float64,
            ),
        ));
        Ok(vec![
            S::Init(output, self.snapshot_empty_graph(span)?),
            S::Init(order, field(record.order, order_ty)),
            S::Init(extras, field(record.extras, graph)),
            S::Init(index, number(span, 0.0)),
            S::While(
                binary(
                    span,
                    ast::BinaryOp::Lt,
                    local(span, index, Type::Float64),
                    Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Length(Box::new(local(span, order, order_ty))),
                    },
                    Type::Boolean,
                ),
                step,
            ),
            S::Return(Some(local(span, output, graph))),
        ])
    }
    pub(super) fn prepare_snapshot_records(&mut self, target: Type) -> Result<()> {
        let mut pending = vec![target];
        let mut seen = BTreeSet::new();
        while let Some(ty) = pending.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match ty {
                Type::Record(shape) => {
                    self.snapshot_record(shape)?;
                    pending.extend(
                        self.types.records[shape as usize]
                            .fields
                            .iter()
                            .map(|field| field.ty),
                    );
                }
                Type::Array(id) => pending.push(self.types.arrays[id as usize]),
                Type::Dictionary(id) => pending.push(self.types.dictionaries[id as usize].value),
                Type::Union(id) | Type::ReferenceUnion(id) => {
                    pending.extend(self.types.unions[id as usize].clone())
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(super) fn snapshot_empty_graph(&mut self, span: swc_common::Span) -> Result<Expr> {
        let graph = dictionaries::intern(Type::Unknown, self.types)?;
        let Type::Dictionary(id) = graph else {
            return Err(invalid("snapshot graph type"));
        };
        let dictionary = self.types.dictionaries[id as usize];
        let empty = |ty| Expr {
            span,
            ty,
            kind: E::Array(Vec::new()),
        };
        Ok(Expr {
            span,
            ty: graph,
            kind: E::Record(dictionary_hashing::storage_fields(
                span,
                dictionary,
                empty(dictionary.keys),
                empty(dictionary.values),
            )),
        })
    }
    pub(super) fn snapshot_clone(&mut self, value: Expr, span: swc_common::Span) -> Result<Expr> {
        self.snapshot_clone_mode(value, span, true)
    }
    pub(super) fn snapshot_extra(&mut self, value: Expr, span: swc_common::Span) -> Result<Expr> {
        self.snapshot_clone_mode(value, span, false)
    }
    fn snapshot_clone_mode(
        &mut self,
        value: Expr,
        span: swc_common::Span,
        allow_undefined: bool,
    ) -> Result<Expr> {
        let depth = Expr {
            span,
            ty: Type::Float64,
            kind: E::Float64(self.types.profile.data().json_depth as f64),
        };
        self.engine_apply(
            (&crate::modules::engine::SNAPSHOT, "snapshotClone", &[]),
            vec![
                value,
                depth,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(i32::from(allow_undefined)),
                },
            ],
            span,
        )
    }
    pub(super) fn snapshot_record(&mut self, shape: u32) -> Result<SnapshotRecord> {
        if let Some(record) = self.types.snapshots.records.get(&shape) {
            return Ok(*record);
        }
        let graph = dictionaries::intern(Type::Unknown, self.types)?;
        let order = arrays::intern(Type::String, self.types)?;
        let mut fields = self.types.records[shape as usize].fields.clone();
        fields.extend([
            ir::RecordField {
                name: INDEX_SIGNATURE_KEY.into(),
                ty: graph,
                optional: false,
            },
            ir::RecordField {
                name: SNAPSHOT_ORDER_KEY.into(),
                ty: order,
                optional: false,
            },
        ]);
        let Type::Record(physical) = records::intern_snapshot(fields, self.types)? else {
            return Err(invalid("missing snapshot record shape"));
        };
        let fields = &self.types.records[physical as usize].fields;
        let find = |name: &str| {
            fields
                .iter()
                .position(|field| field.name == name)
                .map(|slot| slot as u32)
                .ok_or_else(|| invalid("missing snapshot metadata slot"))
        };
        let record = SnapshotRecord {
            physical,
            extras: find(INDEX_SIGNATURE_KEY)?,
            order: find(SNAPSHOT_ORDER_KEY)?,
            graph: self.reserve_function("too many functions including snapshot graph views")?,
            has: self.reserve_function("too many functions including snapshot presence checks")?,
        };
        self.types.snapshots.records.insert(shape, record);
        self.types
            .subtyping
            .note_view(Type::Record(physical), shape);
        self.types.subtyping.note_boxed_shape(physical);
        self.define_function(
            record.has,
            swc_common::DUMMY_SP,
            vec![Type::Record(physical), Type::String],
            Type::Boolean,
            |child, args| {
                let [object, key] = <[Expr; 2]>::try_from(args)
                    .map_err(|_| invalid("snapshot presence arguments"))?;
                let extras = Expr {
                    span: object.span,
                    ty: graph,
                    kind: E::Field(Box::new(object), record.extras),
                };
                let found = child.dictionary_has(extras, key, false)?;
                Ok(vec![S::Return(Some(found))])
            },
        )?;
        self.define_function(
            record.graph,
            swc_common::DUMMY_SP,
            vec![Type::Record(physical)],
            graph,
            |child, args| child.snapshot_graph_body(record, args, graph),
        )?;
        Ok(record)
    }
}
