//! Value15 dictionaries (v34 `index_signatures`): `{ [key: string]: T }` and
//! `Record<string, T>`. A dictionary is physically the two-slot storage record
//! `{ keys: string[]; values: T[] }`; every operation lowers to ordinary IR over
//! those vectors inside a rooted value block, keeping keys distinct and in
//! ECMAScript own-key order (canonical array indices ascending, then insertion).
use super::*;
use ir::{DictionaryHelper, DictionaryType, RecordField};
use varta_common::value::{
    DICTIONARY_KEYS_FIELD, DICTIONARY_VALUES_FIELD, ErrorClass, MAX_DICTIONARY_ENTRIES,
};

/// Canonical storage slots (fields sorted by name: `keys` < `values`).
pub(super) const KEYS: u32 = 0;
pub(super) const VALUES: u32 = 1;
const PROTO: &str = "__proto__";

pub(super) fn intern(value: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    if !types.profile.index_signatures() {
        return Err(unsupported("index signature"));
    }
    if matches!(value, Type::Generator(_)) {
        return Err(generators::opaque("a dictionary"));
    }
    if matches!(value, Type::Function(_)) && !types.profile.function_fields() {
        return Err(unsupported("function-valued index signature"));
    }
    if value.profile_kind(types.profile).is_none() {
        return Err(unsupported("index signature value type"));
    }
    if let Some(id) = types
        .dictionaries
        .iter()
        .position(|d| d.value == value && d.declared.is_none())
    {
        return Ok(Type::Dictionary(id as u32));
    }
    let keys = arrays::intern(Type::String, types)?;
    let values = arrays::intern(value, types)?;
    let Type::Record(storage) = records::intern(
        vec![
            RecordField {
                name: DICTIONARY_KEYS_FIELD.into(),
                ty: keys,
                optional: false,
            },
            RecordField {
                name: DICTIONARY_VALUES_FIELD.into(),
                ty: values,
                optional: false,
            },
        ],
        types,
    )?
    else {
        return Err(invalid("missing dictionary storage shape"));
    };
    let id = types.dictionaries.len() as u32;
    types.dictionaries.push(DictionaryType {
        value,
        keys,
        values,
        storage,
        declared: None,
    });
    Ok(Type::Dictionary(id))
}

/// `{ [key: string]: T }`: a type literal whose only member is one string
/// index signature, or (v35 `mixed_index_records`) a mixed object type that
/// also declares fields. `None` when the literal has no index signature.
pub(super) fn signature(
    literal: &ast::TsTypeLit,
    types: &mut ir::ValueTypes,
) -> Result<Option<Type>> {
    let mut indices = literal.members.iter().filter_map(|member| match member {
        ast::TsTypeElement::TsIndexSignature(index) => Some(index),
        _ => None,
    });
    let Some(index) = indices.next() else {
        return Ok(None);
    };
    if literal.members.len() == 1 {
        return intern(index_value(index, types)?, types).map(Some);
    }
    if !types.profile.mixed_index_records() {
        return Err(unsupported(
            "index signature combined with declared fields (a mixed object type)",
        ));
    }
    if indices.next().is_some() {
        return Err(unsupported(
            "more than one index signature in an object type",
        ));
    }
    let value = index_value(index, types)?;
    let members: Vec<_> = literal
        .members
        .iter()
        .filter(|member| !matches!(member, ast::TsTypeElement::TsIndexSignature(_)))
        .cloned()
        .collect();
    let fields = records::type_fields(&members, types)?;
    mixed_records::intern(value, fields, false, types).map(Some)
}

/// The value type `T` of one `[key: string]: T` signature.
pub(super) fn index_value(
    index: &ast::TsIndexSignature,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    if index.readonly || index.is_static {
        return Err(unsupported("readonly or static index signature"));
    }
    let [ast::TsFnParam::Ident(key)] = index.params.as_slice() else {
        return Err(unsupported(
            "index signature with other than one key parameter",
        ));
    };
    let key = match key.type_ann.as_deref() {
        Some(ann) => keyword_type(&ann.type_ann, types)?,
        None => return Err(invalid("index signature keys require an annotation")),
    };
    if key != Type::String {
        return Err(unsupported(
            "index signature key other than string (number and symbol keys)",
        ));
    }
    annotation(index.type_ann.as_deref(), types)?
        .ok_or_else(|| invalid("index signatures require a value annotation"))
}

