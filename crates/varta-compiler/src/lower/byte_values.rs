//! `"key" in` a value held by an `unknown` cell (v38 `byte_values`).
//!
//! `"key" in value` of an `object` fact and `"key" in (value as
//! Record<string, unknown>)` of an `unknown` or `object` value read the cell
//! as a JSON object (`object_narrowing`, `declared_fields`). A `Uint8Array`
//! and the engine prelude instances the pinned SDK passes as multipart parts
//! (`ArrayBuffer`, `Blob`, `File`) are no JSON objects: JavaScript answers
//! `in` for them from their own index keys (a `Uint8Array`) and their
//! prototype chains. For a string literal key, this form answers exactly as
//! Node 24.20.0 does, from the cell's tag and exact class descriptor (a
//! program's subclass of such a class is not matched); every other value
//! keeps the JSON-object view, which throws its named `TypeError` for class
//! instances, arrays and Error objects.
use super::*;
use crate::modules::engine::{ARRAY_BUFFER_CLASS, BINARY, BLOB_CLASS, FILE_CLASS};
use swc_common::Span;

/// String-keyed members of `Uint8Array.prototype` and
/// `%TypedArray%.prototype` in Node 24.20.0 (`Object.getOwnPropertyNames`
/// along the chain, `Object.prototype` excluded; it is `records::inherited`).
const UINT8_ARRAY_PROTOTYPE: &[&str] = &[
    "constructor",
    "BYTES_PER_ELEMENT",
    "buffer",
    "byteLength",
    "byteOffset",
    "length",
    "entries",
    "keys",
    "values",
    "at",
    "copyWithin",
    "every",
    "fill",
    "filter",
    "find",
    "findIndex",
    "findLast",
    "findLastIndex",
    "forEach",
    "includes",
    "indexOf",
    "join",
    "lastIndexOf",
    "map",
    "reverse",
    "reduce",
    "reduceRight",
    "set",
    "slice",
    "some",
    "sort",
    "subarray",
    "toReversed",
    "toSorted",
    "with",
    "toLocaleString",
    "toString",
];
/// String-keyed members of `ArrayBuffer.prototype` in Node 24.20.0.
const ARRAY_BUFFER_PROTOTYPE: &[&str] = &[
    "constructor",
    "byteLength",
    "slice",
    "maxByteLength",
    "resizable",
    "resize",
    "transfer",
    "transferToFixedLength",
    "detached",
];
/// String-keyed members of `Blob.prototype` in Node 24.20.0.
const BLOB_PROTOTYPE: &[&str] = &[
    "constructor",
    "type",
    "size",
    "slice",
    "arrayBuffer",
    "text",
    "bytes",
    "stream",
    "textStream",
];
/// String-keyed members of `File.prototype` in Node 24.20.0 (its chain
/// continues with [`BLOB_PROTOTYPE`]).
const FILE_PROTOTYPE: &[&str] = &["constructor", "name", "lastModified"];

/// The engine classes whose instances `in` answers from their prototype
/// chains (none has an own enumerable property in Node): the class and its
/// chain of member lists.
const PROTOTYPE_CHAINS: [(&str, &[&[&str]]); 3] = [
    (ARRAY_BUFFER_CLASS, &[ARRAY_BUFFER_PROTOTYPE]),
    (BLOB_CLASS, &[BLOB_PROTOTYPE]),
    (FILE_CLASS, &[FILE_PROTOTYPE, BLOB_PROTOTYPE]),
];

/// Whether `name` is a member of one of `chain`'s lists or of
/// `Object.prototype`.
fn inherits(chain: &[&[&str]], name: &str) -> bool {
    records::inherited(name) || chain.iter().any(|members| members.contains(&name))
}

fn constant(span: Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

/// How the right operand of an admitted `in` reaches its cell.
struct Operand<'a> {
    /// The expression whose value is the `unknown` or `object` cell.
    value: &'a ast::Expr,
    /// The quoted assertion target of `value as Record<string, unknown>`;
    /// `None` for an identifier narrowed to `object`.
    asserted: Option<String>,
}

