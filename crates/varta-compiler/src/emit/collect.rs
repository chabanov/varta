use super::*;
type Location<'a> = (swc_common::Span, &'a crate::source::Source);
pub(super) struct Collected<'a> {
    pub hosts: BTreeSet<Host>,
    pub canonical_functions: BTreeSet<usize>,
    pub strings: BTreeMap<Vec<u8>, Location<'a>>,
    pub helpers: BTreeSet<strings::Helper>,
    pub layouts: BTreeMap<records::LayoutKey, Location<'a>>,
    pub arrays: bool,
    pub unions: bool,
    pub schemas: BTreeMap<Type, Location<'a>>,
    /// V38 (`json_decoders`): the checked parse targets of compiled decoders.
    pub decoders: BTreeSet<Type>,
    schema_types: BTreeSet<Type>,
    types: &'a ir::ValueTypes,
}
impl<'a> Collected<'a> {
    pub fn gather(program: &'a ir::Program) -> Self {
        let mut result = Self {
            hosts: BTreeSet::new(),
            canonical_functions: BTreeSet::new(),
            strings: BTreeMap::new(),
            helpers: BTreeSet::new(),
            layouts: BTreeMap::new(),
            arrays: false,
            unions: false,
            types: &program.types,
            schemas: BTreeMap::new(),
            decoders: BTreeSet::new(),
            schema_types: BTreeSet::new(),
        };
        for function in &program.functions {
            result.statements(&function.body, &function.source);
        }
        // Scheduled suspension points register their tokens (v34).
        if program
            .suspendable
            .values()
            .any(|machine| machine.scheduled)
        {
            result.hosts.extend([
                Host::PromiseReact,
                Host::SchedulerEnqueue,
                Host::PromiseHandled,
            ]);
        }
        result
    }
    /// V38 `charAt` and string index reads: the ASCII fast path in static
    /// data and the general path, the linked `string_char_at16` intrinsic
    /// with its snapshot.
    /// The helpers a string result of `host` needs: the allocator the
    /// linked v38 operation allocates its owned result with, or the
    /// snapshot that copies a result-buffer string.
    fn text_result(&mut self, host: Host) {
        self.helpers
            .insert(if crate::link::owned_text(self.types.profile, host) {
                strings::Helper::Allocate
            } else {
                strings::Helper::Snapshot
            });
    }
    fn char_at(&mut self, span: swc_common::Span, source: &'a crate::source::Source) {
        self.helpers
            .extend([strings::Helper::CharAt, strings::Helper::Allocate]);
        self.strings
            .entry(strings::ascii_units())
            .or_insert((span, source));
    }
    /// V38: the inline fast path in front of a linked primitive conversion,
    /// comparison or value18 reader, with what it calls.
    fn fast_path(
        &mut self,
        host: Host,
        args: &[Expr],
        span: swc_common::Span,
        source: &'a crate::source::Source,
    ) {
        let Some(helper) =
            strings::Helper::fast_path(self.types.profile, host, args, &self.types.unions)
        else {
            self.hosts.insert(host);
            return;
        };
        self.hosts.extend(helper.fast_path_hosts(host));
        {
            self.helpers.insert(helper);
            self.helpers.extend(helper.fast_path_dependencies());
            if helper.spells() {
                self.strings
                    .entry(strings::spellings())
                    .or_insert((span, source));
            }
        }
    }
    fn expression(&mut self, expr: &Expr, source: &'a crate::source::Source) {
        if expr.ty.is_cell() {
            self.unions = true;
        }
        match &expr.kind {
            E::ValueBlock { body, .. } => self.statements(body, source),
            E::Assignment { value, body, .. } => {
                self.expression(value, source);
                self.statements(body, source);
            }
            E::FunctionCheck { value, .. } => self.expression(value, source),
            E::FunctionRef { index, fresh, .. } => {
                self.helpers.insert(strings::Helper::Allocate);
                if !fresh {
                    self.canonical_functions.insert(*index);
                }
            }
            E::FunctionCall {
                callee,
                ignored_receiver,
                args,
                ..
            } => {
                self.expression(callee, source);
                if let Some(value) = ignored_receiver {
                    self.expression(value, source);
                }
                for arg in args {
                    self.expression(arg, source);
                }
            }
            E::StringSearch(mode, value, needle, position) => {
                self.helpers.insert(strings::Helper::string_search(*mode));
                self.expression(value, source);
                self.expression(needle, source);
                self.expression(position, source);
            }
            E::StringRange {
                value,
                start,
                end,
                substring,
            } => {
                self.helpers.insert(if *substring {
                    strings::Helper::StringSubstring
                } else {
                    strings::Helper::StringSlice
                });
                self.expression(value, source);
                self.expression(start, source);
                self.expression(end, source);
            }
            E::StringTrim { value, mode } => {
                self.helpers.insert(strings::Helper::string_trim(*mode));
                self.expression(value, source);
            }
            E::TextUnits { value, op } => {
                self.helpers.insert(strings::Helper::text_units(*op));
                self.expression(value, source);
            }
            E::StringRepeat {
                value,
                count,
                error,
                ..
            } => {
                self.helpers.insert(strings::Helper::Allocate);
                self.expression(value, source);
                self.expression(count, source);
                self.expression(error, source);
            }
            E::NewError(_, value) => {
                self.helpers.insert(strings::Helper::Allocate);
                self.expression(value, source);
            }
            E::ErrorOwn(value) | E::ErrorMessage(value) => self.expression(value, source),
            E::ErrorText(value) | E::ErrorName(value) => {
                if matches!(&expr.kind, E::ErrorText(_)) {
                    self.helpers.insert(strings::Helper::Concat);
                }
                for class in varta_common::value::ErrorClass::ALL {
                    for text in [class.name().to_owned(), format!("{}: ", class.name())] {
                        self.strings
                            .entry(self.types.profile.string_bytes(&text))
                            .or_insert((expr.span, source));
                    }
                }
                self.expression(value, source);
            }
            E::BoxUnion(value) => {
                self.helpers.insert(strings::Helper::Allocate);
                self.expression(value, source);
            }
            E::UnionLogical(op, a, b) => {
                let host =
                    self.types
                        .profile
                        .host_operation(if *op == BinaryOp::NullishCoalescing {
                            Host::UnionNullish
                        } else {
                            Host::UnionTruthy
                        });
                self.fast_path(host, &[], expr.span, source);
                self.expression(a, source);
                self.expression(b, source);
            }
            E::Conditional(c, a, b) => {
                self.expression(c, source);
                self.expression(a, source);
                self.expression(b, source);
            }

            E::ParseJson(text) => {
                self.hosts.insert(self.types.profile.json_parse());
                self.helpers.insert(strings::Helper::Allocate);
                if super::decoders::enabled(self.types.profile) {
                    self.decoders.insert(expr.ty);
                    self.helpers.insert(strings::Helper::RootPush);
                }
                self.schemas.entry(expr.ty).or_insert((expr.span, source));
                self.schema_type(expr.ty, (expr.span, source));
                self.expression(text, source);
            }
            E::ArraySearch(mode, base, needle, from) => {
                self.arrays = true;
                if *mode == ir::ArraySearch::LastIndexOf {
                    self.helpers.insert(strings::Helper::ArrayLastIndex);
                } else {
                    self.helpers.insert(strings::Helper::ArraySearch);
                }
                self.helpers
                    .insert(strings::Helper::equality(self.types.profile));
                self.expression(base, source);
                self.expression(needle, source);
                self.expression(from, source);
            }
            E::ArraySlice(base, start, end) => {
                self.arrays = true;
                self.helpers.insert(strings::Helper::ArraySlice);
                self.expression(base, source);
                self.expression(start, source);
                self.expression(end, source);
            }
            E::ArrayAt(base, index) => {
                if expr.ty != Type::Undefined {
                    self.helpers.insert(strings::Helper::Allocate);
                }
                self.expression(base, source);
                self.expression(index, source);
            }
            E::ArrayPush(base, values) => {
                self.expression(base, source);
                if let Some(appended) = super::arrays::appended(self.types.profile, values) {
                    self.arrays = true;
                    self.helpers
                        .insert(strings::Helper::ArrayAppend(appended.len() as u8));
                    for value in appended {
                        self.expression(value, source);
                    }
                } else {
                    self.helpers.insert(strings::Helper::ArrayPush);
                    self.expression(values, source);
                }
            }
            E::ArrayPop(base) => {
                self.helpers.insert(strings::Helper::Allocate);
                self.expression(base, source);
            }
            E::ArrayReverse(base) => {
                self.helpers.insert(strings::Helper::ArrayReverse);
                self.expression(base, source);
            }
            E::ArrayFill(base, value, start, end) => {
                self.helpers.insert(strings::Helper::ArrayFill);
                self.expression(base, source);
                self.expression(value, source);
                self.expression(start, source);
                self.expression(end, source);
            }
            E::ArrayEnumerate { value, keys } => {
                self.arrays = true;
                self.helpers.insert(if *keys {
                    strings::Helper::ArrayKeys
                } else {
                    strings::Helper::ArrayValues
                });
                self.expression(value, source);
            }
            E::ArrayReserve { length, .. } => {
                self.arrays = true;
                self.helpers.insert(strings::Helper::Allocate);
                self.expression(length, source);
            }
            E::Array(values) => {
                self.arrays = true;
                self.helpers.insert(strings::Helper::Allocate);
                for value in values.iter().flatten() {
                    self.expression(value, source);
                }
            }
            E::ArrayDelete {
                array,
                index,
                error,
            } => {
                self.arrays = true;
                self.expression(array, source);
                if let Some(index) = index {
                    self.expression(index, source);
                }
                if let Some(error) = error {
                    self.expression(error, source);
                }
            }
            E::RecordDelete { object, absent, .. } => {
                self.expression(object, source);
                self.expression(absent, source);
            }
            E::IndexKey(key) => {
                self.helpers.insert(strings::Helper::ArrayIndexKey);
                self.expression(key, source);
            }
            E::ArrayStringHasOwn(base, key) => {
                self.arrays = true;
                self.helpers.insert(strings::Helper::ArrayStringHasOwn);
                self.expression(base, source);
                self.expression(key, source);
            }
            E::ArrayHasOwn(base, index) => {
                self.arrays = true;
                self.expression(base, source);
                self.expression(index, source);
            }
            E::MaybeIndex(base, index) => {
                if expr.ty != Type::Undefined {
                    self.helpers.insert(strings::Helper::Allocate);
                }
                if base.ty == Type::String {
                    if crate::codegen_v2(self.types.profile) {
                        self.char_at(expr.span, source);
                    } else {
                        self.hosts.insert(Host::StringCharAt16);
                        self.helpers.insert(strings::Helper::Snapshot);
                    }
                }
                self.expression(base, source);
                self.expression(index, source);
            }
            E::HasOwn(value, _) => self.expression(value, source),
            E::Index(array, index) => {
                self.helpers.insert(strings::Helper::ArrayIndex);
                self.expression(array, source);
                self.expression(index, source);
            }
            E::String(s) => {
                if expr.ty == Type::BigInt {
                    self.helpers.insert(strings::Helper::Snapshot);
                }
                self.strings.entry(s.clone()).or_insert((expr.span, source));
            }
            E::Record(fields) | E::OptionalRecord(fields, _) => {
                if let Some(id) = self.types.record_layout(expr.ty) {
                    self.layouts
                        .entry(records::layout_key(id, fields, &self.types.records))
                        .or_insert((expr.span, source));
                    for field in &self.types.records[id as usize].fields {
                        self.strings
                            .entry(field.name.as_bytes().to_vec())
                            .or_insert((expr.span, source));
                    }
                }
                self.helpers.insert(strings::Helper::Allocate);
                for (_, value) in fields {
                    self.expression(value, source);
                }
            }
            E::KvStage(_, value) => {
                self.hosts.insert(Host::KvStage);
                self.expression(value, source);
            }
            E::KvDecode(host, index) => {
                self.hosts.insert(*host);
                self.helpers.insert(strings::Helper::Allocate);
                self.schemas.entry(expr.ty).or_insert((expr.span, source));
                self.schema_type(expr.ty, (expr.span, source));
                self.expression(index, source);
            }
            E::Json(value, _) => {
                let host = self.types.profile.json_stringify();
                self.hosts.insert(host);
                // V38 (`data_limits`): the host allocates and retains the owned
                // result through the exported value ABI helpers.
                if host == Host::JsonStringifyOwned {
                    self.helpers
                        .extend([strings::Helper::Allocate, strings::Helper::RootPush]);
                } else if matches!(
                    expr.ty,
                    Type::String | Type::Union(_) | Type::ReferenceUnion(_) | Type::Unknown
                ) {
                    self.helpers.insert(strings::Helper::Snapshot);
                }
                if expr.ty.is_cell() {
                    self.helpers.insert(strings::Helper::Allocate);
                }
                self.expression(value, source);
            }
            E::CheckedBigIntHost { host, args, .. } => {
                self.hosts.insert(*host);
                self.text_result(*host);
                for arg in args {
                    self.expression(arg, source);
                }
            }
            E::Host(host, args) => {
                if host.returns_string() {
                    self.text_result(*host);
                }
                self.fast_path(*host, args, expr.span, source);
                for arg in args {
                    self.expression(arg, source);
                }
            }
            E::RegExp(op, args) => {
                self.helpers.insert(strings::Helper::regexp(*op));
                for arg in args {
                    self.expression(arg, source);
                }
            }
            E::Math(mode, args) => {
                if *mode == ir::MathIntrinsic::Random {
                    self.hosts.insert(Host::MathRandom);
                }
                if *mode == ir::MathIntrinsic::Pow {
                    self.hosts.insert(Host::FloatPower);
                }
                if let ir::MathIntrinsic::Native(operation) = mode {
                    self.hosts.insert(operation.host());
                }
                for arg in args {
                    self.expression(arg, source);
                }
            }
            E::Intrinsic(op, args) => {
                match op {
                    ir::Intrinsic::CodeUnit => {
                        self.helpers.insert(strings::Helper::CodeUnit);
                    }
                    ir::Intrinsic::FromUnits => {
                        self.helpers.insert(strings::Helper::FromUnits);
                    }
                    ir::Intrinsic::Units => {
                        self.helpers
                            .insert(strings::Helper::Units(args.len() as u8));
                    }
                    ir::Intrinsic::Identity => {}
                    ir::Intrinsic::CharAt => self.char_at(expr.span, source),
                    ir::Intrinsic::CharCodeAt => {
                        self.helpers.insert(strings::Helper::CharCodeAt);
                    }
                    ir::Intrinsic::DictionaryFind => {
                        self.helpers.extend([
                            strings::Helper::DictionaryFind,
                            strings::Helper::StringEquals,
                        ]);
                    }
                    ir::Intrinsic::DictionaryUnlink => {
                        self.helpers.extend([
                            strings::Helper::DictionaryUnlink,
                            strings::Helper::StringEquals,
                        ]);
                    }
                    ir::Intrinsic::DictionaryPlace => {
                        self.helpers.insert(strings::Helper::DictionaryPlace);
                    }
                    ir::Intrinsic::DictionaryIndex => {
                        self.helpers.insert(strings::Helper::DictionaryIndex);
                    }
                    ir::Intrinsic::DictionaryInsertAt => {
                        self.helpers.insert(strings::Helper::DictionaryInsertAt);
                    }
                }
                for arg in args {
                    self.expression(arg, source);
                }
            }
            E::Call(_, args) | E::NumberPredicate(_, args) => {
                for arg in args {
                    self.expression(arg, source);
                }
            }
            E::NumberSameValue(a, b) => {
                self.expression(a, source);
                self.expression(b, source);
            }
            E::Binary(op, a, b) => {
                if a.ty == Type::Float64 && *op == BinaryOp::Mod {
                    self.hosts.insert(Host::FloatRemainder);
                    if crate::codegen_v2(self.types.profile) {
                        self.helpers.insert(strings::Helper::FloatRemainder);
                    }
                }
                if a.ty == Type::Float64 && *op == BinaryOp::Exp {
                    self.hosts.insert(Host::FloatPower);
                }
                if a.ty == Type::String
                    && *op == BinaryOp::Add
                    && crate::codegen_v2(self.types.profile)
                {
                    // V38: one helper call per chunk of the whole chain.
                    // A builder append (`s = s + chain`) emits the right
                    // chain alone.
                    let appended = super::concat_chain::leaves(b).len();
                    let leaves = super::concat_chain::leaves(expr);
                    for count in [leaves.len(), appended] {
                        for step in super::concat_chain::steps(count)
                            .into_iter()
                            .filter(|s| *s > 1)
                        {
                            self.helpers.insert(super::concat_chain::helper(step));
                        }
                    }
                    for leaf in leaves {
                        self.expression(leaf, source);
                    }
                    return;
                }
                if a.ty == Type::String {
                    self.helpers.insert(if *op == BinaryOp::Add {
                        strings::Helper::Concat
                    } else if crate::codegen_v2(self.types.profile)
                        && matches!(
                            op,
                            BinaryOp::EqEq | BinaryOp::EqEqEq | BinaryOp::NotEq | BinaryOp::NotEqEq
                        )
                    {
                        strings::Helper::StringEquals
                    } else {
                        strings::Helper::Compare
                    });
                }
                self.expression(a, source);
                self.expression(b, source);
            }
            E::Sequence(a, b) => {
                self.expression(a, source);
                self.expression(b, source);
            }
            E::IsError(a)
            | E::IsClass { value: a, .. }
            | E::ArrayKind(a, _)
            | E::IsUnionMember(a, _)
            | E::IsNullish(a)
            | E::IsUndefined(a)
            | E::UnboxUnion(a)
            | E::Negative(a)
            | E::Undefined(a)
            | E::ToBoolean(a)
            | E::Not(a)
            | E::Capture(a)
            | E::ResultField(_, a)
            | E::Field(a, _)
            | E::Length(a) => self.expression(a, source),
            E::Suspend { value, .. } => {
                self.helpers.insert(strings::Helper::Allocate);
                self.expression(value, source);
            }
            E::GeneratorStep {
                generator, value, ..
            } => {
                self.helpers.insert(strings::Helper::Allocate);
                self.expression(generator, source);
                self.expression(value, source);
            }
            E::GeneratorValue(value) | E::Reinterpret(value) => self.expression(value, source),
            E::Schedule { args, mode, .. } => {
                if *mode == ir::ScheduleMode::Enqueue {
                    self.hosts.insert(Host::SchedulerEnqueue);
                }
                for arg in args {
                    self.expression(arg, source);
                }
            }
            _ => {}
        }
    }
    fn schema_type(&mut self, ty: Type, location: Location<'a>) {
        if !self.schema_types.insert(ty) {
            return;
        }
        match ty {
            Type::Unknown => {
                self.unions = true;
                if let Some((array, dictionary)) =
                    crate::lower::json::unknown_graph_types(self.types)
                {
                    self.schema_type(Type::Array(array), location);
                    self.schema_type(Type::Dictionary(dictionary), location);
                }
            }
            Type::Union(id) | Type::ReferenceUnion(id) => {
                self.unions = true;
                let members = self.types.unions[id as usize].clone();
                for member in members {
                    self.schema_type(member, location);
                }
            }
            Type::Record(id) => {
                self.layouts
                    .entry(records::canonical_layout(id, &self.types.records))
                    .or_insert(location);
                let fields = self.types.records[id as usize].fields.clone();
                for field in fields {
                    self.strings
                        .entry(field.name.into_bytes())
                        .or_insert(location);
                    self.schema_type(field.ty, location);
                }
            }
            Type::Array(id) => {
                self.arrays = true;
                self.schema_type(self.types.arrays[id as usize], location);
            }
            Type::Dictionary(id) => {
                let dictionary = self.types.dictionaries[id as usize];
                self.arrays = true;
                self.layouts
                    .entry(records::canonical_layout(
                        dictionary.storage,
                        &self.types.records,
                    ))
                    .or_insert(location);
                for field in &self.types.records[dictionary.storage as usize].fields {
                    self.strings
                        .entry(field.name.as_bytes().to_vec())
                        .or_insert(location);
                }
                self.schema_type(dictionary.keys, location);
                self.schema_type(dictionary.value, location);
            }
            _ => {}
        }
    }
    fn statements(&mut self, body: &[S], source: &'a crate::source::Source) {
        for statement in body {
            match statement {
                S::RecordEnumerate { values, .. } => {
                    for value in values {
                        self.expression(value, source);
                    }
                    self.arrays = true;
                    if self.types.profile.sparse_arrays() {
                        self.helpers.insert(strings::Helper::ArraySetPresent);
                    }
                }
                S::Set(slot, e)
                    if crate::codegen_v2(self.types.profile)
                        && string_builder::append_operand(*slot, e).is_some() =>
                {
                    self.helpers.insert(strings::Helper::StringGrow);
                    self.expression(e, source);
                }
                S::Throw(e)
                | S::Init(_, e)
                | S::Set(_, e)
                | S::InitModuleConstant(_, e)
                | S::Expr(e)
                | S::Return(Some(e)) => self.expression(e, source),
                S::ArrayLength {
                    array,
                    length,
                    error,
                } => {
                    self.expression(array, source);
                    self.expression(length, source);
                    self.expression(error, source);
                }
                S::SetField(a, _, b) => {
                    self.expression(a, source);
                    self.expression(b, source);
                }
                S::SetIndex {
                    array: a,
                    index: i,
                    value: b,
                    ..
                } => {
                    self.helpers.insert(strings::Helper::ArrayIndex);
                    self.expression(a, source);
                    self.expression(i, source);
                    self.expression(b, source);
                }
                S::TryCatch { body, handler, .. } => {
                    self.statements(body, source);
                    self.statements(handler, source);
                }
                S::TryFinally {
                    body, finalizer, ..
                } => {
                    self.statements(body, source);
                    self.statements(finalizer, source);
                }
                S::Block(b) | S::Scope(_, b) | S::Label { body: b, .. } => {
                    self.statements(b, source)
                }
                S::If(e, a, b) => {
                    self.expression(e, source);
                    self.statements(a, source);
                    self.statements(b, source);
                }
                S::While(e, b) | S::DoWhile(e, b) => {
                    self.expression(e, source);
                    self.statements(b, source);
                }
                S::For(a, e, u, b) => {
                    self.statements(a, source);
                    if let Some(e) = e {
                        self.expression(e, source);
                    }
                    self.statements(u, source);
                    self.statements(b, source);
                }
                S::Switch { value, cases, .. } => {
                    self.expression(value, source);
                    for (e, b) in cases {
                        if let Some(e) = e {
                            self.expression(e, source);
                        }
                        self.statements(b, source);
                    }
                }
                _ => {}
            }
        }
    }
}