/// Whether a dictionary of type `source` may be used as `target` without a
/// test (v35): both share one storage layout (the same value type), and
/// TypeScript relates them. Declared members are a static view whose reads
/// are checked, so retagging never trusts a value's declared fields.
pub(super) fn converts(target: Type, source: Type, types: &ir::ValueTypes) -> bool {
    let (Type::Dictionary(to), Type::Dictionary(from)) = (target, source) else {
        return false;
    };
    let (to, from) = (
        types.dictionaries[to as usize],
        types.dictionaries[from as usize],
    );
    if to == from || to.storage != from.storage || to.value != from.value {
        return to == from;
    }
    let source = types.declared(source);
    let Some(target) = types.declared(target) else {
        // Every mixed record is a dictionary of its index type; an `object`
        // view becomes one only through a checked assertion.
        return !source.is_some_and(|members| members.object);
    };
    if target.object || source.is_some_and(|members| members.object) {
        return false;
    }
    target.fields.iter().all(|field| {
        match source.and_then(|members| members.fields.iter().find(|f| f.name == field.name)) {
            Some(own) => {
                (field.optional || !own.optional)
                    && mixed_records::assignable(field.ty, own.ty, types)
            }
            // TypeScript relates the source index signature to an optional
            // target property; a required one must be declared.
            None => field.optional && mixed_records::assignable(field.ty, to.value, types),
        }
    })
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
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn binary(op: ast::BinaryOp, left: Expr, right: Expr) -> Expr {
    Expr {
        span: left.span,
        ty: Type::Boolean,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}
fn add(left: Expr, right: f64) -> Expr {
    let span = left.span;
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Binary(
            ast::BinaryOp::Add,
            Box::new(left),
            Box::new(number(span, right)),
        ),
    }
}
fn length(array: Expr) -> Expr {
    Expr {
        span: array.span,
        ty: Type::Float64,
        kind: E::Length(Box::new(array)),
    }
}
fn index_key(key: Expr) -> Expr {
    Expr {
        span: key.span,
        ty: Type::Float64,
        kind: E::IndexKey(Box::new(key)),
    }
}

/// Hidden locals of one dictionary operation and its rooted value block.
pub(super) struct Block {
    span: swc_common::Span,
    dictionary: DictionaryType,
    ty: Type,
    source: u32,
    key: u32,
    roots: Vec<u32>,
    pub(super) body: Vec<S>,
}
impl Block {
    fn source(&self) -> Expr {
        local(self.span, self.source, self.ty)
    }
    fn key(&self) -> Expr {
        local(self.span, self.key, Type::String)
    }
    fn keys(&self) -> Expr {
        Expr {
            span: self.span,
            ty: self.dictionary.keys,
            kind: E::Field(Box::new(self.source()), KEYS),
        }
    }
    fn values(&self) -> Expr {
        Expr {
            span: self.span,
            ty: self.dictionary.values,
            kind: E::Field(Box::new(self.source()), VALUES),
        }
    }
    /// The key (`true`) or value vector with its element type.
    fn side(&self, keys: bool) -> (Expr, Type) {
        if keys {
            (self.keys(), Type::String)
        } else {
            (self.values(), self.dictionary.value)
        }
    }
    fn element(&self, array: Expr, element: Type, index: Expr) -> Expr {
        Expr {
            span: self.span,
            ty: element,
            kind: E::Index(Box::new(array), Box::new(index)),
        }
    }
    /// `keys.indexOf(key)`: content equality over the dense key vector.
    fn search(&self) -> Expr {
        Expr {
            span: self.span,
            ty: Type::Float64,
            kind: E::ArraySearch(
                ir::ArraySearch::IndexOf,
                Box::new(self.keys()),
                Box::new(self.key()),
                Box::new(number(self.span, 0.0)),
            ),
        }
    }
}

impl Lower<'_> {
    pub(super) fn dictionary_block(&mut self, value: Expr, key: Expr) -> Result<Block> {
        let Type::Dictionary(id) = value.ty else {
            return Err(invalid("missing dictionary type"));
        };
        let key = self.dictionary_key_value(key)?;
        let span = value.span;
        let ty = value.ty;
        let source = self.hidden(ty);
        let name = self.hidden(Type::String);
        Ok(Block {
            span,
            dictionary: self.types.dictionaries[id as usize],
            ty,
            source,
            key: name,
            roots: vec![source, name],
            body: vec![S::Init(source, value), S::Init(name, key)],
        })
    }
    /// A key as a property name: a narrowed string or (v35) a number,
    /// converted exactly like `ToPropertyKey` (`String(n)`, so `d[1]` is
    /// `d["1"]` and `d[-0]` is `d["0"]`). Other types are rejected.
    pub(super) fn dictionary_key_value(&mut self, key: Expr) -> Result<Expr> {
        let key = literals::erase(key, self.types);
        if key.ty == Type::Float64 && self.types.profile.mixed_index_records() {
            let span = key.span;
            return self.primitive_convert(key, &[], false, span);
        }
        if key.ty != Type::String {
            return Err(self.source.locate(
                type_error(format!(
                    "dictionary key must be a narrowed string, found {}; coercion is unsupported",
                    key.ty
                )),
                key.span,
            ));
        }
        Ok(key)
    }
    pub(super) fn finish_dictionary(&mut self, block: Block, output: u32, ty: Type) -> Expr {
        let marker = self.hidden(Type::Number);
        let mut roots = block.roots;
        roots.push(output);
        Expr {
            span: block.span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body: block.body,
            },
        }
    }
    /// Look `key` up in `value`: the found entry (valid when the test holds),
    /// the found test and the open block, receiver and key evaluated once.
    pub(super) fn dictionary_lookup(
        &mut self,
        value: Expr,
        key: Expr,
    ) -> Result<(Expr, Expr, Block)> {
        let mut block = self.dictionary_block(value, key)?;
        let span = block.span;
        let index = self.hidden(Type::Float64);
        block.body.push(S::Init(index, block.search()));
        let found = block.element(
            block.values(),
            block.dictionary.value,
            local(span, index, Type::Float64),
        );
        let test = binary(
            ast::BinaryOp::GtEq,
            local(span, index, Type::Float64),
            number(span, 0.0),
        );
        Ok((found, test, block))
    }
    /// Throws for an absent key naming an inherited `Object.prototype` member,
    /// the checked boundary shared with closed computed record reads.
    fn inherited_miss(&mut self, block: &Block) -> Result<Vec<S>> {
        let mut entries = Vec::new();
        let mut names: Vec<_> = records::INHERITED_NAMES.to_vec();
        names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
        for name in names {
            let error = self.array_error(
                ErrorClass::TypeError,
                "Inherited record property access is unsupported",
                block.span,
            )?;
            entries.push((self.types.profile.string_bytes(name), vec![S::Throw(error)]));
        }
        Ok(own_keys::lookup(block.span, block.key, &mut entries))
    }
    /// The key of `d.name`, `d["name"]` or `d[expression]`, evaluated after the
    /// receiver.
    pub(super) fn dictionary_key(&mut self, member: &ast::MemberExpr) -> Result<Expr> {
        let span = member.prop.span();
        match &member.prop {
            ast::MemberProp::Ident(id) => Ok(Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(&id.sym)),
            }),
            ast::MemberProp::Computed(key) => self.expr(&key.expr),
            _ => Err(unsupported("private dictionary property")),
        }
    }
    /// `d[key]`: the stored `T`, or `undefined` for an absent own key.
    pub(super) fn dictionary_read(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
    ) -> Result<Expr> {
        if let Some(field) = mixed_records::declared_field(value.ty, member, self.types) {
            return self.declared_read(member, value, &field);
        }
        mixed_records::object_key(value.ty, member, self.types)
            .map_err(|e| self.source.locate(e, member.prop.span()))?;
        let key = self.dictionary_key(member)?;
        self.dictionary_get(value, key)
    }
    /// The stored `T` under `key`, or `undefined` for an absent own key.
    pub(super) fn dictionary_get(&mut self, value: Expr, key: Expr) -> Result<Expr> {
        if self.types.profile.mixed_index_records() {
            let key = self.dictionary_key_value(key)?;
            return self.dictionary_helper_call(DictionaryHelper::Get, vec![value, key]);
        }
        self.dictionary_get_inline(value, key)
    }
    /// The inline body of [`Self::dictionary_get`].
    pub(super) fn dictionary_get_inline(&mut self, value: Expr, key: Expr) -> Result<Expr> {
        let mut block = self.dictionary_block(value, key)?;
        let element = block.dictionary.value;
        let result = unions::intern(vec![element, Type::Undefined], self.types)?;
        let output = self.hidden(result);
        let index = self.hidden(Type::Float64);
        let span = block.span;
        let undefined = unions::widen(
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            },
            Some(result),
            self.types,
        )?;
        let found = unions::widen(
            block.element(block.values(), element, local(span, index, Type::Float64)),
            Some(result),
            self.types,
        )?;
        let miss = self.inherited_miss(&block)?;
        block.body.extend([
            S::Init(index, block.search()),
            S::Init(output, undefined),
            S::If(
                binary(
                    ast::BinaryOp::GtEq,
                    local(span, index, Type::Float64),
                    number(span, 0.0),
                ),
                vec![S::Set(output, found)],
                miss,
            ),
        ]);
        Ok(self.finish_dictionary(block, output, result))
    }
    /// `d[key] = value` (statement or expression): replaces an existing
    /// entry in place or inserts a new one at its own-key position.
    pub(super) fn dictionary_write(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
        rhs: &ast::Expr,
    ) -> Result<Expr> {
        // A declared key is written at its declared type (v35), then stored
        // as the index type it is assignable to.
        let declared = mixed_records::declared_field(base.ty, member, self.types);
        if declared.is_none() {
            mixed_records::object_key(base.ty, member, self.types)
                .map_err(|e| self.source.locate(e, member.prop.span()))?;
        }
        let key = self.dictionary_key(member)?;
        if self.types.profile.mixed_index_records() {
            let Type::Dictionary(id) = base.ty else {
                return Err(invalid("missing dictionary type"));
            };
            let element = self.types.dictionaries[id as usize].value;
            let expected = declared.as_ref().map_or(element, |field| field.ty);
            let key = self.dictionary_key_value(key)?;
            let value = self.expr_expected(rhs, Some(expected))?;
            return self.dictionary_set(base, key, value, expected);
        }
        let block = self.dictionary_block(base, key)?;
        let element = block.dictionary.value;
        let expected = declared.as_ref().map_or(element, |field| field.ty);
        let value = self.expr_expected(rhs, Some(expected))?;
        self.dictionary_store(block, value, expected)
    }
    /// Store the evaluated `value` (of the key's `expected` type) under `key`.
    pub(super) fn dictionary_set(
        &mut self,
        base: Expr,
        key: Expr,
        value: Expr,
        expected: Type,
    ) -> Result<Expr> {
        if self.types.profile.mixed_index_records() {
            let Type::Dictionary(id) = base.ty else {
                return Err(invalid("missing dictionary type"));
            };
            let element = self.types.dictionaries[id as usize].value;
            let value = self.dictionary_value(value, expected, element)?;
            let key = self.dictionary_key_value(key)?;
            return self.dictionary_helper_call(DictionaryHelper::Set, vec![base, key, value]);
        }
        let block = self.dictionary_block(base, key)?;
        self.dictionary_store(block, value, expected)
    }
    /// Check `value` against the key's `expected` type and widen it to the
    /// index type `element` it is stored as.
    pub(super) fn dictionary_value(
        &mut self,
        value: Expr,
        expected: Type,
        element: Type,
    ) -> Result<Expr> {
        if !mixed_records::assignable(expected, value.ty, self.types) {
            return Err(self.source.locate(
                type_error(format!(
                    "dictionary value type mismatch: expected {expected}, found {}",
                    value.ty
                )),
                value.span,
            ));
        }
        let value = unions::widen(value, Some(expected), self.types)?;
        unions::widen(value, Some(element), self.types)
    }
    /// Store an evaluated `value` of the `expected` key type under the
    /// block's key: replace the entry in place or insert it at its own-key
    /// position. The expression's value is the stored `T`.
    pub(super) fn dictionary_store(
        &mut self,
        mut block: Block,
        value: Expr,
        expected: Type,
    ) -> Result<Expr> {
        let span = block.span;
        let element = block.dictionary.value;
        let value = self.dictionary_value(value, expected, element)?;
        let output = self.hidden(element);
        let index = self.hidden(Type::Float64);
        let rank = self.hidden(Type::Float64);
        let position = self.hidden(Type::Float64);
        let cursor = self.hidden(Type::Float64);
        let at = |slot| local(span, slot, Type::Float64);
        let proto = self.array_error(
            ErrorClass::TypeError,
            "Dictionary __proto__ assignment is unsupported",
            span,
        )?;
        let full = self.array_error(
            ErrorClass::RangeError,
            "Dictionary entry limit exceeded",
            span,
        )?;
        // Array-index keys form a sorted prefix; find the first larger one.
        let scan = binary(
            ast::BinaryOp::LogicalAnd,
            binary(ast::BinaryOp::Lt, at(position), length(block.keys())),
            binary(
                ast::BinaryOp::LogicalAnd,
                binary(
                    ast::BinaryOp::GtEq,
                    index_key(block.element(block.keys(), Type::String, at(position))),
                    number(span, 0.0),
                ),
                binary(
                    ast::BinaryOp::Lt,
                    index_key(block.element(block.keys(), Type::String, at(position))),
                    at(rank),
                ),
            ),
        );
        let push = |array: Expr, item: Expr| {
            let ty = array.ty;
            S::Expr(Expr {
                span,
                ty: Type::Float64,
                kind: E::ArrayPush(
                    Box::new(array),
                    Box::new(Expr {
                        span,
                        ty,
                        kind: E::Array(vec![Some(item)]),
                    }),
                ),
            })
        };
        let mut insert = vec![
            S::If(
                binary(
                    ast::BinaryOp::GtEq,
                    length(block.keys()),
                    number(span, MAX_DICTIONARY_ENTRIES as f64),
                ),
                vec![S::Throw(full)],
                vec![],
            ),
            S::Init(rank, index_key(block.key())),
            S::Init(position, length(block.keys())),
            S::If(
                binary(ast::BinaryOp::GtEq, at(rank), number(span, 0.0)),
                vec![
                    S::Set(position, number(span, 0.0)),
                    S::While(scan, vec![S::Set(position, add(at(position), 1.0))]),
                ],
                vec![],
            ),
            S::Init(cursor, length(block.keys())),
            push(block.keys(), block.key()),
            push(block.values(), local(span, output, element)),
        ];
        // Shift the tail up by one, then store the entry at its position.
        let mut shift = Vec::new();
        for keys in [true, false] {
            let (array, ty) = block.side(keys);
            let (source, _) = block.side(keys);
            shift.push(self.set_index(
                array,
                at(cursor),
                block.element(source, ty, add(at(cursor), -1.0)),
            ));
        }
        shift.push(S::Set(cursor, add(at(cursor), -1.0)));
        insert.push(S::While(
            binary(ast::BinaryOp::Gt, at(cursor), at(position)),
            shift,
        ));
        insert.push(self.set_index(block.keys(), at(position), block.key()));
        insert.push(self.set_index(block.values(), at(position), local(span, output, element)));
        // An absent `__proto__` key would invoke the prototype setter.
        let mut absent = vec![S::If(
            binary(
                ast::BinaryOp::EqEqEq,
                block.key(),
                Expr {
                    span,
                    ty: Type::String,
                    kind: E::String(self.types.profile.string_bytes(PROTO)),
                },
            ),
            vec![S::Throw(proto)],
            vec![],
        )];
        absent.extend(insert);
        let replace = self.set_index(block.values(), at(index), local(span, output, element));
        block.body.extend([
            S::Init(output, value),
            S::Init(index, block.search()),
            S::If(
                binary(ast::BinaryOp::GtEq, at(index), number(span, 0.0)),
                vec![replace],
                absent,
            ),
        ]);
        Ok(self.finish_dictionary(block, output, element))
    }
    fn set_index(&mut self, array: Expr, index: Expr, value: Expr) -> S {
        let slots = self
            .types
            .profile
            .sparse_arrays()
            .then(|| (self.hidden(array.ty), self.hidden(Type::Float64)));
        S::SetIndex {
            array,
            index,
            value,
            slots,
        }
    }
    /// `delete d[key]`: removes an own entry, keeping the order of the rest.
    pub(super) fn dictionary_delete(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
    ) -> Result<Expr> {
        match mixed_records::declared_field(base.ty, member, self.types) {
            // TypeScript (TS2790): only optional properties may be deleted.
            Some(field) if !field.optional => {
                return Err(self.source.locate(
                    type_error(format!(
                        "the operand of delete must be optional; {} is a required declared property",
                        field.name
                    )),
                    member.span,
                ));
            }
            Some(_) => {}
            None => mixed_records::object_key(base.ty, member, self.types)
                .map_err(|e| self.source.locate(e, member.prop.span()))?,
        }
        let key = self.dictionary_key(member)?;
        if self.types.profile.mixed_index_records() {
            let key = self.dictionary_key_value(key)?;
            return self.dictionary_helper_call(DictionaryHelper::Delete, vec![base, key]);
        }
        self.dictionary_delete_inline(base, key)
    }
    /// The inline body of `delete d[key]`.
    pub(super) fn dictionary_delete_inline(&mut self, base: Expr, key: Expr) -> Result<Expr> {
        let mut block = self.dictionary_block(base, key)?;
        let span = block.span;
        let output = self.hidden(Type::Boolean);
        let index = self.hidden(Type::Float64);
        let at = |slot| local(span, slot, Type::Float64);
        let mut shift = Vec::new();
        for keys in [true, false] {
            let (array, ty) = block.side(keys);
            let (source, _) = block.side(keys);
            shift.push(self.set_index(
                array,
                at(index),
                block.element(source, ty, add(at(index), 1.0)),
            ));
        }
        shift.push(S::Set(index, add(at(index), 1.0)));
        let mut remove = vec![S::While(
            binary(ast::BinaryOp::Lt, add(at(index), 1.0), length(block.keys())),
            shift,
        )];
        for keys in [true, false] {
            let error = self.array_error(ErrorClass::RangeError, "Invalid array length", span)?;
            let (array, _) = block.side(keys);
            let (current, _) = block.side(keys);
            remove.push(S::ArrayLength {
                length: add(length(current), -1.0),
                array,
                error,
            });
        }
        block.body.extend([
            S::Init(output, boolean(span, true)),
            S::Init(index, block.search()),
            S::If(
                binary(ast::BinaryOp::GtEq, at(index), number(span, 0.0)),
                remove,
                vec![],
            ),
        ]);
        Ok(self.finish_dictionary(block, output, Type::Boolean))
    }
    /// `Object.hasOwn(d, key)` (own) or `key in d` (own or an inherited
    /// `Object.prototype` member name).
    pub(super) fn dictionary_has(
        &mut self,
        value: Expr,
        key: Expr,
        inherited: bool,
    ) -> Result<Expr> {
        let mut block = self.dictionary_block(value, key)?;
        let span = block.span;
        let output = self.hidden(Type::Boolean);
        let mut miss = Vec::new();
        if inherited {
            let mut names: Vec<_> = records::INHERITED_NAMES.to_vec();
            names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let mut entries: Vec<_> = names
                .into_iter()
                .map(|name| {
                    (
                        self.types.profile.string_bytes(name),
                        vec![S::Set(output, boolean(span, true))],
                    )
                })
                .collect();
            miss = own_keys::lookup(span, block.key, &mut entries);
        }
        block.body.extend([
            S::Init(
                output,
                binary(ast::BinaryOp::GtEq, block.search(), number(span, 0.0)),
            ),
            S::If(local(span, output, Type::Boolean), vec![], miss),
        ]);
        Ok(self.finish_dictionary(block, output, Type::Boolean))
    }
    /// `Object.keys(d)` / `Object.values(d)`: a fresh array in own-key order.
    pub(super) fn dictionary_enumeration(&mut self, value: Expr, keys: bool) -> Result<Expr> {
        let Type::Dictionary(id) = value.ty else {
            return Err(invalid("missing dictionary type"));
        };
        let dictionary = self.types.dictionaries[id as usize];
        let span = value.span;
        let ty = value.ty;
        let source = self.hidden(ty);
        let (slot, array) = if keys {
            (KEYS, dictionary.keys)
        } else {
            (VALUES, dictionary.values)
        };
        let field = || Expr {
            span,
            ty: array,
            kind: E::Field(Box::new(local(span, source, ty)), slot),
        };
        let output = self.hidden(array);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: array,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![source, output],
                body: vec![
                    S::Init(source, value),
                    S::Init(
                        output,
                        Expr {
                            span,
                            ty: array,
                            kind: E::ArraySlice(
                                Box::new(field()),
                                Box::new(number(span, 0.0)),
                                Box::new(length(field())),
                            ),
                        },
                    ),
                ],
            },
        })
    }
    /// An object literal with a dictionary context: static keys only. Values
    /// run in source order; entries are stored in own-key order.
    pub(super) fn dictionary_literal(&mut self, object: &ast::ObjectLit, id: u32) -> Result<Expr> {
        if self.types.profile.mixed_index_records()
            && object
                .props
                .iter()
                .any(|property| matches!(property, ast::PropOrSpread::Spread(_)))
        {
            return self.dictionary_spread_literal(object, id);
        }
        let dictionary = self.types.dictionaries[id as usize];
        let span = object.span;
        if object.props.len() > MAX_DICTIONARY_ENTRIES {
            return Err(invalid("dictionary literal entry count exceeds limit"));
        }
        let declared = self
            .types
            .declared(Type::Dictionary(id))
            .map(|members| members.fields.clone())
            .unwrap_or_default();
        let mut entries: Vec<(String, u32)> = Vec::new();
        let mut body = Vec::new();
        let mut roots = Vec::new();
        for property in &object.props {
            let ast::PropOrSpread::Prop(property) = property else {
                return Err(unsupported("object spread"));
            };
            // A declared key (v35) is typed by its declared field.
            let expected = |name: &str| {
                declared
                    .iter()
                    .find(|field| field.name == name)
                    .map_or(dictionary.value, |field| field.ty)
            };
            let (name, value) = match &**property {
                ast::Prop::KeyValue(field) => {
                    let name = records::property_key(&field.key)?;
                    let value = self.expr_expected(&field.value, Some(expected(&name)))?;
                    (name, value)
                }
                ast::Prop::Shorthand(id) => {
                    let name = records::key(id.sym.to_string())?;
                    let value =
                        self.expr_expected(&ast::Expr::Ident(id.clone()), Some(expected(&name)))?;
                    (name, value)
                }
                _ => return Err(unsupported("object method, getter or setter")),
            };
            let field = expected(&name);
            if !mixed_records::assignable(field, value.ty, self.types) {
                return Err(self.source.locate(
                    type_error(format!(
                        "dictionary value type mismatch: expected {field}, found {}",
                        value.ty
                    )),
                    value.span,
                ));
            }
            if entries.iter().any(|(existing, _)| *existing == name) {
                return Err(unsupported("duplicate object key"));
            }
            let slot = self.hidden(dictionary.value);
            let value = unions::widen(value, Some(field), self.types)?;
            body.push(S::Init(
                slot,
                unions::widen(value, Some(dictionary.value), self.types)?,
            ));
            roots.push(slot);
            entries.push((name, slot));
        }
        if let Some(missing) = declared
            .iter()
            .find(|field| !field.optional && !entries.iter().any(|(name, _)| *name == field.name))
        {
            return Err(self.source.locate(
                type_error(format!(
                    "object literal is missing the required declared property {}",
                    missing.name
                )),
                span,
            ));
        }
        entries.sort_by_key(|(name, _)| {
            let units: Vec<u16> = name.encode_utf16().collect();
            match varta_common::value::array_index(&units) {
                Some(index) => (false, index),
                None => (true, 0),
            }
        });
        let keys = entries
            .iter()
            .map(|(name, _)| {
                Some(Expr {
                    span,
                    ty: Type::String,
                    kind: E::String(self.types.profile.string_bytes(name)),
                })
            })
            .collect();
        let values = entries
            .iter()
            .map(|(_, slot)| Some(local(span, *slot, dictionary.value)))
            .collect();
        let literal = Expr {
            span,
            ty: Type::Dictionary(id),
            kind: E::Record(vec![
                (
                    KEYS,
                    Expr {
                        span,
                        ty: dictionary.keys,
                        kind: E::Array(keys),
                    },
                ),
                (
                    VALUES,
                    Expr {
                        span,
                        ty: dictionary.values,
                        kind: E::Array(values),
                    },
                ),
            ]),
        };
        if body.is_empty() {
            return Ok(literal);
        }
        let output = self.hidden(Type::Dictionary(id));
        let marker = self.hidden(Type::Number);
        roots.push(output);
        body.push(S::Init(output, literal));
        Ok(Expr {
            span,
            ty: Type::Dictionary(id),
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