impl Lower<'_> {
    /// `"key" in value` (an `object` fact) or `"key" in (value as
    /// Record<string, unknown>)` (`value` an `unknown` or `object` value)
    /// with a string literal key under v38 (`byte_values`); `None` for any
    /// other `in`.
    pub(super) fn cell_in(&mut self, binary: &ast::BinExpr) -> Result<Option<Expr>> {
        if binary.op != ast::BinaryOp::In || !self.types.profile.byte_values() {
            return Ok(None);
        }
        let ast::Expr::Lit(ast::Lit::Str(text)) = stmt::unparen(&binary.left) else {
            return Ok(None);
        };
        let Some(name) = text.value.as_str().map(str::to_owned) else {
            return Ok(None);
        };
        let Some(operand) = self.cell_operand(&binary.right)? else {
            return Ok(None);
        };
        let span = binary.span;
        let value = self.expr(operand.value)?;
        let cell = match value.ty {
            Type::Unknown => value,
            Type::Object(_) => Expr {
                span: value.span,
                ty: Type::Unknown,
                kind: E::Reinterpret(Box::new(value)),
            },
            other => {
                return Err(CompilerError::Internal(format!(
                    "`in` operand of type {other} read as an unknown cell"
                )));
            }
        };
        self.cell_in_value(&name, cell, operand.asserted, span)
            .map(Some)
    }
    /// The admitted right operand of [`Self::cell_in`], if any.
    fn cell_operand<'a>(&mut self, right: &'a ast::Expr) -> Result<Option<Operand<'a>>> {
        let cell = |ty: Option<Type>| matches!(ty, Some(Type::Unknown | Type::Object(_)));
        match stmt::unparen(right) {
            ast::Expr::Ident(_) => {
                // Only an `object` fact (v35) admits a bare `in`.
                let fact = self.peek_type(right, 0);
                Ok((fact == Some(Type::Object(false))).then_some(Operand {
                    value: right,
                    asserted: None,
                }))
            }
            ast::Expr::TsAs(assertion) => {
                if !cell(self.peek_type(&assertion.expr, 0)) {
                    return Ok(None);
                }
                let annotation = &*assertion.type_ann;
                let target = keyword_type(annotation, self.types)
                    .map_err(|e| self.source.locate(e, annotation.span()))?;
                let graph = dictionaries::intern(Type::Unknown, self.types)?;
                if target != graph || self.types.declared(target).is_some() {
                    return Ok(None);
                }
                Ok(Some(Operand {
                    value: &assertion.expr,
                    asserted: Some(self.quoted(annotation.span(), target)),
                }))
            }
            _ => Ok(None),
        }
    }
    /// `name in` the `unknown` cell `cell`: a `Uint8Array`'s index keys and
    /// prototype chain, an exact `ArrayBuffer`, `Blob` or `File` instance's
    /// prototype chain, otherwise the JSON-object view (`asserted` names
    /// the assertion target of its `TypeError`).
    fn cell_in_value(
        &mut self,
        name: &str,
        cell: Expr,
        asserted: Option<String>,
        span: Span,
    ) -> Result<Expr> {
        let saved = self.hidden(Type::Unknown);
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        let held = |ty: Type| Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsUnionMember(Box::new(local(span, saved, Type::Unknown)), ty),
        };
        let unboxed = |ty: Type| Expr {
            span,
            ty,
            kind: E::UnboxUnion(Box::new(local(span, saved, Type::Unknown))),
        };
        let (mut body, dictionary) = self.json_object_in(name, saved, output, asserted, span)?;
        for (class_name, chain) in PROTOTYPE_CHAINS {
            let Some(class) = self.types.classes.prelude_class(class_name) else {
                continue;
            };
            let instance = Type::Class(class);
            let exact = Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsClass {
                    value: Box::new(unboxed(instance)),
                    shapes: vec![self.types.classes.shape(class)],
                    slot: self.hidden(Type::Number),
                },
            };
            let test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::Conditional(
                    Box::new(held(instance)),
                    Box::new(exact),
                    Box::new(constant(span, false)),
                ),
            };
            body = vec![S::If(
                test,
                vec![S::Init(output, constant(span, inherits(chain, name)))],
                body,
            )];
        }
        // A module without the binary prelude has no `Uint8Array` value.
        if engine::module(&BINARY, self.types).is_some() {
            let bytes = collections::bytes(self.types)?;
            let answer =
                match varta_common::value::array_index(&name.encode_utf16().collect::<Vec<_>>()) {
                    // An index key is own while it is below the length.
                    Some(index) => Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Binary(
                            ast::BinaryOp::Lt,
                            Box::new(Expr {
                                span,
                                ty: Type::Float64,
                                kind: E::Float64(index as f64),
                            }),
                            Box::new(Expr {
                                span,
                                ty: self.types.number(),
                                kind: E::Length(Box::new(self.storage(unboxed(bytes)))),
                            }),
                        ),
                    },
                    // Any other canonical numeric key is no property of a typed
                    // array, and no prototype member is numeric.
                    None => constant(span, inherits(&[UINT8_ARRAY_PROTOTYPE], name)),
                };
            body = vec![S::If(held(bytes), vec![S::Init(output, answer)], body)];
        }
        body.insert(0, S::Init(saved, cell));
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved, dictionary],
                body,
            },
        })
    }
    /// `name in` the JSON-object view of the cell in `saved` into `output`:
    /// the checked conversion to `Record<string, unknown>` (a `TypeError`
    /// for any other value), then its own entries and inherited
    /// `Object.prototype` names. Also returns the rooted local that holds
    /// the converted dictionary.
    fn json_object_in(
        &mut self,
        name: &str,
        saved: u32,
        output: u32,
        asserted: Option<String>,
        span: Span,
    ) -> Result<(Vec<S>, u32)> {
        let graph = dictionaries::intern(Type::Unknown, self.types)?;
        let (target, message) = match asserted {
            Some(quoted) => {
                let message = format!("type assertion to {quoted} failed");
                (quoted, message)
            }
            None => (
                "Record<string, unknown>".to_owned(),
                object_narrowing::NOT_A_DICTIONARY.to_owned(),
            ),
        };
        let converted = self.assert_value(
            local(span, saved, Type::Unknown),
            graph,
            &target,
            &message,
            span,
        )?;
        let dictionary = self.hidden(graph);
        let key = Expr {
            span,
            ty: Type::String,
            kind: E::String(self.types.profile.string_bytes(name)),
        };
        let has = self.dictionary_has(local(span, dictionary, graph), key, true)?;
        Ok((
            vec![S::Init(dictionary, converted), S::Init(output, has)],
            dictionary,
        ))
    }
}
