//! Source semantics are versioned separately from the host and value layouts.
use serde::{Deserialize, Serialize};

pub const SECTION: &str = "varta.language";
pub const SCHEMA_VERSION: u32 = 1;
pub const MAX_METADATA_BYTES: usize = 1024;
pub const SUPPORTED_PROFILES: &[LanguageProfile] = &[
    LanguageProfile::CheckedI32Utf8V1,
    LanguageProfile::CheckedF64Utf8V1,
    LanguageProfile::CheckedF64Utf8V2,
    LanguageProfile::CheckedF64Utf16V1,
    LanguageProfile::CheckedF64Utf16V2,
    LanguageProfile::CheckedF64Utf16V3,
    LanguageProfile::CheckedF64Utf16V4,
    LanguageProfile::CheckedF64Utf16V5,
    LanguageProfile::CheckedF64Utf16V6,
    LanguageProfile::CheckedF64Utf16V7,
    LanguageProfile::CheckedF64Utf16V8,
    LanguageProfile::CheckedF64Utf16V9,
    LanguageProfile::CheckedF64Utf16V10,
    LanguageProfile::CheckedF64Utf16V11,
    LanguageProfile::CheckedF64Utf16V12,
    LanguageProfile::CheckedF64Utf16V13,
    LanguageProfile::CheckedF64Utf16V14,
    LanguageProfile::CheckedF64Utf16V15,
    LanguageProfile::CheckedF64Utf16V16,
    LanguageProfile::CheckedF64Utf16V17,
    LanguageProfile::CheckedF64Utf16V18,
    LanguageProfile::CheckedF64Utf16V19,
    LanguageProfile::CheckedF64Utf16V20,
    LanguageProfile::CheckedF64Utf16V21,
    LanguageProfile::CheckedF64Utf16V22,
    LanguageProfile::CheckedF64Utf16V23,
    LanguageProfile::CheckedF64Utf16V24,
    LanguageProfile::CheckedF64Utf16V25,
    LanguageProfile::CheckedF64Utf16V26,
    LanguageProfile::CheckedF64Utf16V27,
    LanguageProfile::CheckedF64Utf16V28,
    LanguageProfile::CheckedF64Utf16V29,
    LanguageProfile::CheckedF64Utf16V30,
    LanguageProfile::CheckedF64Utf16V31,
    LanguageProfile::CheckedF64Utf16V32,
    LanguageProfile::CheckedF64Utf16V33,
    LanguageProfile::CheckedF64Utf16V34,
    LanguageProfile::CheckedF64Utf16V35,
];

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LanguageProfile {
    #[default]
    #[serde(rename = "checked-i32-utf8-v1")]
    CheckedI32Utf8V1,
    #[serde(rename = "checked-f64-utf8-v1")]
    CheckedF64Utf8V1,
    #[serde(rename = "checked-f64-utf8-v2")]
    CheckedF64Utf8V2,
    #[serde(rename = "checked-f64-utf16-v1")]
    CheckedF64Utf16V1,
    #[serde(rename = "checked-f64-utf16-v2")]
    CheckedF64Utf16V2,
    #[serde(rename = "checked-f64-utf16-v3")]
    CheckedF64Utf16V3,
    #[serde(rename = "checked-f64-utf16-v4")]
    CheckedF64Utf16V4,
    #[serde(rename = "checked-f64-utf16-v5")]
    CheckedF64Utf16V5,
    #[serde(rename = "checked-f64-utf16-v6")]
    CheckedF64Utf16V6,
    #[serde(rename = "checked-f64-utf16-v7")]
    CheckedF64Utf16V7,
    #[serde(rename = "checked-f64-utf16-v8")]
    CheckedF64Utf16V8,
    #[serde(rename = "checked-f64-utf16-v9")]
    CheckedF64Utf16V9,
    #[serde(rename = "checked-f64-utf16-v10")]
    CheckedF64Utf16V10,
    #[serde(rename = "checked-f64-utf16-v11")]
    CheckedF64Utf16V11,
    #[serde(rename = "checked-f64-utf16-v12")]
    CheckedF64Utf16V12,
    #[serde(rename = "checked-f64-utf16-v13")]
    CheckedF64Utf16V13,
    #[serde(rename = "checked-f64-utf16-v14")]
    CheckedF64Utf16V14,
    #[serde(rename = "checked-f64-utf16-v15")]
    CheckedF64Utf16V15,
    #[serde(rename = "checked-f64-utf16-v16")]
    CheckedF64Utf16V16,
    #[serde(rename = "checked-f64-utf16-v17")]
    CheckedF64Utf16V17,
    #[serde(rename = "checked-f64-utf16-v18")]
    CheckedF64Utf16V18,
    #[serde(rename = "checked-f64-utf16-v19")]
    CheckedF64Utf16V19,
    #[serde(rename = "checked-f64-utf16-v20")]
    CheckedF64Utf16V20,
    #[serde(rename = "checked-f64-utf16-v21")]
    CheckedF64Utf16V21,
    #[serde(rename = "checked-f64-utf16-v22")]
    CheckedF64Utf16V22,
    #[serde(rename = "checked-f64-utf16-v23")]
    CheckedF64Utf16V23,
    #[serde(rename = "checked-f64-utf16-v24")]
    CheckedF64Utf16V24,
    #[serde(rename = "checked-f64-utf16-v25")]
    CheckedF64Utf16V25,
    #[serde(rename = "checked-f64-utf16-v26")]
    CheckedF64Utf16V26,
    #[serde(rename = "checked-f64-utf16-v27")]
    CheckedF64Utf16V27,
    #[serde(rename = "checked-f64-utf16-v28")]
    CheckedF64Utf16V28,
    #[serde(rename = "checked-f64-utf16-v29")]
    CheckedF64Utf16V29,
    #[serde(rename = "checked-f64-utf16-v30")]
    CheckedF64Utf16V30,
    #[serde(rename = "checked-f64-utf16-v31")]
    CheckedF64Utf16V31,
    #[serde(rename = "checked-f64-utf16-v32")]
    CheckedF64Utf16V32,
    #[serde(rename = "checked-f64-utf16-v33")]
    CheckedF64Utf16V33,
    #[serde(rename = "checked-f64-utf16-v34")]
    CheckedF64Utf16V34,
    #[serde(rename = "checked-f64-utf16-v35")]
    CheckedF64Utf16V35,
}
impl LanguageProfile {
    #[must_use]
    pub fn property_deletion(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    /// Pilot M1-M3 batch (v32). Each selector gates one independently developed
    /// feature; all admit v32 and the profiles that inherit it (v33, v34, v35).
    #[must_use]
    pub fn module_constants(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn literal_types(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn classes(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn module_reexports(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn template_literals(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn generics(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn default_exports(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn interface_extends(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn error_subclasses(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    /// K4.4 step 1 runtime admission (v33): continuation frames, function
    /// catalog version 2 in preflight and the exported `varta_resume`
    /// dispatcher, declared by the v33 host ABI and checked by
    /// [`crate::function::admission`]. Source reaches frames through
    /// [`Self::generators`].
    #[must_use]
    pub fn resumable_frames(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V33 | Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35
        )
    }
    /// K4.4 step 2 (v33): synchronous `function*` declarations with a
    /// `Generator<T, TReturn, TNext>` annotation, `yield`, the generator
    /// `next`/`throw`/`return` protocol and `for...of` with iterator closing,
    /// over guest-resumed frames. Generators add no value tag or value ABI.
    #[must_use]
    pub fn generators(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V33 | Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35
        )
    }
    /// Value layout 15 (v34 and v35): every value graph, descriptor, catalog, JSON
    /// schema and exception cell uses the value15 layouts, read only by the
    /// value15 host operations, JSON schema 15 and exception channel v6. The
    /// value15 layouts keep every value13 layout and tag and add the batch's own.
    #[must_use]
    pub fn dynamic_values(self) -> bool {
        matches!(self, Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35)
    }
    /// Pilot batch v34, records stream: string index signatures
    /// (`{[k: string]: T}`) and `Record<string, T>` as value15 dictionaries
    /// ([`crate::value::Dictionary`], tag [`crate::value::ValueKind::Dictionary`]).
    #[must_use]
    pub fn index_signatures(self) -> bool {
        matches!(self, Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35)
    }
    /// Pilot batch v34, records stream: non-generic recursive aliases and
    /// interfaces, such as a JSON value union, that refer to themselves through
    /// record fields, arrays or index signatures; JSON schema 15 refers forward.
    #[must_use]
    pub fn recursive_types(self) -> bool {
        matches!(self, Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35)
    }
    /// Pilot batch v34, records stream: records wider than
    /// [`crate::value::MAX_RECORD_FIELDS`] up to
    /// [`crate::value::MAX_WIDE_RECORD_FIELDS`] in the value15 wide layout.
    #[must_use]
    pub fn wide_records(self) -> bool {
        matches!(self, Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35)
    }
    /// Pilot batch v34, unknown stream: the `unknown` type, a reference-union
    /// cell holding any value with a cell tag, usable after narrowing by
    /// `typeof`, `===`, `instanceof` or a checked assertion; caught values and
    /// Error `cause` are `unknown`. Adds no value tag, layout or host import.
    #[must_use]
    pub fn unknown_type(self) -> bool {
        matches!(self, Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35)
    }
    /// Pilot batch v34, unknown stream: `x as T` and `x!` as runtime-checked
    /// conversions that throw `TypeError` on mismatch (upcasts are free); never
    /// an unchecked reinterpretation.
    #[must_use]
    pub fn checked_assertions(self) -> bool {
        matches!(self, Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35)
    }
    /// Pilot batch v34, async stream (K4.4 step 3): base `Promise<T>` semantics
    /// (`then`/`catch`/`finally`, resolve/reject, adoption, FIFO jobs and the
    /// unhandled-rejection policy) over a runtime microtask scheduler that
    /// drives `varta_resume`, declared by the v34 host ABI. Admits nothing yet.
    #[must_use]
    pub fn promises(self) -> bool {
        matches!(self, Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35)
    }
    /// Pilot batch v34, async stream (K4.4 step 4): `async` functions and
    /// `await` over the continuation frames and [`Self::promises`]. Admits
    /// nothing yet.
    #[must_use]
    pub fn async_functions(self) -> bool {
        matches!(self, Self::CheckedF64Utf16V34 | Self::CheckedF64Utf16V35)
    }
    /// Pilot batch v35, web stream: the engine-provided typed `Headers` class
    /// (case-insensitive names, ordered entries), implemented in the guest and
    /// requiring no capability. Admits nothing yet.
    #[must_use]
    pub fn web_headers(self) -> bool {
        self == Self::CheckedF64Utf16V35
    }
    /// Pilot batch v35, web stream: the `AbortController`/`AbortSignal` data
    /// model (`abort`, `aborted`, `reason`, `throwIfAborted`, listeners) without
    /// timers. Admits nothing yet.
    #[must_use]
    pub fn abort_signals(self) -> bool {
        self == Self::CheckedF64Utf16V35
    }
    /// Pilot batch v35, records stream: interfaces and object types that
    /// combine declared fields with a string index signature, plus `for...in`,
    /// compound assignment and spread on dictionaries. Admits nothing yet.
    #[must_use]
    pub fn mixed_index_records(self) -> bool {
        self == Self::CheckedF64Utf16V35
    }
    /// Pilot batch v35, records stream: `JSON.parse` and checked parsing that
    /// produce `unknown`, and narrowing `unknown` to dictionaries. Admits
    /// nothing yet.
    #[must_use]
    pub fn unknown_json(self) -> bool {
        self == Self::CheckedF64Utf16V35
    }
    /// Pilot batch v35, async stream: `async` class methods, async arrow
    /// functions and async function expressions over the continuation frames
    /// and [`Self::promises`]; async closure bodies keep their captured cells
    /// as shared frame slots.
    #[must_use]
    pub fn async_methods(self) -> bool {
        self == Self::CheckedF64Utf16V35
    }
    /// Pilot batch v35, async stream: value layout 16
    /// ([`crate::value::PROMISE_VALUE_VERSION`]), promises as tagged values
    /// (in arrays, unions and `unknown`) and the
    /// `Promise.all`/`allSettled`/`race`/`any` combinators.
    #[must_use]
    pub fn promise_values(self) -> bool {
        self == Self::CheckedF64Utf16V35
    }
    /// Pilot batch v35, frames stream: liveness-based continuation frame slots,
    /// an internal frame layout change for v35 modules only; earlier profiles
    /// keep their frame layout. Admits nothing yet.
    #[must_use]
    pub fn frame_liveness(self) -> bool {
        self == Self::CheckedF64Utf16V35
    }
    /// Named interface/alias annotations and type-only module bindings (v31).
    #[must_use]
    pub fn named_types(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn host_abi(self) -> u32 {
        match self {
            Self::CheckedI32Utf8V1 => crate::abi::VERSION,
            Self::CheckedF64Utf8V1 => crate::abi::NUMBER_VERSION,
            Self::CheckedF64Utf8V2 => crate::abi::UNDEFINED_VERSION,
            Self::CheckedF64Utf16V1 => crate::abi::UTF16_VERSION,
            Self::CheckedF64Utf16V2 => crate::abi::UNION_VERSION,
            Self::CheckedF64Utf16V3 => crate::abi::MISSING_VERSION,
            Self::CheckedF64Utf16V4 => crate::abi::REFERENCE_VERSION,
            Self::CheckedF64Utf16V5 => crate::abi::CHECKED_JSON_VERSION,
            Self::CheckedF64Utf16V6 => crate::abi::VECTOR_VERSION,
            Self::CheckedF64Utf16V7 => crate::abi::EXCEPTION_VERSION,
            Self::CheckedF64Utf16V8 => crate::abi::ERROR_VERSION,
            Self::CheckedF64Utf16V9 => crate::abi::ERROR_FAMILY_VERSION,
            Self::CheckedF64Utf16V10 => crate::abi::RECOVERABLE_JSON_VERSION,
            Self::CheckedF64Utf16V11 => crate::abi::FUNCTION_VERSION,
            Self::CheckedF64Utf16V12 => crate::abi::CLOSURE_VERSION,
            Self::CheckedF64Utf16V13 => crate::abi::SPARSE_VERSION,
            Self::CheckedF64Utf16V14 => crate::abi::ARRAY_MUTATION_VERSION,
            Self::CheckedF64Utf16V15 => crate::abi::NUMBER_POWER_VERSION,
            Self::CheckedF64Utf16V16 => crate::abi::NATIVE_MATH_VERSION,
            Self::CheckedF64Utf16V17 => crate::abi::RANDOM_VERSION,
            Self::CheckedF64Utf16V18 => crate::abi::NUMBER_TEXT_VERSION,
            Self::CheckedF64Utf16V19 => crate::abi::PRIMITIVE_STRING_VERSION,
            Self::CheckedF64Utf16V20 => crate::abi::PRIMITIVE_NUMBER_VERSION,
            Self::CheckedF64Utf16V21 => crate::abi::NUMBER_RADIX_VERSION,
            Self::CheckedF64Utf16V22 => crate::abi::NUMBER_FORMAT_VERSION,
            Self::CheckedF64Utf16V23
            | Self::CheckedF64Utf16V24
            | Self::CheckedF64Utf16V25
            | Self::CheckedF64Utf16V26
            | Self::CheckedF64Utf16V27 => crate::abi::NUMBER_PARSE_VERSION,
            Self::CheckedF64Utf16V28 => crate::abi::BIGINT_VERSION,
            Self::CheckedF64Utf16V29
            | Self::CheckedF64Utf16V30
            | Self::CheckedF64Utf16V31
            | Self::CheckedF64Utf16V32 => crate::abi::BIGINT_VALUES_VERSION,
            Self::CheckedF64Utf16V33 => crate::abi::RESUMABLE_FRAMES_VERSION,
            Self::CheckedF64Utf16V34 => crate::abi::SCHEDULED_FRAMES_VERSION,
            Self::CheckedF64Utf16V35 => crate::abi::PROMISE_VALUES_VERSION,
        }
    }
    #[must_use]
    pub fn binary64(self) -> bool {
        self != Self::CheckedI32Utf8V1
    }
    #[must_use]
    pub fn has_undefined(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf8V2
                | Self::CheckedF64Utf16V1
                | Self::CheckedF64Utf16V2
                | Self::CheckedF64Utf16V3
                | Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn utf16(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V1
                | Self::CheckedF64Utf16V2
                | Self::CheckedF64Utf16V3
                | Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn unions(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V2
                | Self::CheckedF64Utf16V3
                | Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn missing(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V3
                | Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn reference_unions(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V4
                | Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn checked_json(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V5
                | Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn resizable_arrays(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V6
                | Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn exceptions(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V7
                | Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn errors(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V8
                | Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn error_families(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V9
                | Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn recoverable_json(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V10
                | Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn function_values(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V11
                | Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn closures(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V12
                | Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn sparse_arrays(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V13
                | Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn array_mutations(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V14
                | Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn number_power(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn math_intrinsics(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn number_predicates(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn numeric_constants(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V15
                | Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn native_math(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V16
                | Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn random(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V17
                | Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn number_text(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V18
                | Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn primitive_string(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V19
                | Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn primitive_number(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V20
                | Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn number_radix(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V21
                | Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn number_format(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V22
                | Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn number_parse(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V23
                | Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn primitive_builtins(self) -> bool {
        self.number_parse()
    }
    #[must_use]
    pub fn truth_guards(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V24
                | Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn predicate_searches(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V25
                | Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn reverse_reduce(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V26
                | Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn bigint_values(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn bigint(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn array_inference(self) -> bool {
        matches!(
            self,
            Self::CheckedF64Utf16V27
                | Self::CheckedF64Utf16V28
                | Self::CheckedF64Utf16V29
                | Self::CheckedF64Utf16V30
                | Self::CheckedF64Utf16V31
                | Self::CheckedF64Utf16V32
                | Self::CheckedF64Utf16V33
                | Self::CheckedF64Utf16V34
                | Self::CheckedF64Utf16V35
        )
    }
    #[must_use]
    pub fn refinement_writes(self) -> bool {
        self.truth_guards()
    }
    #[must_use]
    pub fn string_bytes(self, text: &str) -> Vec<u8> {
        if self.utf16() {
            crate::text::encode(text)
        } else {
            text.as_bytes().to_vec()
        }
    }
    /// Select an explicit encoding-specific import; shared scalar imports stay unchanged.
    #[must_use]
    pub fn host_operation(self, operation: crate::HostOperation) -> crate::HostOperation {
        use crate::HostOperation::*;
        if self.promise_values() {
            // Value16 readers replace every value15 reader; nothing else changes.
            let value15 = LanguageProfile::CheckedF64Utf16V34.host_operation(operation);
            return match value15 {
                ValueLogV9 => ValueLogV10,
                ValueTypeOfV8 => ValueTypeOfV9,
                ValueTruthyV8 => ValueTruthyV9,
                ValueNullishV8 => ValueNullishV9,
                ValueStrictEqualV8 => ValueStrictEqualV9,
                ValueNullishEqualV8 => ValueNullishEqualV9,
                ValueIsArrayV8 => ValueIsArrayV9,
                ValueIsErrorClassV4 => ValueIsErrorClassV5,
                JsonStringifyV14 => JsonStringifyV15,
                JsonParseV16 => JsonParseV17,
                PrimitiveStringV3 => PrimitiveStringV4,
                PrimitiveNumberV3 => PrimitiveNumberV4,
                BigIntFromValueV2 => BigIntFromValueV3,
                other => other,
            };
        }
        if self.dynamic_values() {
            // Value15 readers replace every value13 reader; nothing else changes.
            let value13 = LanguageProfile::CheckedF64Utf16V33.host_operation(operation);
            return match value13 {
                ValueLogV8 => ValueLogV9,
                ValueTypeOfV7 => ValueTypeOfV8,
                ValueTruthyV7 => ValueTruthyV8,
                ValueNullishV7 => ValueNullishV8,
                ValueStrictEqualV7 => ValueStrictEqualV8,
                ValueNullishEqualV7 => ValueNullishEqualV8,
                ValueIsArrayV7 => ValueIsArrayV8,
                ValueIsErrorClassV3 => ValueIsErrorClassV4,
                JsonStringifyV13 => JsonStringifyV14,
                JsonParseV15 => JsonParseV16,
                PrimitiveStringV2 => PrimitiveStringV3,
                PrimitiveNumberV2 => PrimitiveNumberV3,
                BigIntFromValue => BigIntFromValueV2,
                other => other,
            };
        }
        if self.bigint_values() {
            return match operation {
                UnionLog | ValueLog | ValueLogV3 | ValueLogV4 | ValueLogV5 | ValueLogV6
                | ValueLogV7 => ValueLogV8,
                UnionTypeOf | ValueTypeOf | ValueTypeOfV3 | ValueTypeOfV4 | ValueTypeOfV5
                | ValueTypeOfV6 => ValueTypeOfV7,
                UnionTruthy | ValueTruthy | ValueTruthyV3 | ValueTruthyV4 | ValueTruthyV5
                | ValueTruthyV6 => ValueTruthyV7,
                UnionNullish | ValueNullish | ValueNullishV3 | ValueNullishV4 | ValueNullishV5
                | ValueNullishV6 => ValueNullishV7,
                UnionStrictEqual | ValueStrictEqual | ValueStrictEqualV3 | ValueStrictEqualV4
                | ValueStrictEqualV5 | ValueStrictEqualV6 => ValueStrictEqualV7,
                UnionNullishEqual | ValueNullishEqual | ValueNullishEqualV3
                | ValueNullishEqualV4 | ValueNullishEqualV5 | ValueNullishEqualV6 => {
                    ValueNullishEqualV7
                }
                ValueIsArray | ValueIsArrayV3 | ValueIsArrayV4 | ValueIsArrayV5
                | ValueIsArrayV6 => ValueIsArrayV7,
                ValueIsError | ValueIsErrorClass | ValueIsErrorClassV2 => ValueIsErrorClassV3,
                LogFloat => LogFloatV2,
                other => self.utf16_operation(other),
            };
        }
        if self.number_text() {
            match operation {
                LogFloat => return LogFloatV2,
                UnionLog | ValueLog | ValueLogV3 | ValueLogV4 | ValueLogV5 | ValueLogV6 => {
                    return ValueLogV7;
                }
                _ => {}
            }
        }
        if self.sparse_arrays() {
            return match operation {
                UnionLog | ValueLog | ValueLogV3 | ValueLogV4 | ValueLogV5 => ValueLogV6,
                UnionTypeOf | ValueTypeOf | ValueTypeOfV3 | ValueTypeOfV4 | ValueTypeOfV5 => {
                    ValueTypeOfV6
                }
                UnionTruthy | ValueTruthy | ValueTruthyV3 | ValueTruthyV4 | ValueTruthyV5 => {
                    ValueTruthyV6
                }
                UnionNullish | ValueNullish | ValueNullishV3 | ValueNullishV4 | ValueNullishV5 => {
                    ValueNullishV6
                }
                UnionStrictEqual | ValueStrictEqual | ValueStrictEqualV3 | ValueStrictEqualV4
                | ValueStrictEqualV5 => ValueStrictEqualV6,
                UnionNullishEqual | ValueNullishEqual | ValueNullishEqualV3
                | ValueNullishEqualV4 | ValueNullishEqualV5 => ValueNullishEqualV6,
                ValueIsArray | ValueIsArrayV3 | ValueIsArrayV4 | ValueIsArrayV5 => ValueIsArrayV6,
                ValueIsError | ValueIsErrorClass => ValueIsErrorClassV2,
                other => self.utf16_operation(other),
            };
        }

        if self.error_families() {
            match operation {
                UnionLog | ValueLog | ValueLogV3 | ValueLogV4 => return ValueLogV5,
                UnionTypeOf | ValueTypeOf | ValueTypeOfV3 | ValueTypeOfV4 => return ValueTypeOfV5,
                UnionTruthy | ValueTruthy | ValueTruthyV3 | ValueTruthyV4 => return ValueTruthyV5,
                UnionNullish | ValueNullish | ValueNullishV3 | ValueNullishV4 => {
                    return ValueNullishV5;
                }
                UnionStrictEqual | ValueStrictEqual | ValueStrictEqualV3 | ValueStrictEqualV4 => {
                    return ValueStrictEqualV5;
                }
                UnionNullishEqual | ValueNullishEqual | ValueNullishEqualV3
                | ValueNullishEqualV4 => return ValueNullishEqualV5,
                ValueIsArray | ValueIsArrayV3 | ValueIsArrayV4 => return ValueIsArrayV5,
                ValueIsError => return ValueIsErrorClass,
                _ => {}
            }
        }
        if self.errors() {
            match operation {
                UnionLog | ValueLog | ValueLogV3 => return ValueLogV4,
                UnionTypeOf | ValueTypeOf | ValueTypeOfV3 => return ValueTypeOfV4,
                UnionTruthy | ValueTruthy | ValueTruthyV3 => return ValueTruthyV4,
                UnionNullish | ValueNullish | ValueNullishV3 => return ValueNullishV4,
                UnionStrictEqual | ValueStrictEqual | ValueStrictEqualV3 => {
                    return ValueStrictEqualV4;
                }
                UnionNullishEqual | ValueNullishEqual | ValueNullishEqualV3 => {
                    return ValueNullishEqualV4;
                }
                ValueIsArray | ValueIsArrayV3 => return ValueIsArrayV4,
                _ => {}
            }
        }
        if self.resizable_arrays() {
            match operation {
                UnionLog | ValueLog => return ValueLogV3,
                UnionTypeOf | ValueTypeOf => return ValueTypeOfV3,
                UnionTruthy | ValueTruthy => return ValueTruthyV3,
                UnionNullish | ValueNullish => return ValueNullishV3,
                UnionStrictEqual | ValueStrictEqual => return ValueStrictEqualV3,
                UnionNullishEqual | ValueNullishEqual => return ValueNullishEqualV3,
                ValueIsArray => return ValueIsArrayV3,
                _ => {}
            }
        }
        if self.reference_unions() {
            match operation {
                UnionLog => return ValueLog,
                UnionTypeOf => return ValueTypeOf,
                UnionTruthy => return ValueTruthy,
                UnionNullish => return ValueNullish,
                UnionStrictEqual => return ValueStrictEqual,
                UnionNullishEqual => return ValueNullishEqual,
                _ => {}
            }
        }
        self.utf16_operation(operation)
    }
    fn utf16_operation(self, operation: crate::HostOperation) -> crate::HostOperation {
        use crate::HostOperation::*;
        if !self.utf16() {
            return operation;
        }
        match operation {
            LogString => LogString16,
            EnvHas => EnvHas16,
            EnvGet => EnvGet16,
            EnvPrint => EnvPrint16,
            FsRead => FsRead16,
            FsPrint => FsPrint16,
            NetFetch => NetFetch16,
            NetGet => NetGet16,
            other => other,
        }
    }
    #[must_use]
    pub fn exception_channel(self) -> (&'static str, i32) {
        use crate::exceptions as e;
        if self.promise_values() {
            (e::PROMISE_VERSION, e::PROMISE_CHANNEL_VERSION)
        } else if self.dynamic_values() {
            (e::DYNAMIC_VERSION, e::DYNAMIC_CHANNEL_VERSION)
        } else if self.error_subclasses() {
            (e::CLASS_ERROR_VERSION, e::CLASS_ERROR_CHANNEL_VERSION)
        } else if self.bigint_values() {
            (e::BIGINT_VERSION, e::BIGINT_CHANNEL_VERSION)
        } else if self.error_families() {
            (e::FAMILY_VERSION, e::FAMILY_CHANNEL_VERSION)
        } else {
            (e::VERSION, e::ERROR_CHANNEL_VERSION)
        }
    }
    #[must_use]
    pub fn value_abi(self) -> u32 {
        match self {
            Self::CheckedF64Utf16V29
            | Self::CheckedF64Utf16V30
            | Self::CheckedF64Utf16V31
            | Self::CheckedF64Utf16V32
            | Self::CheckedF64Utf16V33 => crate::value::BIGINT_VALUE_VERSION,
            Self::CheckedF64Utf16V34 => crate::value::DYNAMIC_VALUE_VERSION,
            Self::CheckedF64Utf16V35 => crate::value::PROMISE_VALUE_VERSION,
            Self::CheckedF64Utf16V13
            | Self::CheckedF64Utf16V14
            | Self::CheckedF64Utf16V15
            | Self::CheckedF64Utf16V16
            | Self::CheckedF64Utf16V17
            | Self::CheckedF64Utf16V18
            | Self::CheckedF64Utf16V19
            | Self::CheckedF64Utf16V20
            | Self::CheckedF64Utf16V21
            | Self::CheckedF64Utf16V22
            | Self::CheckedF64Utf16V23
            | Self::CheckedF64Utf16V24
            | Self::CheckedF64Utf16V25
            | Self::CheckedF64Utf16V26
            | Self::CheckedF64Utf16V27
            | Self::CheckedF64Utf16V28 => crate::value::SPARSE_VALUE_VERSION,
            Self::CheckedI32Utf8V1 => crate::value::VALUE_VERSION,
            Self::CheckedF64Utf8V1 => crate::value::NUMBER_VALUE_VERSION,
            Self::CheckedF64Utf8V2 => crate::value::UNDEFINED_VALUE_VERSION,
            Self::CheckedF64Utf16V1 => crate::value::UTF16_VALUE_VERSION,
            Self::CheckedF64Utf16V2 => crate::value::UNION_VALUE_VERSION,
            Self::CheckedF64Utf16V3 => crate::value::MISSING_VALUE_VERSION,
            Self::CheckedF64Utf16V8 => crate::value::ERROR_VALUE_VERSION,
            Self::CheckedF64Utf16V9
            | Self::CheckedF64Utf16V10
            | Self::CheckedF64Utf16V11
            | Self::CheckedF64Utf16V12 => crate::value::ERROR_FAMILY_VALUE_VERSION,
            Self::CheckedF64Utf16V6 | Self::CheckedF64Utf16V7 => crate::value::VECTOR_VALUE_VERSION,
            Self::CheckedF64Utf16V4 | Self::CheckedF64Utf16V5 => {
                crate::value::REFERENCE_VALUE_VERSION
            }
        }
    }
    /// Value-graph layout version that compiled code stamps into record
    /// descriptors and closure catalogs and that the profile's value, JSON and
    /// exception operations read. It equals [`Self::value_abi`] for every
    /// profile: v34 and v35 stamp the value15 layouts
    /// ([`crate::value::DYNAMIC_VALUE_VERSION`]) read by its value15 operations.
    #[must_use]
    pub fn value_layout(self) -> u32 {
        self.value_abi()
    }
    #[must_use]
    pub fn json_stringify(self) -> crate::HostOperation {
        match self {
            Self::CheckedF64Utf16V29
            | Self::CheckedF64Utf16V30
            | Self::CheckedF64Utf16V31
            | Self::CheckedF64Utf16V32
            | Self::CheckedF64Utf16V33 => crate::HostOperation::JsonStringifyV13,
            Self::CheckedF64Utf16V34 => crate::HostOperation::JsonStringifyV14,
            Self::CheckedF64Utf16V35 => crate::HostOperation::JsonStringifyV15,
            Self::CheckedF64Utf16V13
            | Self::CheckedF64Utf16V14
            | Self::CheckedF64Utf16V15
            | Self::CheckedF64Utf16V16
            | Self::CheckedF64Utf16V17
            | Self::CheckedF64Utf16V18
            | Self::CheckedF64Utf16V19
            | Self::CheckedF64Utf16V20
            | Self::CheckedF64Utf16V21
            | Self::CheckedF64Utf16V22
            | Self::CheckedF64Utf16V23
            | Self::CheckedF64Utf16V24
            | Self::CheckedF64Utf16V25
            | Self::CheckedF64Utf16V26
            | Self::CheckedF64Utf16V27
            | Self::CheckedF64Utf16V28 => crate::HostOperation::JsonStringifyV12,
            Self::CheckedI32Utf8V1 => crate::HostOperation::JsonStringify,
            Self::CheckedF64Utf8V1 => crate::HostOperation::JsonStringifyV3,
            Self::CheckedF64Utf8V2 => crate::HostOperation::JsonStringifyV4,
            Self::CheckedF64Utf16V1 => crate::HostOperation::JsonStringifyV5,
            Self::CheckedF64Utf16V2 => crate::HostOperation::JsonStringifyV6,
            Self::CheckedF64Utf16V3 => crate::HostOperation::JsonStringifyV7,
            Self::CheckedF64Utf16V8 => crate::HostOperation::JsonStringifyV10,
            Self::CheckedF64Utf16V9
            | Self::CheckedF64Utf16V10
            | Self::CheckedF64Utf16V11
            | Self::CheckedF64Utf16V12 => crate::HostOperation::JsonStringifyV11,
            Self::CheckedF64Utf16V6 | Self::CheckedF64Utf16V7 => {
                crate::HostOperation::JsonStringifyV9
            }
            Self::CheckedF64Utf16V4 | Self::CheckedF64Utf16V5 => {
                crate::HostOperation::JsonStringifyV8
            }
        }
    }
    #[must_use]
    pub fn json_parse(self) -> crate::HostOperation {
        match self {
            Self::CheckedF64Utf16V29
            | Self::CheckedF64Utf16V30
            | Self::CheckedF64Utf16V31
            | Self::CheckedF64Utf16V32
            | Self::CheckedF64Utf16V33 => crate::HostOperation::JsonParseV15,
            Self::CheckedF64Utf16V34 => crate::HostOperation::JsonParseV16,
            Self::CheckedF64Utf16V35 => crate::HostOperation::JsonParseV17,
            Self::CheckedF64Utf16V13
            | Self::CheckedF64Utf16V14
            | Self::CheckedF64Utf16V15
            | Self::CheckedF64Utf16V16
            | Self::CheckedF64Utf16V17
            | Self::CheckedF64Utf16V18
            | Self::CheckedF64Utf16V19
            | Self::CheckedF64Utf16V20
            | Self::CheckedF64Utf16V21
            | Self::CheckedF64Utf16V22
            | Self::CheckedF64Utf16V23
            | Self::CheckedF64Utf16V24
            | Self::CheckedF64Utf16V25
            | Self::CheckedF64Utf16V26
            | Self::CheckedF64Utf16V27
            | Self::CheckedF64Utf16V28 => crate::HostOperation::JsonParseV14,
            Self::CheckedI32Utf8V1 => crate::HostOperation::JsonParse,
            Self::CheckedF64Utf8V1 => crate::HostOperation::JsonParseV3,
            Self::CheckedF64Utf8V2 => crate::HostOperation::JsonParseV4,
            Self::CheckedF64Utf16V1 => crate::HostOperation::JsonParseV5,
            Self::CheckedF64Utf16V2 => crate::HostOperation::JsonParseV6,
            Self::CheckedF64Utf16V3 => crate::HostOperation::JsonParseV7,
            Self::CheckedF64Utf16V4 => crate::HostOperation::JsonParseV8,
            Self::CheckedF64Utf16V5 => crate::HostOperation::JsonParseV9,
            Self::CheckedF64Utf16V8 => crate::HostOperation::JsonParseV11,
            Self::CheckedF64Utf16V9 => crate::HostOperation::JsonParseV12,
            Self::CheckedF64Utf16V10 | Self::CheckedF64Utf16V11 | Self::CheckedF64Utf16V12 => {
                crate::HostOperation::JsonParseV13
            }
            Self::CheckedF64Utf16V6 | Self::CheckedF64Utf16V7 => crate::HostOperation::JsonParseV10,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LanguageContract {
    pub schema_version: u32,
    pub profile: LanguageProfile,
}
impl LanguageContract {
    #[must_use]
    pub fn current() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            profile: LanguageProfile::CheckedI32Utf8V1,
        }
    }
    pub fn parse(bytes: &[u8]) -> Result<Self, LanguageError> {
        if bytes.len() > MAX_METADATA_BYTES {
            return Err(LanguageError("metadata exceeds byte limit".into()));
        }
        let contract: Self =
            serde_json::from_slice(bytes).map_err(|e| LanguageError(e.to_string()))?;
        contract.validate()?;
        Ok(contract)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, LanguageError> {
        self.validate()?;
        serde_json::to_vec(self).map_err(|e| LanguageError(e.to_string()))
    }
    fn validate(&self) -> Result<(), LanguageError> {
        if self.schema_version != SCHEMA_VERSION || !SUPPORTED_PROFILES.contains(&self.profile) {
            return Err(LanguageError(
                "unsupported schema version or profile".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LanguageOrigin {
    Declared,
    LegacyHostAbi,
    Unspecified,
}

/// Validated declaration or legacy interpretation, never a proof of source
/// provenance or of arbitrary Wasm obeying a particular language's semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageInspection {
    pub profile: Option<LanguageProfile>,
    pub origin: LanguageOrigin,
}

#[derive(Debug, thiserror::Error)]
#[error("invalid language profile: {0}")]
pub struct LanguageError(pub String);

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn property_deletion_v30_reuses_v29_layouts_and_freezes_older_admission() {
        let new = LanguageProfile::CheckedF64Utf16V30;
        let old = LanguageProfile::CheckedF64Utf16V29;
        assert!(new.property_deletion());
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.property_deletion(),
                profile == new || profile.named_types()
            );
        }
        assert_eq!(new.host_abi(), old.host_abi());
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.exception_channel(), old.exception_channel());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        assert_eq!(
            serde_json::from_str::<LanguageProfile>("\"checked-f64-utf16-v30\"").unwrap(),
            new
        );
        assert_eq!(
            LanguageProfile::default(),
            LanguageProfile::CheckedI32Utf8V1
        );
    }

    #[test]
    fn named_types_v31_reuses_v30_layouts_and_freezes_older_admission() {
        let new = LanguageProfile::CheckedF64Utf16V31;
        let old = LanguageProfile::CheckedF64Utf16V30;
        assert!(new.named_types() && new.property_deletion() && new.bigint_values());
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.named_types(),
                profile == new
                    || profile == LanguageProfile::CheckedF64Utf16V32
                    || profile == LanguageProfile::CheckedF64Utf16V33
                    || profile == LanguageProfile::CheckedF64Utf16V34
                    || profile == LanguageProfile::CheckedF64Utf16V35
            );
        }
        assert_eq!(new.host_abi(), old.host_abi());
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.exception_channel(), old.exception_channel());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        assert_eq!(
            serde_json::from_str::<LanguageProfile>("\"checked-f64-utf16-v31\"").unwrap(),
            new
        );
    }

    #[test]
    fn pilot_batch_v32_reuses_v31_layouts_and_gates_each_feature() {
        let new = LanguageProfile::CheckedF64Utf16V32;
        let old = LanguageProfile::CheckedF64Utf16V31;
        assert!(new.named_types() && new.property_deletion() && new.bigint_values());
        for &profile in SUPPORTED_PROFILES {
            for selector in [
                profile.module_constants(),
                profile.literal_types(),
                profile.classes(),
                profile.module_reexports(),
                profile.template_literals(),
                profile.generics(),
                profile.default_exports(),
                profile.interface_extends(),
                profile.error_subclasses(),
            ] {
                assert_eq!(
                    selector,
                    profile == new
                        || profile == LanguageProfile::CheckedF64Utf16V33
                        || profile == LanguageProfile::CheckedF64Utf16V34
                        || profile == LanguageProfile::CheckedF64Utf16V35
                );
            }
        }
        assert_eq!(new.host_abi(), old.host_abi());
        assert_eq!(new.value_abi(), old.value_abi());
        // `error_subclasses()` selects its own channel; see the test below.
        assert_ne!(new.exception_channel(), old.exception_channel());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        assert_eq!(
            serde_json::from_str::<LanguageProfile>("\"checked-f64-utf16-v32\"").unwrap(),
            new
        );
    }

    #[test]
    fn suspension_v33_inherits_v32_with_a_new_host_abi_and_admits_nothing_yet() {
        let new = LanguageProfile::CheckedF64Utf16V33;
        let old = LanguageProfile::CheckedF64Utf16V32;
        assert!(new.named_types() && new.property_deletion() && new.bigint_values());
        assert!(
            new.module_constants()
                && new.literal_types()
                && new.classes()
                && new.module_reexports()
                && new.template_literals()
                && new.generics()
                && new.default_exports()
                && new.interface_extends()
                && new.error_subclasses()
        );
        let inherited = |profile: LanguageProfile| {
            profile == new
                || profile == LanguageProfile::CheckedF64Utf16V34
                || profile == LanguageProfile::CheckedF64Utf16V35
        };
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.resumable_frames(),
                inherited(profile),
                "{profile:?}"
            );
            assert_eq!(profile.generators(), inherited(profile), "{profile:?}");
        }
        assert_eq!(new.host_abi(), crate::abi::RESUMABLE_FRAMES_VERSION);
        assert_eq!(old.host_abi(), crate::abi::BIGINT_VALUES_VERSION);
        assert!(crate::abi::supported(new.host_abi()));
        assert!(new.host_abi() > old.host_abi());
        let users = SUPPORTED_PROFILES
            .iter()
            .filter(|p| p.host_abi() == crate::abi::RESUMABLE_FRAMES_VERSION);
        assert_eq!(users.collect::<Vec<_>>(), [&new]);
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.exception_channel(), old.exception_channel());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        assert_eq!(
            serde_json::from_str::<LanguageProfile>("\"checked-f64-utf16-v33\"").unwrap(),
            new
        );
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        let launcher = crate::launcher::LauncherInfo::current();
        assert!(launcher.language_profiles.contains(&new));
        assert!(launcher.host_abi > new.host_abi());
    }

    #[test]
    fn pilot_v34_inherits_v33_with_new_host_and_value_abis_and_admits_nothing_yet() {
        let new = LanguageProfile::CheckedF64Utf16V34;
        let old = LanguageProfile::CheckedF64Utf16V33;
        let inherited = |profile: LanguageProfile| {
            profile == new || profile == LanguageProfile::CheckedF64Utf16V35
        };
        assert!(new.named_types() && new.property_deletion() && new.bigint_values());
        assert!(
            new.module_constants()
                && new.literal_types()
                && new.classes()
                && new.module_reexports()
                && new.template_literals()
                && new.generics()
                && new.default_exports()
                && new.interface_extends()
                && new.error_subclasses()
                && new.resumable_frames()
                && new.generators()
        );
        for &profile in SUPPORTED_PROFILES {
            for selector in [
                profile.index_signatures(),
                profile.recursive_types(),
                profile.wide_records(),
                profile.unknown_type(),
                profile.checked_assertions(),
                profile.promises(),
                profile.async_functions(),
            ] {
                assert_eq!(selector, inherited(profile), "{profile:?}");
            }
        }
        // The scheduler host ABI and value ABI 15 belong to v34 alone; v35
        // inherits its selectors with the value16 ABIs.
        assert_eq!(new.host_abi(), crate::abi::SCHEDULED_FRAMES_VERSION);
        assert_eq!(old.host_abi(), crate::abi::RESUMABLE_FRAMES_VERSION);
        assert!(crate::abi::supported(new.host_abi()));
        assert_eq!(new.value_abi(), crate::value::DYNAMIC_VALUE_VERSION);
        assert!(crate::value::supported_version(new.value_abi()));
        assert!(!crate::value::compatible_version(
            old.value_abi(),
            new.value_abi()
        ));
        assert!(!crate::value::compatible_version(
            new.value_abi(),
            old.value_abi()
        ));
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.host_abi() == crate::abi::SCHEDULED_FRAMES_VERSION,
                profile == new
            );
            assert_eq!(
                profile.value_abi() == crate::value::DYNAMIC_VALUE_VERSION,
                profile == new
            );
            assert_eq!(profile.value_layout(), profile.value_abi(), "{profile:?}");
            assert_eq!(profile.dynamic_values(), inherited(profile), "{profile:?}");
        }
        // Value layout 15: v34 stamps value15 and reads it through channel v6,
        // JSON schema 15 and its own value/JSON operations. Every value13 tag
        // keeps its meaning under value15.
        assert_eq!(new.value_layout(), crate::value::DYNAMIC_VALUE_VERSION);
        assert_eq!(
            new.exception_channel(),
            (
                crate::exceptions::DYNAMIC_VERSION,
                crate::exceptions::DYNAMIC_CHANNEL_VERSION
            )
        );
        assert_eq!(new.json_parse(), crate::HostOperation::JsonParseV16);
        assert_eq!(new.json_stringify(), crate::HostOperation::JsonStringifyV14);
        for kind in (0..=u32::from(u8::MAX)).filter_map(crate::value::ValueKind::from_tag) {
            if kind == crate::value::ValueKind::Promise {
                // The value16 tag (v35 promise values).
                assert!(!kind.supported_by(crate::value::DYNAMIC_VALUE_VERSION));
                continue;
            }
            if kind == crate::value::ValueKind::Dictionary {
                // The one value15-only tag (index signatures).
                assert!(kind.supported_by(crate::value::DYNAMIC_VALUE_VERSION));
                assert!(
                    SUPPORTED_PROFILES
                        .iter()
                        .filter(|p| !inherited(**p))
                        .all(|p| !kind.supported_by(p.value_layout()))
                );
                continue;
            }
            assert_eq!(
                kind.supported_by(crate::value::DYNAMIC_VALUE_VERSION),
                kind.supported_by(crate::value::BIGINT_VALUE_VERSION),
                "{kind:?}"
            );
        }
        assert_eq!(
            serde_json::from_str::<LanguageProfile>("\"checked-f64-utf16-v34\"").unwrap(),
            new
        );
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        let launcher = crate::launcher::LauncherInfo::current();
        assert!(launcher.language_profiles.contains(&new));
        assert!(launcher.host_abi > new.host_abi());
        assert!(launcher.value_abi > new.value_abi());
    }

    #[test]
    fn pilot_v35_inherits_v34_with_value16_and_its_own_host_abi() {
        use crate::HostOperation as H;
        let new = LanguageProfile::CheckedF64Utf16V35;
        let old = LanguageProfile::CheckedF64Utf16V34;
        assert!(new.named_types() && new.property_deletion() && new.bigint_values());
        assert!(
            new.module_constants()
                && new.literal_types()
                && new.classes()
                && new.module_reexports()
                && new.template_literals()
                && new.generics()
                && new.default_exports()
                && new.interface_extends()
                && new.error_subclasses()
                && new.resumable_frames()
                && new.generators()
                && new.dynamic_values()
                && new.index_signatures()
                && new.recursive_types()
                && new.wide_records()
                && new.unknown_type()
                && new.checked_assertions()
                && new.promises()
                && new.async_functions()
        );
        for &profile in SUPPORTED_PROFILES {
            for selector in [
                profile.web_headers(),
                profile.abort_signals(),
                profile.mixed_index_records(),
                profile.unknown_json(),
                profile.async_methods(),
                profile.promise_values(),
                profile.frame_liveness(),
            ] {
                assert_eq!(selector, profile == new, "{profile:?}");
            }
        }
        // The promise-values stream owns the v35 ABI bumps: value ABI and
        // layout 16 (promise values), host ABI 31 (the value16 readers),
        // exception channel v7 and JSON schema 16. V34 keeps its own.
        assert_eq!(new.host_abi(), crate::abi::PROMISE_VALUES_VERSION);
        assert_eq!(old.host_abi(), crate::abi::SCHEDULED_FRAMES_VERSION);
        assert!(crate::abi::supported(new.host_abi()));
        assert!(!crate::abi::supported(
            crate::abi::PROMISE_VALUES_VERSION + 1
        ));
        assert_eq!(new.value_abi(), crate::value::PROMISE_VALUE_VERSION);
        assert_eq!(new.value_layout(), new.value_abi());
        assert_eq!(old.value_abi(), crate::value::DYNAMIC_VALUE_VERSION);
        assert!(crate::value::supported_version(new.value_abi()));
        assert!(!crate::value::supported_version(
            crate::value::PROMISE_VALUE_VERSION + 1
        ));
        assert!(!crate::value::compatible_version(
            old.value_abi(),
            new.value_abi()
        ));
        assert!(!crate::value::compatible_version(
            new.value_abi(),
            old.value_abi()
        ));
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.host_abi() == crate::abi::PROMISE_VALUES_VERSION,
                profile == new
            );
            assert_eq!(
                profile.value_abi() == crate::value::PROMISE_VALUE_VERSION,
                profile == new
            );
        }
        assert_eq!(
            new.exception_channel(),
            (
                crate::exceptions::PROMISE_VERSION,
                crate::exceptions::PROMISE_CHANNEL_VERSION
            )
        );
        assert_eq!(new.json_parse(), H::JsonParseV17);
        assert_eq!(new.json_stringify(), H::JsonStringifyV15);
        // Value16 keeps every value15 tag with its meaning and adds the
        // promise tag, which no other layout admits.
        use crate::value::ValueKind;
        for kind in (0..=u32::from(u8::MAX)).filter_map(ValueKind::from_tag) {
            if kind == ValueKind::Promise {
                assert!(kind.is_reference());
                assert!(kind.supported_by(crate::value::PROMISE_VALUE_VERSION));
                assert!(
                    SUPPORTED_PROFILES
                        .iter()
                        .filter(|p| **p != new)
                        .all(|p| !kind.supported_by(p.value_layout()))
                );
                continue;
            }
            assert_eq!(
                kind.supported_by(crate::value::PROMISE_VALUE_VERSION),
                kind.supported_by(crate::value::DYNAMIC_VALUE_VERSION),
                "{kind:?}"
            );
        }
        assert_eq!(
            crate::value::record_bytes(crate::value::PROMISE_VALUE_VERSION, 40),
            crate::value::record_bytes(crate::value::DYNAMIC_VALUE_VERSION, 40)
        );
        assert_eq!(
            crate::value::max_record_fields(crate::value::PROMISE_VALUE_VERSION),
            crate::value::MAX_WIDE_RECORD_FIELDS
        );
        // Operations without a layout binding are shared with v34.
        for operation in [H::LogString, H::LogFloat, H::FsRead, H::PromiseReact] {
            assert_eq!(
                new.host_operation(operation),
                old.host_operation(operation),
                "{operation:?}"
            );
        }
        assert_eq!(
            serde_json::from_str::<LanguageProfile>("\"checked-f64-utf16-v35\"").unwrap(),
            new
        );
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        assert!(serde_json::from_str::<LanguageProfile>("\"checked-f64-utf16-v36\"").is_err());
        assert!(
            LanguageContract::parse(br#"{"schemaVersion":1,"profile":"checked-f64-utf16-v36"}"#)
                .is_err()
        );
        let mut launcher = crate::launcher::LauncherInfo::current();
        assert!(launcher.language_profiles.contains(&new));
        assert_eq!(launcher.host_abi, new.host_abi());
        assert_eq!(launcher.value_abi, new.value_abi());
        launcher.language_profiles.retain(|p| *p != new);
        assert!(launcher.validate_current().is_err());
    }

    #[test]
    fn value16_operations_mirror_value15_and_bind_only_to_v35() {
        use crate::HostOperation as H;
        let new = LanguageProfile::CheckedF64Utf16V35;
        let old = LanguageProfile::CheckedF64Utf16V34;
        let pairs = [
            (H::ValueLogV10, H::ValueLogV9),
            (H::ValueTypeOfV9, H::ValueTypeOfV8),
            (H::ValueTruthyV9, H::ValueTruthyV8),
            (H::ValueNullishV9, H::ValueNullishV8),
            (H::ValueStrictEqualV9, H::ValueStrictEqualV8),
            (H::ValueNullishEqualV9, H::ValueNullishEqualV8),
            (H::ValueIsArrayV9, H::ValueIsArrayV8),
            (H::ValueIsErrorClassV5, H::ValueIsErrorClassV4),
            (H::JsonParseV17, H::JsonParseV16),
            (H::JsonStringifyV15, H::JsonStringifyV14),
            (H::PrimitiveStringV4, H::PrimitiveStringV3),
            (H::PrimitiveNumberV4, H::PrimitiveNumberV3),
            (H::BigIntFromValueV3, H::BigIntFromValueV2),
        ];
        for (value16, value15) in pairs {
            assert_eq!(value16.value15_counterpart(), Some(value15));
            assert_eq!(value15.value15_counterpart(), None);
            assert_eq!(value16.value13_counterpart(), None);
            assert_eq!(value16.scalar_signature(), value15.scalar_signature());
            assert_eq!(value16.returns_string(), value15.returns_string());
            assert_eq!(
                value16.needs_result_globals(),
                value15.needs_result_globals()
            );
            assert_eq!(value16.capability(), None);
            assert_eq!(value16.minimum_abi(), crate::abi::PROMISE_VALUES_VERSION);
            assert_eq!(H::from_name(value16.name()), Some(value16));
            assert_ne!(value16.name(), value15.name());
            // v35 replaces each value15 reader (and the value13 reader it
            // replaced); v34 never selects value16 and its host ABI is below
            // the operation's minimum.
            assert_eq!(new.host_operation(value15), value16);
            assert_eq!(new.host_operation(value16), value16);
            assert_eq!(
                new.host_operation(value15.value13_counterpart().unwrap()),
                value16
            );
            assert_eq!(old.host_operation(value15), value15);
            assert!(value16.minimum_abi() > old.host_abi());
        }
        for (generic, value16) in [
            (H::UnionLog, H::ValueLogV10),
            (H::ValueTypeOf, H::ValueTypeOfV9),
            (H::UnionTruthy, H::ValueTruthyV9),
            (H::ValueIsError, H::ValueIsErrorClassV5),
            (H::ValueIsArray, H::ValueIsArrayV9),
        ] {
            assert_eq!(new.host_operation(generic), value16);
            assert_ne!(old.host_operation(generic), value16);
        }
        assert_eq!(new.host_operation(H::LogString), H::LogString16);
    }

    #[test]
    fn value15_operations_mirror_value13_and_bind_only_to_v34() {
        use crate::HostOperation as H;
        let new = LanguageProfile::CheckedF64Utf16V34;
        let old = LanguageProfile::CheckedF64Utf16V33;
        let pairs = [
            (H::ValueLogV9, H::ValueLogV8),
            (H::ValueTypeOfV8, H::ValueTypeOfV7),
            (H::ValueTruthyV8, H::ValueTruthyV7),
            (H::ValueNullishV8, H::ValueNullishV7),
            (H::ValueStrictEqualV8, H::ValueStrictEqualV7),
            (H::ValueNullishEqualV8, H::ValueNullishEqualV7),
            (H::ValueIsArrayV8, H::ValueIsArrayV7),
            (H::ValueIsErrorClassV4, H::ValueIsErrorClassV3),
            (H::JsonParseV16, H::JsonParseV15),
            (H::JsonStringifyV14, H::JsonStringifyV13),
            (H::PrimitiveStringV3, H::PrimitiveStringV2),
            (H::PrimitiveNumberV3, H::PrimitiveNumberV2),
            (H::BigIntFromValueV2, H::BigIntFromValue),
        ];
        for (value15, value13) in pairs {
            assert_eq!(value15.value13_counterpart(), Some(value13));
            assert_eq!(value13.value13_counterpart(), None);
            assert_eq!(value15.scalar_signature(), value13.scalar_signature());
            assert_eq!(value15.returns_string(), value13.returns_string());
            assert_eq!(
                value15.needs_result_globals(),
                value13.needs_result_globals()
            );
            assert_eq!(value15.capability(), None);
            assert_eq!(value15.minimum_abi(), crate::abi::SCHEDULED_FRAMES_VERSION);
            assert_eq!(H::from_name(value15.name()), Some(value15));
            assert_ne!(value15.name(), value13.name());
            // v34 replaces each value13 reader; v33 never selects value15 and
            // its host ABI is below the operation's minimum.
            assert_eq!(new.host_operation(value13), value15);
            assert_eq!(new.host_operation(value15), value15);
            assert_eq!(old.host_operation(value13), value13);
            assert!(value15.minimum_abi() > old.host_abi());
        }
        // Generic selectors reach the value15 readers under v34 only.
        for (generic, value15) in [
            (H::UnionLog, H::ValueLogV9),
            (H::ValueTypeOf, H::ValueTypeOfV8),
            (H::UnionTruthy, H::ValueTruthyV8),
            (H::ValueIsError, H::ValueIsErrorClassV4),
            (H::ValueIsArray, H::ValueIsArrayV8),
        ] {
            assert_eq!(new.host_operation(generic), value15);
            assert_ne!(old.host_operation(generic), value15);
        }
        assert_eq!(new.host_operation(H::LogString), H::LogString16);
        assert_eq!(new.host_operation(H::LogFloat), H::LogFloatV2);
    }

    #[test]
    fn error_subclasses_select_channel_v5_from_v32() {
        use crate::exceptions as e;
        for &profile in SUPPORTED_PROFILES {
            let channel = profile.exception_channel();
            assert_eq!(
                channel == (e::CLASS_ERROR_VERSION, e::CLASS_ERROR_CHANNEL_VERSION),
                profile.error_subclasses() && !profile.dynamic_values(),
                "{profile:?}"
            );
            assert_eq!(
                channel == (e::DYNAMIC_VERSION, e::DYNAMIC_CHANNEL_VERSION),
                profile.dynamic_values() && !profile.promise_values(),
                "{profile:?}"
            );
            assert_eq!(
                channel == (e::PROMISE_VERSION, e::PROMISE_CHANNEL_VERSION),
                profile.promise_values(),
                "{profile:?}"
            );
        }
        assert!(e::DYNAMIC_VERSION.starts_with(e::PREFIX));
        const { assert!(e::CLASS_ERROR_CHANNEL_VERSION < e::DYNAMIC_CHANNEL_VERSION) };
        assert_eq!(
            LanguageProfile::CheckedF64Utf16V31.exception_channel(),
            (e::BIGINT_VERSION, e::BIGINT_CHANNEL_VERSION)
        );
        assert!(e::CLASS_ERROR_VERSION.starts_with(e::PREFIX));
        assert!(
            [
                e::ERROR_CHANNEL_VERSION,
                e::FAMILY_CHANNEL_VERSION,
                e::BIGINT_CHANNEL_VERSION
            ]
            .iter()
            .all(|old| *old < e::CLASS_ERROR_CHANNEL_VERSION)
        );
    }

    #[test]
    fn bigint_values_v29_versions_tags_json_and_exception_channels() {
        use crate::{HostOperation as H, HostScalar::*};
        let new = LanguageProfile::CheckedF64Utf16V29;
        let old = LanguageProfile::CheckedF64Utf16V28;
        assert_eq!(new.host_abi(), crate::abi::BIGINT_VALUES_VERSION);
        assert_eq!(new.value_abi(), crate::value::BIGINT_VALUE_VERSION);
        assert_eq!(old.host_abi(), crate::abi::BIGINT_VERSION);
        assert_eq!(old.value_abi(), crate::value::SPARSE_VALUE_VERSION);
        assert_eq!(
            new.exception_channel(),
            (
                crate::exceptions::BIGINT_VERSION,
                crate::exceptions::BIGINT_CHANNEL_VERSION
            )
        );
        assert_eq!(
            old.exception_channel(),
            (
                crate::exceptions::FAMILY_VERSION,
                crate::exceptions::FAMILY_CHANNEL_VERSION
            )
        );
        assert_eq!(new.json_stringify(), H::JsonStringifyV13);
        assert_eq!(new.json_parse(), H::JsonParseV15);
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.bigint_values(),
                profile == new
                    || profile == LanguageProfile::CheckedF64Utf16V30
                    || profile == LanguageProfile::CheckedF64Utf16V31
                    || profile == LanguageProfile::CheckedF64Utf16V32
                    || profile == LanguageProfile::CheckedF64Utf16V33
                    || profile == LanguageProfile::CheckedF64Utf16V34
                    || profile == LanguageProfile::CheckedF64Utf16V35
            );
        }
        for (host, args, results) in [
            (H::ValueLogV8, &[I64][..], &[][..]),
            (H::ValueTypeOfV7, &[I64][..], &[I32][..]),
            (H::ValueTruthyV7, &[I64][..], &[I32][..]),
            (H::ValueNullishV7, &[I64][..], &[I32][..]),
            (H::ValueIsArrayV7, &[I64][..], &[I32][..]),
            (H::ValueStrictEqualV7, &[I64, I64][..], &[I32][..]),
            (H::ValueNullishEqualV7, &[I64, I64][..], &[I32][..]),
            (H::ValueIsErrorClassV3, &[I64, I32][..], &[I32][..]),
            (H::JsonStringifyV13, &[I32, I32, I32][..], &[I32, I32][..]),
            (H::JsonParseV15, &[I32, I32, I32, I32][..], &[I64][..]),
            (H::PrimitiveStringV2, &[I64][..], &[I32][..]),
            (H::PrimitiveNumberV2, &[I64][..], &[F64][..]),
            (H::BigIntFromValue, &[I64][..], &[I32, I32][..]),
            (H::PrimitiveCompare, &[I64, I64][..], &[F64][..]),
            (H::PrimitiveEqual, &[I32, I64, I64][..], &[I32][..]),
            (H::BigIntWidthV2, &[I32, I64, I64][..], &[I32, I32][..]),
            (H::BigIntToStringV2, &[I64, I64][..], &[I32, I32][..]),
        ] {
            assert_eq!(host.scalar_signature(), (args, results));
            assert_eq!(host.minimum_abi(), crate::abi::BIGINT_VALUES_VERSION);
            assert_eq!(host.capability(), None);
            assert_eq!(H::from_name(host.name()), Some(host));
        }
    }
    #[test]
    fn bigint_v28_negotiates_pure_typed_hosts_without_reinterpreting_old_values() {
        use crate::{HostOperation as H, HostScalar::*};
        let profile = LanguageProfile::CheckedF64Utf16V28;
        assert_eq!(profile.host_abi(), crate::abi::BIGINT_VERSION);
        assert_eq!(
            profile.value_abi(),
            LanguageProfile::CheckedF64Utf16V27.value_abi()
        );
        assert!(
            profile.bigint()
                && profile.array_inference()
                && profile.reverse_reduce()
                && profile.primitive_builtins()
        );
        for &old in SUPPORTED_PROFILES {
            assert_eq!(old.bigint(), old == profile || old.bigint_values());
        }
        for (host, args, result, buffer) in [
            (H::BigIntBinary, &[I32, I64, I64][..], &[I32, I32][..], true),
            (H::BigIntUnary, &[I32, I64][..], &[I32, I32][..], true),
            (H::BigIntFromNumber, &[F64][..], &[I32, I32][..], true),
            (H::BigIntFromString, &[I32, I32][..], &[I32, I32][..], true),
            (H::BigIntToString, &[I64, F64][..], &[I32, I32][..], true),
            (H::BigIntWidth, &[I32, I64, F64][..], &[I32, I32][..], true),
            (H::BigIntToNumber, &[I64][..], &[F64][..], false),
            (H::BigIntCompare, &[I64, I64][..], &[F64][..], false),
            (H::BigIntCompareNumber, &[I64, F64][..], &[F64][..], false),
            (
                H::BigIntCompareString,
                &[I64, I32, I32][..],
                &[F64][..],
                false,
            ),
        ] {
            assert_eq!(host.scalar_signature(), (args, result));
            assert_eq!(host.minimum_abi(), crate::abi::BIGINT_VERSION);
            assert_eq!(H::from_name(host.name()), Some(host));
            assert_eq!(host.capability(), None);
            assert_eq!(host.needs_result_globals(), buffer);
        }
    }
    #[test]
    fn array_inference_v27_inherits_v26_with_shared_abi_and_exact_negotiation() {
        let old = LanguageProfile::CheckedF64Utf16V26;
        let new = LanguageProfile::CheckedF64Utf16V27;
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.array_inference(),
                profile == new || profile.bigint()
            );
        }
        assert!(
            new.reverse_reduce()
                && new.predicate_searches()
                && new.truth_guards()
                && new.refinement_writes()
                && new.primitive_builtins()
        );
        assert_eq!(new.host_abi(), old.host_abi());
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        let mut launcher = crate::launcher::LauncherInfo::current();
        assert!(launcher.language_profiles.contains(&new));
        launcher.language_profiles.retain(|p| *p != new);
        assert!(launcher.validate_current().is_err());
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn reverse_reduce_v26_inherits_v25_with_exact_profile_negotiation() {
        let old = LanguageProfile::CheckedF64Utf16V25;
        let new = LanguageProfile::CheckedF64Utf16V26;
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.reverse_reduce(),
                profile == new
                    || matches!(
                        profile,
                        LanguageProfile::CheckedF64Utf16V27
                            | LanguageProfile::CheckedF64Utf16V28
                            | LanguageProfile::CheckedF64Utf16V29
                            | LanguageProfile::CheckedF64Utf16V30
                            | LanguageProfile::CheckedF64Utf16V31
                            | LanguageProfile::CheckedF64Utf16V32
                            | LanguageProfile::CheckedF64Utf16V33
                            | LanguageProfile::CheckedF64Utf16V34
                            | LanguageProfile::CheckedF64Utf16V35
                    )
            );
        }
        assert!(
            new.predicate_searches()
                && new.truth_guards()
                && new.refinement_writes()
                && new.closures()
                && new.sparse_arrays()
                && new.array_mutations()
                && new.primitive_builtins()
        );
        assert_eq!(new.host_abi(), old.host_abi());
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        let mut launcher = crate::launcher::LauncherInfo::current();
        assert!(launcher.language_profiles.contains(&new));
        launcher.language_profiles.retain(|p| *p != new);
        assert!(launcher.validate_current().is_err());
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn predicate_searches_v25_inherit_v24_and_share_abi_with_exact_negotiation() {
        let old = LanguageProfile::CheckedF64Utf16V24;
        let new = LanguageProfile::CheckedF64Utf16V25;
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.predicate_searches(),
                profile == new
                    || profile == LanguageProfile::CheckedF64Utf16V26
                    || matches!(
                        profile,
                        LanguageProfile::CheckedF64Utf16V27
                            | LanguageProfile::CheckedF64Utf16V28
                            | LanguageProfile::CheckedF64Utf16V29
                            | LanguageProfile::CheckedF64Utf16V30
                            | LanguageProfile::CheckedF64Utf16V31
                            | LanguageProfile::CheckedF64Utf16V32
                            | LanguageProfile::CheckedF64Utf16V33
                            | LanguageProfile::CheckedF64Utf16V34
                            | LanguageProfile::CheckedF64Utf16V35
                    )
            );
        }
        assert!(
            new.truth_guards()
                && new.refinement_writes()
                && new.closures()
                && new.sparse_arrays()
                && new.array_mutations()
                && new.primitive_builtins()
        );
        assert_eq!(new.host_abi(), old.host_abi());
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        let mut launcher = crate::launcher::LauncherInfo::current();
        assert!(launcher.language_profiles.contains(&new));
        assert_eq!(launcher.host_abi, crate::abi::PROMISE_VALUES_VERSION);
        launcher.language_profiles.retain(|p| *p != new);
        assert!(launcher.validate_current().is_err());
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn truth_guards_v24_versions_source_rules_without_new_host_or_value_abi() {
        let old = LanguageProfile::CheckedF64Utf16V23;
        let new = LanguageProfile::CheckedF64Utf16V24;
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.truth_guards(),
                profile == new
                    || profile == LanguageProfile::CheckedF64Utf16V25
                    || profile == LanguageProfile::CheckedF64Utf16V26
                    || matches!(
                        profile,
                        LanguageProfile::CheckedF64Utf16V27
                            | LanguageProfile::CheckedF64Utf16V28
                            | LanguageProfile::CheckedF64Utf16V29
                            | LanguageProfile::CheckedF64Utf16V30
                            | LanguageProfile::CheckedF64Utf16V31
                            | LanguageProfile::CheckedF64Utf16V32
                            | LanguageProfile::CheckedF64Utf16V33
                            | LanguageProfile::CheckedF64Utf16V34
                            | LanguageProfile::CheckedF64Utf16V35
                    )
            );
            assert_eq!(
                profile.refinement_writes(),
                profile == new
                    || profile == LanguageProfile::CheckedF64Utf16V25
                    || profile == LanguageProfile::CheckedF64Utf16V26
                    || matches!(
                        profile,
                        LanguageProfile::CheckedF64Utf16V27
                            | LanguageProfile::CheckedF64Utf16V28
                            | LanguageProfile::CheckedF64Utf16V29
                            | LanguageProfile::CheckedF64Utf16V30
                            | LanguageProfile::CheckedF64Utf16V31
                            | LanguageProfile::CheckedF64Utf16V32
                            | LanguageProfile::CheckedF64Utf16V33
                            | LanguageProfile::CheckedF64Utf16V34
                            | LanguageProfile::CheckedF64Utf16V35
                    )
            );
        }
        assert!(
            new.primitive_builtins()
                && new.number_parse()
                && new.number_format()
                && new.number_radix()
        );
        assert_eq!(new.host_abi(), old.host_abi());
        assert_eq!(new.host_abi(), crate::abi::NUMBER_PARSE_VERSION);
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        let mut launcher = crate::launcher::LauncherInfo::current();
        assert!(launcher.language_profiles.contains(&new));
        launcher.language_profiles.retain(|p| *p != new);
        assert!(launcher.validate_current().is_err());
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn primitive_builtins_share_v23_without_new_host_or_value_abi() {
        let new = LanguageProfile::CheckedF64Utf16V23;
        for &profile in SUPPORTED_PROFILES {
            assert_eq!(
                profile.primitive_builtins(),
                profile == new
                    || profile == LanguageProfile::CheckedF64Utf16V24
                    || profile == LanguageProfile::CheckedF64Utf16V25
                    || profile == LanguageProfile::CheckedF64Utf16V26
                    || matches!(
                        profile,
                        LanguageProfile::CheckedF64Utf16V27
                            | LanguageProfile::CheckedF64Utf16V28
                            | LanguageProfile::CheckedF64Utf16V29
                            | LanguageProfile::CheckedF64Utf16V30
                            | LanguageProfile::CheckedF64Utf16V31
                            | LanguageProfile::CheckedF64Utf16V32
                            | LanguageProfile::CheckedF64Utf16V33
                            | LanguageProfile::CheckedF64Utf16V34
                            | LanguageProfile::CheckedF64Utf16V35
                    )
            );
        }
        assert_eq!(new.host_abi(), crate::abi::NUMBER_PARSE_VERSION);
        assert_eq!(
            new.value_abi(),
            LanguageProfile::CheckedF64Utf16V22.value_abi()
        );
        assert_eq!(
            new.host_operation(crate::HostOperation::PrimitiveNumber),
            crate::HostOperation::PrimitiveNumber
        );
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn number_parse_profile_versions_both_primitive_cell_operations() {
        use crate::{HostOperation as H, HostScalar};
        let old = LanguageProfile::CheckedF64Utf16V22;
        let new = LanguageProfile::CheckedF64Utf16V23;
        assert!(!old.number_parse());
        assert!(
            new.number_parse()
                && new.number_format()
                && new.number_radix()
                && new.primitive_number()
        );
        assert_eq!(new.host_abi(), crate::abi::NUMBER_PARSE_VERSION);
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        for (host, params) in [
            (H::NumberParseInt, &[HostScalar::I64, HostScalar::I64][..]),
            (H::NumberParseFloat, &[HostScalar::I64][..]),
        ] {
            assert_eq!(host.scalar_signature(), (params, &[HostScalar::F64][..]));
            assert_eq!(host.minimum_abi(), 26);
            assert_eq!(host.capability(), None);
            assert!(!host.returns_string() && !host.needs_result_globals());
            assert_eq!(H::from_name(host.name()), Some(host));
        }
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
    }
    #[test]
    fn number_format_profile_versions_all_three_owned_text_operations() {
        use crate::{HostOperation as H, HostScalar, number_format::NumberFormat};
        let old = LanguageProfile::CheckedF64Utf16V21;
        let new = LanguageProfile::CheckedF64Utf16V22;
        assert!(new.number_format() && new.number_radix() && new.primitive_number());
        assert_eq!(new.host_abi(), crate::abi::NUMBER_FORMAT_VERSION);
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        for &mode in NumberFormat::ALL {
            let operation = mode.host();
            assert_eq!(NumberFormat::from_name(mode.name()), Some(mode));
            assert_eq!(H::from_name(operation.name()), Some(operation));
            assert_eq!(operation.minimum_abi(), new.host_abi());
            assert_eq!(
                operation.scalar_signature(),
                (
                    &[HostScalar::F64, HostScalar::F64][..],
                    &[HostScalar::I32][..]
                )
            );
            assert_eq!(operation.capability(), None);
            assert!(operation.returns_string() && operation.needs_result_globals());
        }
        for operation in [
            H::NumberToString,
            H::PrimitiveNumber,
            H::PrimitiveString,
            H::LogFloat,
            H::UnionLog,
        ] {
            assert_eq!(new.host_operation(operation), old.host_operation(operation));
        }
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn number_radix_profile_versions_owned_text_without_changing_other_selectors() {
        use crate::{HostOperation as H, HostScalar};
        let old = LanguageProfile::CheckedF64Utf16V20;
        let new = LanguageProfile::CheckedF64Utf16V21;
        assert!(new.number_radix() && new.primitive_number() && new.primitive_string());
        assert_eq!(new.host_abi(), crate::abi::NUMBER_RADIX_VERSION);
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        let operation = H::NumberToString;
        assert_eq!(
            operation.scalar_signature(),
            (
                &[HostScalar::F64, HostScalar::F64][..],
                &[HostScalar::I32][..]
            )
        );
        assert_eq!(operation.minimum_abi(), new.host_abi());
        assert_eq!(H::from_name(operation.name()), Some(operation));
        assert_eq!(operation.capability(), None);
        assert!(operation.returns_string() && operation.needs_result_globals());
        for operation in [
            H::PrimitiveNumber,
            H::PrimitiveString,
            H::LogFloat,
            H::UnionLog,
            H::MathRandom,
        ] {
            assert_eq!(new.host_operation(operation), old.host_operation(operation));
        }
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn primitive_number_profile_selects_a_pure_cell_to_f64_operation() {
        use crate::{HostOperation as H, HostScalar};
        let old = LanguageProfile::CheckedF64Utf16V19;
        let new = LanguageProfile::CheckedF64Utf16V20;
        assert!(new.primitive_number() && new.primitive_string() && new.number_text());
        assert_eq!(new.host_abi(), crate::abi::PRIMITIVE_NUMBER_VERSION);
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        let operation = H::PrimitiveNumber;
        assert_eq!(
            operation.scalar_signature(),
            (&[HostScalar::I64][..], &[HostScalar::F64][..])
        );
        assert_eq!(operation.minimum_abi(), new.host_abi());
        assert_eq!(H::from_name(operation.name()), Some(operation));
        assert_eq!(operation.capability(), None);
        assert!(!operation.returns_string() && !operation.needs_result_globals());
        for operation in [
            H::PrimitiveString,
            H::LogFloat,
            H::UnionLog,
            H::MathRandom,
            H::MathSin,
            H::ValueTruthy,
        ] {
            assert_eq!(new.host_operation(operation), old.host_operation(operation));
        }
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn primitive_string_profile_versions_owned_string_results_and_preserves_other_selectors() {
        use crate::{HostOperation as H, HostScalar};
        let old = LanguageProfile::CheckedF64Utf16V18;
        let new = LanguageProfile::CheckedF64Utf16V19;
        assert!(new.primitive_string() && new.number_text() && new.random() && new.native_math());
        assert_eq!(new.host_abi(), crate::abi::PRIMITIVE_STRING_VERSION);
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        let operation = H::PrimitiveString;
        assert_eq!(
            operation.scalar_signature(),
            (&[HostScalar::I64][..], &[HostScalar::I32][..])
        );
        assert_eq!(operation.minimum_abi(), new.host_abi());
        assert_eq!(H::from_name(operation.name()), Some(operation));
        assert_eq!(operation.capability(), None);
        assert!(operation.returns_string() && operation.needs_result_globals());
        for operation in [
            H::LogFloat,
            H::UnionLog,
            H::ValueTruthy,
            H::MathRandom,
            H::MathSin,
            H::LogString,
        ] {
            assert_eq!(new.host_operation(operation), old.host_operation(operation));
        }
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn number_text_profile_selects_versioned_logs_without_changing_other_operations() {
        use crate::{HostOperation as H, HostScalar};
        let old = LanguageProfile::CheckedF64Utf16V17;
        let new = LanguageProfile::CheckedF64Utf16V18;
        assert!(new.number_text() && new.random() && new.native_math());
        assert_eq!(new.host_abi(), crate::abi::NUMBER_TEXT_VERSION);
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        for (old_log, new_log, params) in [
            (H::LogFloat, H::LogFloatV2, &[HostScalar::F64][..]),
            (H::UnionLog, H::ValueLogV7, &[HostScalar::I64][..]),
            (H::ValueLogV6, H::ValueLogV7, &[HostScalar::I64][..]),
        ] {
            assert_eq!(new.host_operation(old_log), new_log);
            assert_eq!(new.host_operation(new_log), new_log);
            assert_eq!(new_log.minimum_abi(), new.host_abi());
            assert_eq!(new_log.scalar_signature(), (params, &[][..]));
            assert_eq!(H::from_name(new_log.name()), Some(new_log));
            assert_eq!(new_log.capability(), None);
            assert!(!new_log.needs_result_globals() && !new_log.returns_string());
        }
        for operation in [
            H::ValueTruthy,
            H::ValueStrictEqual,
            H::JsonParseV14,
            H::JsonStringifyV12,
            H::LogString,
            H::LogInt,
            H::MathRandom,
            H::MathSin,
        ] {
            assert_eq!(new.host_operation(operation), old.host_operation(operation));
        }
        assert_eq!(old.host_operation(H::LogFloat), H::LogFloat);
        assert_eq!(old.host_operation(H::UnionLog), H::ValueLogV6);
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn random_profile_versions_entropy_without_changing_values_or_json() {
        use crate::{HostOperation, HostScalar};
        let old = LanguageProfile::CheckedF64Utf16V16;
        let new = LanguageProfile::CheckedF64Utf16V17;
        assert_eq!(new.host_abi(), crate::abi::RANDOM_VERSION);
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        assert!(new.native_math() && new.math_intrinsics() && new.numeric_constants());
        assert!(new.number_predicates() && new.closures() && new.array_mutations());
        assert!(
            SUPPORTED_PROFILES
                .iter()
                .filter(|p| !p.number_text())
                .all(|p| p.random() == (*p == new))
        );
        assert_eq!(HostOperation::MathRandom.minimum_abi(), new.host_abi());
        assert_eq!(
            HostOperation::MathRandom.scalar_signature(),
            (&[][..], &[HostScalar::F64][..])
        );
        assert_eq!(
            HostOperation::from_name("number_random"),
            Some(HostOperation::MathRandom)
        );
        assert_eq!(HostOperation::MathRandom.capability(), None);
        assert!(!HostOperation::MathRandom.needs_result_globals());
        assert!(!HostOperation::MathRandom.returns_string());
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
    }
    #[test]
    fn native_math_profile_inherits_v15_values_and_features_with_a_new_host_abi() {
        let old = LanguageProfile::CheckedF64Utf16V15;
        let new = LanguageProfile::CheckedF64Utf16V16;
        assert_eq!(new.host_abi(), crate::abi::NATIVE_MATH_VERSION);
        assert!(new.native_math());
        assert!(!old.native_math());
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        assert!(
            [
                new.binary64(),
                new.has_undefined(),
                new.utf16(),
                new.unions(),
                new.missing(),
                new.reference_unions(),
                new.checked_json(),
                new.resizable_arrays(),
                new.exceptions(),
                new.errors(),
                new.error_families(),
                new.recoverable_json(),
                new.function_values(),
                new.closures(),
                new.sparse_arrays(),
                new.array_mutations(),
                new.number_power(),
                new.math_intrinsics(),
                new.number_predicates(),
                new.numeric_constants()
            ]
            .into_iter()
            .all(|value| value)
        );
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
        assert_eq!(
            crate::launcher::LauncherInfo::current().host_abi,
            crate::abi::PROMISE_VALUES_VERSION
        );
    }
    #[test]
    fn power_profile_versions_math_without_changing_sparse_values_or_default() {
        let old = LanguageProfile::CheckedF64Utf16V14;
        let new = LanguageProfile::CheckedF64Utf16V15;
        assert_eq!(new.host_abi(), crate::abi::NUMBER_POWER_VERSION);
        assert_eq!(old.host_abi(), crate::abi::ARRAY_MUTATION_VERSION);
        assert!(new.number_power());
        assert!(!old.number_power());
        assert!(new.math_intrinsics());
        assert!(!old.math_intrinsics());
        assert!(new.number_predicates());
        assert!(!old.number_predicates());
        assert!(new.numeric_constants());
        assert!(!old.numeric_constants());
        assert_eq!(new.value_abi(), old.value_abi());
        assert_eq!(new.json_parse(), old.json_parse());
        assert_eq!(new.json_stringify(), old.json_stringify());
        assert!(
            [
                new.has_undefined(),
                new.utf16(),
                new.unions(),
                new.missing(),
                new.reference_unions(),
                new.checked_json(),
                new.resizable_arrays(),
                new.exceptions(),
                new.errors(),
                new.error_families(),
                new.recoverable_json(),
                new.function_values(),
                new.closures(),
                new.sparse_arrays(),
                new.array_mutations()
            ]
            .into_iter()
            .all(|enabled| enabled)
        );
        assert_eq!(
            LanguageContract::current().profile,
            LanguageProfile::CheckedI32Utf8V1
        );
        let contract = LanguageContract {
            schema_version: SCHEMA_VERSION,
            profile: new,
        };
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
    }
    #[test]
    fn power_host_registry_is_pure_typed_and_requires_the_new_abi() {
        use crate::{HostOperation as H, HostScalar as S};
        assert_eq!(H::from_name("number_power"), Some(H::FloatPower));
        assert_eq!(H::FloatPower.name(), "number_power");
        assert_eq!(
            H::FloatPower.scalar_signature(),
            (&[S::F64, S::F64][..], &[S::F64][..])
        );
        assert_eq!(
            H::FloatPower.minimum_abi(),
            crate::abi::NUMBER_POWER_VERSION
        );
        assert_eq!(H::FloatPower.capability(), None);
        assert!(!H::FloatPower.returns_string());
        assert!(!H::FloatPower.needs_result_globals());
    }
    #[test]
    fn language_contract_is_strict_bounded_and_versioned() {
        let contract = LanguageContract::current();
        assert_eq!(
            LanguageContract::parse(&contract.to_bytes().unwrap()).unwrap(),
            contract
        );
        for bytes in [
            br#"{}"#.as_slice(),
            br#"{"schemaVersion":2,"profile":"checked-i32-utf8-v1"}"#,
            br#"{"schemaVersion":1,"profile":"ecmascript-2025-v1"}"#,
            br#"{"schemaVersion":1,"profile":"checked-i32-utf8-v1","fallback":true}"#,
            br#"{"schemaVersion":1,"schemaVersion":1,"profile":"checked-i32-utf8-v1"}"#,
            br#"{"schemaVersion":1,"profile":"checked-i32-utf8-v1","profile":"checked-i32-utf8-v1"}"#,
        ] {
            assert!(LanguageContract::parse(bytes).is_err());
        }
        assert!(LanguageContract::parse(&vec![b' '; MAX_METADATA_BYTES + 1]).is_err());
        let mut unsupported = contract;
        unsupported.schema_version += 1;
        assert!(unsupported.to_bytes().is_err());
    }
}
