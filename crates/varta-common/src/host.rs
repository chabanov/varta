//! Canonical operation names and Wasm signatures shared by compiler and runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HostOperation {
    /// Value16 (profile v35) readers: each mirrors its value15 counterpart's
    /// signature and contract, reading value16 layouts and nothing else.
    ValueLogV10,
    ValueTypeOfV9,
    ValueTruthyV9,
    ValueNullishV9,
    ValueStrictEqualV9,
    ValueNullishEqualV9,
    ValueIsArrayV9,
    ValueIsErrorClassV5,
    JsonParseV17,
    JsonStringifyV15,
    PrimitiveStringV4,
    PrimitiveNumberV4,
    BigIntFromValueV3,

    /// Value15 (profile v34) readers: each mirrors its value13 counterpart's
    /// signature and contract, reading value15 layouts and nothing else.
    ValueLogV9,
    ValueTypeOfV8,
    ValueTruthyV8,
    ValueNullishV8,
    ValueStrictEqualV8,
    ValueNullishEqualV8,
    ValueIsArrayV8,
    ValueIsErrorClassV4,
    JsonParseV16,
    JsonStringifyV14,
    PrimitiveStringV3,
    PrimitiveNumberV3,
    BigIntFromValueV2,

    /// v34 microtask scheduler (`crate::scheduler`): register a suspended
    /// frame as a reaction of a pending promise.
    PromiseReact,
    /// v34: move a settled promise's reactions into the microtask queue.
    PromiseSettle,
    /// v34: queue a frame resumption as a microtask.
    SchedulerEnqueue,
    /// v34: report a promise rejected without a handler; returns its tracking id.
    PromiseUnhandled,
    /// v34: withdraw a tracked rejection once a handler is added.
    PromiseHandled,
    PrimitiveCompare,
    PrimitiveEqual,
    BigIntWidthV2,
    BigIntToStringV2,
    BigIntFromValue,
    ValueLogV8,
    ValueTypeOfV7,
    ValueTruthyV7,
    ValueNullishV7,
    ValueStrictEqualV7,
    ValueNullishEqualV7,
    ValueIsArrayV7,
    ValueIsErrorClassV3,
    JsonParseV15,
    JsonStringifyV13,
    PrimitiveStringV2,
    PrimitiveNumberV2,

    BigIntCompareString,
    BigIntWidth,
    BigIntBinary,
    BigIntUnary,
    BigIntFromNumber,
    BigIntFromString,
    BigIntToString,
    BigIntToNumber,
    BigIntCompare,
    BigIntCompareNumber,

    NumberParseInt,
    NumberParseFloat,
    JsonParseV14,
    JsonParseV13,
    NumberToFixed,
    NumberToExponential,
    NumberToPrecision,
    NumberToString,
    PrimitiveNumber,
    PrimitiveString,
    ValueLogV7,
    ValueLogV6,
    ValueLogV5,
    ValueTypeOfV6,
    ValueTypeOfV5,
    ValueTruthyV6,
    ValueTruthyV5,
    ValueNullishV6,
    ValueNullishV5,
    ValueStrictEqualV6,
    ValueStrictEqualV5,
    ValueNullishEqualV6,
    ValueNullishEqualV5,
    ValueIsArrayV6,
    ValueIsArrayV5,
    ValueIsErrorClassV2,
    ValueIsErrorClass,
    JsonStringifyV12,
    JsonStringifyV11,
    JsonParseV12,

    ValueLogV4,
    ValueTypeOfV4,
    ValueTruthyV4,
    ValueNullishV4,
    ValueStrictEqualV4,
    ValueNullishEqualV4,
    ValueIsArrayV4,
    ValueIsError,
    JsonStringifyV10,
    JsonParseV11,

    ValueLogV3,
    ValueTypeOfV3,
    ValueTruthyV3,
    ValueNullishV3,
    ValueStrictEqualV3,
    ValueNullishEqualV3,
    ValueIsArrayV3,
    JsonStringifyV9,
    JsonParseV10,
    ValueLog,
    ValueTypeOf,
    ValueTruthy,
    ValueNullish,
    ValueStrictEqual,
    ValueNullishEqual,
    ValueIsArray,
    JsonStringifyV8,
    JsonParseV8,
    JsonParseV9,
    UnionLog,
    UnionTypeOf,
    UnionTruthy,
    UnionNullish,
    UnionStrictEqual,
    UnionNullishEqual,
    JsonStringifyV6,
    JsonStringifyV7,
    JsonParseV7,
    JsonParseV6,
    LogString16,
    EnvHas16,
    EnvGet16,
    EnvPrint16,
    FsRead16,
    FsPrint16,
    NetFetch16,
    NetGet16,
    JsonStringifyV5,
    JsonParseV5,
    StringCharAt16,
    StringCharCodeAt16,
    LogFloatV2,
    LogFloat,
    FloatRemainder,
    FloatPower,
    MathRandom,
    MathAcos,
    MathAcosh,
    MathAsin,
    MathAsinh,
    MathAtan,
    MathAtanh,
    MathAtan2,
    MathCbrt,
    MathCos,
    MathCosh,
    MathExp,
    MathExpm1,
    MathLog,
    MathLog1p,
    MathLog10,
    MathLog2,
    MathSin,
    MathSinh,
    MathTan,
    MathTanh,

    ExitFloat,
    LogInt,
    LogString,
    Time,
    EnvHas,
    EnvGet,
    EnvPrint,
    FsRead,
    FsPrint,
    NetFetch,
    NetGet,
    Exit,
    JsonStringify,
    JsonParse,
    JsonStringifyV3,
    JsonParseV3,
    JsonStringifyV4,
    JsonParseV4,
    LogUndefined,
    HttpStop,
    HttpService,
}
impl HostOperation {
    /// The value15 operation whose signature, capability and observable
    /// contract a value16 (profile v35) operation keeps; `None` otherwise.
    #[must_use]
    pub fn value15_counterpart(self) -> Option<Self> {
        Some(match self {
            Self::ValueLogV10 => Self::ValueLogV9,
            Self::ValueTypeOfV9 => Self::ValueTypeOfV8,
            Self::ValueTruthyV9 => Self::ValueTruthyV8,
            Self::ValueNullishV9 => Self::ValueNullishV8,
            Self::ValueStrictEqualV9 => Self::ValueStrictEqualV8,
            Self::ValueNullishEqualV9 => Self::ValueNullishEqualV8,
            Self::ValueIsArrayV9 => Self::ValueIsArrayV8,
            Self::ValueIsErrorClassV5 => Self::ValueIsErrorClassV4,
            Self::JsonParseV17 => Self::JsonParseV16,
            Self::JsonStringifyV15 => Self::JsonStringifyV14,
            Self::PrimitiveStringV4 => Self::PrimitiveStringV3,
            Self::PrimitiveNumberV4 => Self::PrimitiveNumberV3,
            Self::BigIntFromValueV3 => Self::BigIntFromValueV2,
            _ => return None,
        })
    }
    /// The value13 operation whose signature, capability and observable
    /// contract a value15 (profile v34) operation keeps; `None` otherwise.
    #[must_use]
    pub fn value13_counterpart(self) -> Option<Self> {
        Some(match self {
            Self::ValueLogV9 => Self::ValueLogV8,
            Self::ValueTypeOfV8 => Self::ValueTypeOfV7,
            Self::ValueTruthyV8 => Self::ValueTruthyV7,
            Self::ValueNullishV8 => Self::ValueNullishV7,
            Self::ValueStrictEqualV8 => Self::ValueStrictEqualV7,
            Self::ValueNullishEqualV8 => Self::ValueNullishEqualV7,
            Self::ValueIsArrayV8 => Self::ValueIsArrayV7,
            Self::ValueIsErrorClassV4 => Self::ValueIsErrorClassV3,
            Self::JsonParseV16 => Self::JsonParseV15,
            Self::JsonStringifyV14 => Self::JsonStringifyV13,
            Self::PrimitiveStringV3 => Self::PrimitiveStringV2,
            Self::PrimitiveNumberV3 => Self::PrimitiveNumberV2,
            Self::BigIntFromValueV2 => Self::BigIntFromValue,
            _ => return None,
        })
    }
    #[must_use]
    pub fn returns_string(self) -> bool {
        if let Some(value15) = self.value15_counterpart() {
            return value15.returns_string();
        }
        if let Some(value13) = self.value13_counterpart() {
            return value13.returns_string();
        }
        if matches!(self, Self::PrimitiveStringV2 | Self::ValueTypeOfV7) {
            return true;
        }
        matches!(
            self,
            Self::NumberToFixed
                | Self::NumberToExponential
                | Self::NumberToPrecision
                | Self::NumberToString
                | Self::PrimitiveString
                | Self::ValueTypeOfV6
                | Self::ValueTypeOfV5
                | Self::ValueTypeOfV4
                | Self::ValueTypeOfV3
                | Self::ValueTypeOf
                | Self::JsonStringifyV12
                | Self::JsonStringifyV11
                | Self::JsonStringifyV10
                | Self::JsonStringifyV9
                | Self::JsonStringifyV8
                | Self::UnionTypeOf
                | Self::JsonStringifyV7
                | Self::JsonStringifyV6
                | Self::EnvGet16
                | Self::FsRead16
                | Self::NetGet16
                | Self::JsonStringifyV5
                | Self::StringCharAt16
                | Self::EnvGet
                | Self::FsRead
                | Self::NetGet
                | Self::JsonStringify
                | Self::JsonStringifyV3
                | Self::JsonStringifyV4
        )
    }
    #[must_use]
    pub fn needs_result_globals(self) -> bool {
        if let Some(value15) = self.value15_counterpart() {
            return value15.needs_result_globals();
        }
        if let Some(value13) = self.value13_counterpart() {
            return value13.needs_result_globals();
        }
        if matches!(
            self,
            Self::BigIntWidthV2
                | Self::BigIntToStringV2
                | Self::BigIntFromValue
                | Self::JsonParseV15
                | Self::JsonStringifyV13
                | Self::PrimitiveStringV2
                | Self::ValueTypeOfV7
        ) {
            return true;
        }
        matches!(
            self,
            Self::BigIntBinary
                | Self::BigIntUnary
                | Self::BigIntFromNumber
                | Self::BigIntFromString
                | Self::BigIntToString
                | Self::BigIntWidth
        ) || self.returns_string()
            || matches!(
                self,
                Self::JsonParse
                    | Self::JsonParseV3
                    | Self::JsonParseV4
                    | Self::JsonParseV5
                    | Self::JsonParseV6
                    | Self::JsonParseV7
                    | Self::JsonParseV8
                    | Self::JsonParseV14
                    | Self::JsonParseV13
                    | Self::JsonParseV12
                    | Self::JsonParseV11
                    | Self::JsonParseV10
                    | Self::JsonParseV9
            )
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::ValueLogV10 => "value_log_v10",
            Self::ValueTypeOfV9 => "value_typeof_v9",
            Self::ValueTruthyV9 => "value_truthy_v9",
            Self::ValueNullishV9 => "value_nullish_v9",
            Self::ValueStrictEqualV9 => "value_strict_equal_v9",
            Self::ValueNullishEqualV9 => "value_nullish_equal_v9",
            Self::ValueIsArrayV9 => "value_is_array_v9",
            Self::ValueIsErrorClassV5 => "value_is_error_class_v5",
            Self::JsonParseV17 => "json_parse_v17",
            Self::JsonStringifyV15 => "json_stringify_v15",
            Self::PrimitiveStringV4 => "primitive_to_string_v4",
            Self::PrimitiveNumberV4 => "primitive_to_number_v4",
            Self::BigIntFromValueV3 => "bigint_from_value_v3",
            Self::ValueLogV9 => "value_log_v9",
            Self::ValueTypeOfV8 => "value_typeof_v8",
            Self::ValueTruthyV8 => "value_truthy_v8",
            Self::ValueNullishV8 => "value_nullish_v8",
            Self::ValueStrictEqualV8 => "value_strict_equal_v8",
            Self::ValueNullishEqualV8 => "value_nullish_equal_v8",
            Self::ValueIsArrayV8 => "value_is_array_v8",
            Self::ValueIsErrorClassV4 => "value_is_error_class_v4",
            Self::JsonParseV16 => "json_parse_v16",
            Self::JsonStringifyV14 => "json_stringify_v14",
            Self::PrimitiveStringV3 => "primitive_to_string_v3",
            Self::PrimitiveNumberV3 => "primitive_to_number_v3",
            Self::BigIntFromValueV2 => "bigint_from_value_v2",
            Self::BigIntCompareString => "bigint_compare_string",
            Self::PrimitiveCompare => "primitive_compare",
            Self::PrimitiveEqual => "primitive_equal",
            Self::PromiseReact => "promise_react",
            Self::PromiseSettle => "promise_settle",
            Self::SchedulerEnqueue => "scheduler_enqueue",
            Self::PromiseUnhandled => "promise_unhandled",
            Self::PromiseHandled => "promise_handled",
            Self::BigIntWidthV2 => "bigint_width_v2",
            Self::BigIntToStringV2 => "bigint_to_string_v2",
            Self::BigIntWidth => "bigint_width",
            Self::BigIntBinary => "bigint_binary",
            Self::BigIntUnary => "bigint_unary",
            Self::BigIntFromNumber => "bigint_from_number",
            Self::BigIntFromString => "bigint_from_string",
            Self::BigIntToString => "bigint_to_string",
            Self::BigIntToNumber => "bigint_to_number",
            Self::BigIntCompare => "bigint_compare",
            Self::BigIntCompareNumber => "bigint_compare_number",
            Self::NumberParseInt => "number_parse_int",
            Self::NumberParseFloat => "number_parse_float",
            Self::NumberToFixed => "number_to_fixed",
            Self::NumberToExponential => "number_to_exponential",
            Self::NumberToPrecision => "number_to_precision",
            Self::NumberToString => "number_to_string",
            Self::PrimitiveNumberV2 => "primitive_to_number_v2",
            Self::PrimitiveNumber => "primitive_to_number",
            Self::PrimitiveStringV2 => "primitive_to_string_v2",
            Self::PrimitiveString => "primitive_to_string",
            Self::BigIntFromValue => "bigint_from_value",
            Self::ValueLogV8 => "value_log_v8",
            Self::ValueLogV7 => "value_log_v7",
            Self::ValueLogV6 => "value_log_v6",
            Self::ValueLogV5 => "value_log_v5",
            Self::ValueLogV4 => "value_log_v4",
            Self::ValueLogV3 => "value_log_v3",
            Self::ValueLog => "value_log_v2",
            Self::ValueTypeOfV7 => "value_typeof_v7",
            Self::ValueTypeOfV6 => "value_typeof_v6",
            Self::ValueTypeOfV5 => "value_typeof_v5",
            Self::ValueTypeOfV4 => "value_typeof_v4",
            Self::ValueTypeOfV3 => "value_typeof_v3",
            Self::ValueTypeOf => "value_typeof_v2",
            Self::ValueTruthyV7 => "value_truthy_v7",
            Self::ValueTruthyV6 => "value_truthy_v6",
            Self::ValueTruthyV5 => "value_truthy_v5",
            Self::ValueTruthyV4 => "value_truthy_v4",
            Self::ValueTruthyV3 => "value_truthy_v3",
            Self::ValueTruthy => "value_truthy_v2",
            Self::ValueNullishV7 => "value_nullish_v7",
            Self::ValueNullishV6 => "value_nullish_v6",
            Self::ValueNullishV5 => "value_nullish_v5",
            Self::ValueNullishV4 => "value_nullish_v4",
            Self::ValueNullishV3 => "value_nullish_v3",
            Self::ValueNullish => "value_nullish_v2",
            Self::ValueStrictEqualV7 => "value_strict_equal_v7",
            Self::ValueStrictEqualV6 => "value_strict_equal_v6",
            Self::ValueStrictEqualV5 => "value_strict_equal_v5",
            Self::ValueStrictEqualV4 => "value_strict_equal_v4",
            Self::ValueStrictEqualV3 => "value_strict_equal_v3",
            Self::ValueStrictEqual => "value_strict_equal_v2",
            Self::ValueNullishEqualV7 => "value_nullish_equal_v7",
            Self::ValueNullishEqualV6 => "value_nullish_equal_v6",
            Self::ValueNullishEqualV5 => "value_nullish_equal_v5",
            Self::ValueNullishEqualV4 => "value_nullish_equal_v4",
            Self::ValueNullishEqualV3 => "value_nullish_equal_v3",
            Self::ValueNullishEqual => "value_nullish_equal_v2",
            Self::ValueIsErrorClassV3 => "value_is_error_class_v3",
            Self::ValueIsErrorClassV2 => "value_is_error_class_v2",
            Self::ValueIsErrorClass => "value_is_error_class",
            Self::ValueIsError => "value_is_error",
            Self::ValueIsArrayV7 => "value_is_array_v7",
            Self::ValueIsArrayV6 => "value_is_array_v6",
            Self::ValueIsArrayV5 => "value_is_array_v5",
            Self::ValueIsArrayV4 => "value_is_array_v4",
            Self::ValueIsArrayV3 => "value_is_array_v3",
            Self::ValueIsArray => "value_is_array_v2",
            Self::JsonStringifyV8 => "json_stringify_v8",
            Self::JsonParseV8 => "json_parse_v8",
            Self::JsonParseV15 => "json_parse_v15",
            Self::JsonParseV14 => "json_parse_v14",
            Self::JsonParseV13 => "json_parse_v13",
            Self::JsonParseV12 => "json_parse_v12",
            Self::JsonParseV11 => "json_parse_v11",
            Self::JsonParseV10 => "json_parse_v10",
            Self::JsonStringifyV13 => "json_stringify_v13",
            Self::JsonStringifyV12 => "json_stringify_v12",
            Self::JsonStringifyV11 => "json_stringify_v11",
            Self::JsonStringifyV10 => "json_stringify_v10",
            Self::JsonStringifyV9 => "json_stringify_v9",
            Self::JsonParseV9 => "json_parse_v9",
            Self::UnionLog => "union_log",
            Self::UnionTypeOf => "union_typeof",
            Self::UnionTruthy => "union_truthy",
            Self::UnionNullish => "union_nullish",
            Self::UnionStrictEqual => "union_strict_equal",
            Self::UnionNullishEqual => "union_nullish_equal",
            Self::JsonStringifyV6 => "json_stringify_v6",
            Self::JsonStringifyV7 => "json_stringify_v7",
            Self::JsonParseV7 => "json_parse_v7",
            Self::JsonParseV6 => "json_parse_v6",
            Self::LogString16 => "log_string16",
            Self::EnvHas16 => "env_has16",
            Self::EnvGet16 => "env_get16",
            Self::EnvPrint16 => "env_print16",
            Self::FsRead16 => "fs_read16",
            Self::FsPrint16 => "fs_print16",
            Self::NetFetch16 => "net_fetch16",
            Self::NetGet16 => "net_get16",
            Self::JsonStringifyV5 => "json_stringify_v5",
            Self::JsonParseV5 => "json_parse_v5",
            Self::StringCharAt16 => "string_char_at16",
            Self::StringCharCodeAt16 => "string_char_code_at16",
            Self::LogFloatV2 => "log_f64_v2",
            Self::LogFloat => "log_f64",
            Self::FloatRemainder => "number_remainder",
            Self::FloatPower => "number_power",
            Self::MathRandom => "number_random",
            Self::MathAcos => "number_acos",
            Self::MathAcosh => "number_acosh",
            Self::MathAsin => "number_asin",
            Self::MathAsinh => "number_asinh",
            Self::MathAtan => "number_atan",
            Self::MathAtanh => "number_atanh",
            Self::MathAtan2 => "number_atan2",
            Self::MathCbrt => "number_cbrt",
            Self::MathCos => "number_cos",
            Self::MathCosh => "number_cosh",
            Self::MathExp => "number_exp",
            Self::MathExpm1 => "number_expm1",
            Self::MathLog => "number_log",
            Self::MathLog1p => "number_log1p",
            Self::MathLog10 => "number_log10",
            Self::MathLog2 => "number_log2",
            Self::MathSin => "number_sin",
            Self::MathSinh => "number_sinh",
            Self::MathTan => "number_tan",
            Self::MathTanh => "number_tanh",

            Self::ExitFloat => "process_exit_f64",
            Self::LogInt => "log_i32",
            Self::LogString => "log_string",
            Self::Time => "time_now",
            Self::EnvHas => "env_has",
            Self::EnvGet => "env_get",
            Self::EnvPrint => "env_print",
            Self::FsRead => "fs_read",
            Self::FsPrint => "fs_print",
            Self::NetFetch => "net_fetch",
            Self::NetGet => "net_get",
            Self::Exit => "process_exit",
            Self::JsonStringify => "json_stringify",
            Self::JsonParse => "json_parse",
            Self::JsonStringifyV3 => "json_stringify_v3",
            Self::JsonParseV3 => "json_parse_v3",
            Self::JsonStringifyV4 => "json_stringify_v4",
            Self::JsonParseV4 => "json_parse_v4",
            Self::LogUndefined => "log_undefined",
            Self::HttpStop => "http_stop",
            Self::HttpService => "http_service",
        }
    }
    /// The v34 microtask scheduler operations. They grant no capability and
    /// are charged to the scheduler's own limits, not to the host-call budget.
    #[must_use]
    pub fn scheduler(self) -> bool {
        matches!(
            self,
            Self::PromiseReact
                | Self::PromiseSettle
                | Self::SchedulerEnqueue
                | Self::PromiseUnhandled
                | Self::PromiseHandled
        )
    }
    pub fn signature(self) -> (usize, usize) {
        let (params, results) = self.scalar_signature();
        (params.len(), results.len())
    }
    #[must_use]
    pub fn minimum_abi(self) -> u32 {
        if self.value15_counterpart().is_some() {
            return crate::abi::PROMISE_VALUES_VERSION;
        }
        if self.value13_counterpart().is_some() || self.scheduler() {
            return crate::abi::SCHEDULED_FRAMES_VERSION;
        }
        if matches!(
            self,
            Self::PrimitiveCompare
                | Self::PrimitiveEqual
                | Self::BigIntWidthV2
                | Self::BigIntToStringV2
                | Self::BigIntFromValue
                | Self::ValueLogV8
                | Self::ValueTypeOfV7
                | Self::ValueTruthyV7
                | Self::ValueNullishV7
                | Self::ValueStrictEqualV7
                | Self::ValueNullishEqualV7
                | Self::ValueIsArrayV7
                | Self::ValueIsErrorClassV3
                | Self::JsonParseV15
                | Self::JsonStringifyV13
                | Self::PrimitiveStringV2
                | Self::PrimitiveNumberV2
        ) {
            return crate::abi::BIGINT_VALUES_VERSION;
        }
        if matches!(
            self,
            Self::BigIntCompareString
                | Self::BigIntWidth
                | Self::BigIntBinary
                | Self::BigIntUnary
                | Self::BigIntFromNumber
                | Self::BigIntFromString
                | Self::BigIntToString
                | Self::BigIntToNumber
                | Self::BigIntCompare
                | Self::BigIntCompareNumber
        ) {
            return crate::abi::BIGINT_VERSION;
        }
        if matches!(self, Self::NumberParseInt | Self::NumberParseFloat) {
            return crate::abi::NUMBER_PARSE_VERSION;
        }
        if matches!(
            self,
            Self::NumberToFixed | Self::NumberToExponential | Self::NumberToPrecision
        ) {
            return crate::abi::NUMBER_FORMAT_VERSION;
        }
        if self == Self::NumberToString {
            return crate::abi::NUMBER_RADIX_VERSION;
        }
        if self == Self::PrimitiveNumber {
            return crate::abi::PRIMITIVE_NUMBER_VERSION;
        }
        if self == Self::PrimitiveString {
            return crate::abi::PRIMITIVE_STRING_VERSION;
        }
        if matches!(self, Self::LogFloatV2 | Self::ValueLogV7) {
            return crate::abi::NUMBER_TEXT_VERSION;
        }
        if self == Self::MathRandom {
            return crate::abi::RANDOM_VERSION;
        }
        if crate::native_math::NativeMath::from_host(self).is_some() {
            return crate::abi::NATIVE_MATH_VERSION;
        }
        if self == Self::FloatPower {
            return crate::abi::NUMBER_POWER_VERSION;
        }
        if matches!(
            self,
            Self::ValueLogV6
                | Self::ValueTypeOfV6
                | Self::ValueTruthyV6
                | Self::ValueNullishV6
                | Self::ValueStrictEqualV6
                | Self::ValueNullishEqualV6
                | Self::ValueIsArrayV6
                | Self::ValueIsErrorClassV2
                | Self::JsonStringifyV12
                | Self::JsonParseV14
        ) {
            return crate::abi::SPARSE_VERSION;
        }
        if self == Self::JsonParseV13 {
            return crate::abi::RECOVERABLE_JSON_VERSION;
        }
        if matches!(
            self,
            Self::ValueLogV6
                | Self::ValueLogV5
                | Self::ValueTypeOfV6
                | Self::ValueTypeOfV5
                | Self::ValueTruthyV6
                | Self::ValueTruthyV5
                | Self::ValueNullishV6
                | Self::ValueNullishV5
                | Self::ValueStrictEqualV6
                | Self::ValueStrictEqualV5
                | Self::ValueNullishEqualV6
                | Self::ValueNullishEqualV5
                | Self::ValueIsArrayV6
                | Self::ValueIsArrayV5
                | Self::ValueIsErrorClassV2
                | Self::ValueIsErrorClass
                | Self::JsonStringifyV12
                | Self::JsonStringifyV11
                | Self::JsonParseV14
                | Self::JsonParseV13
                | Self::JsonParseV12
        ) {
            return crate::abi::ERROR_FAMILY_VERSION;
        }
        if matches!(
            self,
            Self::ValueLogV6
                | Self::ValueLogV5
                | Self::ValueLogV4
                | Self::ValueTypeOfV6
                | Self::ValueTypeOfV5
                | Self::ValueTypeOfV4
                | Self::ValueTruthyV6
                | Self::ValueTruthyV5
                | Self::ValueTruthyV4
                | Self::ValueNullishV6
                | Self::ValueNullishV5
                | Self::ValueNullishV4
                | Self::ValueStrictEqualV6
                | Self::ValueStrictEqualV5
                | Self::ValueStrictEqualV4
                | Self::ValueNullishEqualV6
                | Self::ValueNullishEqualV5
                | Self::ValueNullishEqualV4
                | Self::ValueIsArrayV6
                | Self::ValueIsArrayV5
                | Self::ValueIsArrayV4
                | Self::ValueIsError
                | Self::JsonStringifyV12
                | Self::JsonStringifyV11
                | Self::JsonStringifyV10
                | Self::JsonParseV14
                | Self::JsonParseV13
                | Self::JsonParseV12
                | Self::JsonParseV11
        ) {
            return crate::abi::ERROR_VERSION;
        }
        if matches!(
            self,
            Self::ValueLogV6
                | Self::ValueLogV5
                | Self::ValueLogV4
                | Self::ValueLogV3
                | Self::ValueTypeOfV6
                | Self::ValueTypeOfV5
                | Self::ValueTypeOfV4
                | Self::ValueTypeOfV3
                | Self::ValueTruthyV6
                | Self::ValueTruthyV5
                | Self::ValueTruthyV4
                | Self::ValueTruthyV3
                | Self::ValueNullishV6
                | Self::ValueNullishV5
                | Self::ValueNullishV4
                | Self::ValueNullishV3
                | Self::ValueStrictEqualV6
                | Self::ValueStrictEqualV5
                | Self::ValueStrictEqualV4
                | Self::ValueStrictEqualV3
                | Self::ValueNullishEqualV6
                | Self::ValueNullishEqualV5
                | Self::ValueNullishEqualV4
                | Self::ValueNullishEqualV3
                | Self::ValueIsError
                | Self::ValueIsArrayV6
                | Self::ValueIsArrayV5
                | Self::ValueIsArrayV4
                | Self::ValueIsArrayV3
                | Self::JsonStringifyV12
                | Self::JsonStringifyV11
                | Self::JsonStringifyV10
                | Self::JsonStringifyV9
                | Self::JsonParseV14
                | Self::JsonParseV13
                | Self::JsonParseV12
                | Self::JsonParseV11
                | Self::JsonParseV10
        ) {
            return crate::abi::VECTOR_VERSION;
        }
        if self == Self::JsonParseV9 {
            return crate::abi::CHECKED_JSON_VERSION;
        }
        if matches!(
            self,
            Self::ValueLog
                | Self::ValueTypeOf
                | Self::ValueTruthy
                | Self::ValueNullish
                | Self::ValueStrictEqual
                | Self::ValueNullishEqual
                | Self::ValueIsArray
                | Self::JsonStringifyV8
                | Self::JsonParseV8
        ) {
            return crate::abi::REFERENCE_VERSION;
        }
        if matches!(self, Self::JsonStringifyV7 | Self::JsonParseV7) {
            return crate::abi::MISSING_VERSION;
        }
        if matches!(
            self,
            Self::UnionLog
                | Self::UnionTypeOf
                | Self::UnionTruthy
                | Self::UnionNullish
                | Self::UnionStrictEqual
                | Self::UnionNullishEqual
                | Self::JsonStringifyV6
                | Self::JsonParseV6
        ) {
            return crate::abi::UNION_VERSION;
        }
        if matches!(
            self,
            Self::LogString16
                | Self::EnvHas16
                | Self::EnvGet16
                | Self::EnvPrint16
                | Self::FsRead16
                | Self::FsPrint16
                | Self::NetFetch16
                | Self::NetGet16
                | Self::JsonStringifyV5
                | Self::JsonParseV5
                | Self::StringCharAt16
                | Self::StringCharCodeAt16
        ) {
            return crate::abi::UTF16_VERSION;
        }
        if matches!(
            self,
            Self::JsonStringifyV4 | Self::JsonParseV4 | Self::LogUndefined
        ) {
            return crate::abi::UNDEFINED_VERSION;
        }
        if matches!(
            self,
            Self::LogFloat
                | Self::FloatRemainder
                | Self::ExitFloat
                | Self::JsonStringifyV3
                | Self::JsonParseV3
        ) {
            crate::abi::NUMBER_VERSION
        } else {
            crate::abi::VERSION
        }
    }
    /// Physical scalar signatures have one shared owner, including internal intrinsics.
    #[must_use]
    pub fn scalar_signature(self) -> (&'static [HostScalar], &'static [HostScalar]) {
        use HostScalar::{F64, I32, I64};
        match self {
            Self::PrimitiveCompare => (&[I64, I64], &[F64]),
            Self::PromiseReact => (&[F64, I64], &[F64]),
            Self::PromiseSettle => (&[F64], &[]),
            Self::SchedulerEnqueue => (&[I64], &[]),
            Self::PromiseUnhandled => (&[I64], &[F64]),
            Self::PromiseHandled => (&[F64], &[]),
            Self::PrimitiveEqual => (&[I32, I64, I64], &[I32]),
            Self::BigIntWidthV2 => (&[I32, I64, I64], &[I32, I32]),
            Self::BigIntToStringV2 => (&[I64, I64], &[I32, I32]),
            Self::BigIntFromValueV3 | Self::BigIntFromValueV2 | Self::BigIntFromValue => {
                (&[I64], &[I32, I32])
            }
            Self::JsonStringifyV15 | Self::JsonStringifyV14 | Self::JsonStringifyV13 => {
                (&[I32, I32, I32], &[I32, I32])
            }
            Self::ValueIsErrorClassV5
            | Self::ValueIsErrorClassV4
            | Self::ValueIsErrorClassV3
            | Self::ValueIsErrorClassV2
            | Self::ValueIsErrorClass => (&[I64, I32], &[I32]),
            Self::ValueLogV10
            | Self::ValueLogV9
            | Self::ValueLogV8
            | Self::ValueLogV7
            | Self::ValueLogV6
            | Self::ValueLogV5
            | Self::ValueLogV4
            | Self::ValueLogV3
            | Self::ValueLog
            | Self::UnionLog => (&[I64], &[]),
            Self::PrimitiveStringV4
            | Self::PrimitiveStringV3
            | Self::PrimitiveStringV2
            | Self::PrimitiveString
            | Self::ValueTypeOfV9
            | Self::ValueTruthyV9
            | Self::ValueNullishV9
            | Self::ValueIsArrayV9
            | Self::ValueTypeOfV8
            | Self::ValueTypeOfV7
            | Self::ValueTypeOfV6
            | Self::ValueTypeOfV5
            | Self::ValueTypeOfV4
            | Self::ValueTypeOfV3
            | Self::ValueTypeOf
            | Self::ValueTruthyV8
            | Self::ValueTruthyV7
            | Self::ValueTruthyV6
            | Self::ValueTruthyV5
            | Self::ValueTruthyV4
            | Self::ValueTruthyV3
            | Self::ValueTruthy
            | Self::ValueNullishV8
            | Self::ValueNullishV7
            | Self::ValueNullishV6
            | Self::ValueNullishV5
            | Self::ValueNullishV4
            | Self::ValueNullishV3
            | Self::ValueNullish
            | Self::ValueIsError
            | Self::ValueIsArrayV8
            | Self::ValueIsArrayV7
            | Self::ValueIsArrayV6
            | Self::ValueIsArrayV5
            | Self::ValueIsArrayV4
            | Self::ValueIsArrayV3
            | Self::ValueIsArray
            | Self::UnionTypeOf
            | Self::UnionTruthy
            | Self::UnionNullish => (&[I64], &[I32]),
            Self::ValueStrictEqualV9
            | Self::ValueNullishEqualV9
            | Self::ValueStrictEqualV8
            | Self::ValueStrictEqualV7
            | Self::ValueStrictEqualV6
            | Self::ValueStrictEqualV5
            | Self::ValueStrictEqualV4
            | Self::ValueStrictEqualV3
            | Self::ValueStrictEqual
            | Self::ValueNullishEqualV8
            | Self::ValueNullishEqualV7
            | Self::ValueNullishEqualV6
            | Self::ValueNullishEqualV5
            | Self::ValueNullishEqualV4
            | Self::ValueNullishEqualV3
            | Self::ValueNullishEqual
            | Self::UnionStrictEqual
            | Self::UnionNullishEqual => (&[I64, I64], &[I32]),
            Self::LogFloat | Self::LogFloatV2 | Self::ExitFloat => (&[F64], &[]),
            Self::MathRandom => (&[], &[F64]),
            Self::NumberToFixed
            | Self::NumberToExponential
            | Self::NumberToPrecision
            | Self::NumberToString => (&[F64, F64], &[I32]),
            Self::BigIntWidth => (&[I32, I64, F64], &[I32, I32]),
            Self::BigIntBinary => (&[I32, I64, I64], &[I32, I32]),
            Self::BigIntUnary => (&[I32, I64], &[I32, I32]),
            Self::BigIntFromNumber => (&[F64], &[I32, I32]),
            Self::BigIntFromString => (&[I32, I32], &[I32, I32]),
            Self::BigIntToString => (&[I64, F64], &[I32, I32]),
            Self::BigIntToNumber => (&[I64], &[F64]),
            Self::BigIntCompareString => (&[I64, I32, I32], &[F64]),
            Self::BigIntCompare => (&[I64, I64], &[F64]),
            Self::BigIntCompareNumber => (&[I64, F64], &[F64]),
            Self::NumberParseInt => (&[I64, I64], &[F64]),
            Self::NumberParseFloat
            | Self::PrimitiveNumberV4
            | Self::PrimitiveNumberV3
            | Self::PrimitiveNumberV2
            | Self::PrimitiveNumber => (&[I64], &[F64]),
            Self::FloatRemainder | Self::FloatPower | Self::MathAtan2 => (&[F64, F64], &[F64]),
            Self::MathAcos
            | Self::MathAcosh
            | Self::MathAsin
            | Self::MathAsinh
            | Self::MathAtan
            | Self::MathAtanh
            | Self::MathCbrt
            | Self::MathCos
            | Self::MathCosh
            | Self::MathExp
            | Self::MathExpm1
            | Self::MathLog
            | Self::MathLog1p
            | Self::MathLog10
            | Self::MathLog2
            | Self::MathSin
            | Self::MathSinh
            | Self::MathTan
            | Self::MathTanh => (&[F64], &[F64]),
            Self::LogInt | Self::Exit | Self::LogUndefined => (&[I32], &[]),
            Self::LogString16
            | Self::EnvPrint16
            | Self::FsPrint16
            | Self::LogString
            | Self::EnvPrint
            | Self::FsPrint => (&[I32, I32], &[]),
            Self::Time => (&[], &[I32]),
            Self::JsonStringifyV12
            | Self::JsonStringifyV11
            | Self::JsonStringifyV10
            | Self::JsonStringifyV9
            | Self::JsonStringifyV8
            | Self::JsonStringifyV7
            | Self::JsonStringifyV6
            | Self::JsonStringifyV5
            | Self::JsonStringify
            | Self::JsonStringifyV3
            | Self::JsonStringifyV4 => (&[I32, I32, I32], &[I32]),
            Self::JsonParse => (&[I32, I32, I32, I32], &[I32]),
            Self::JsonParseV3
            | Self::JsonParseV4
            | Self::JsonParseV5
            | Self::JsonParseV6
            | Self::JsonParseV7
            | Self::JsonParseV8
            | Self::JsonParseV17
            | Self::JsonParseV16
            | Self::JsonParseV15
            | Self::JsonParseV14
            | Self::JsonParseV13
            | Self::JsonParseV12
            | Self::JsonParseV11
            | Self::JsonParseV10
            | Self::JsonParseV9 => (&[I32, I32, I32, I32], &[I64]),
            Self::HttpStop | Self::HttpService => (&[], &[]),
            Self::StringCharAt16 => (&[I32, I32, F64], &[I32]),
            Self::StringCharCodeAt16 => (&[I32, I32, F64], &[F64]),
            Self::EnvHas16
            | Self::EnvGet16
            | Self::FsRead16
            | Self::NetFetch16
            | Self::NetGet16
            | Self::EnvHas
            | Self::EnvGet
            | Self::FsRead
            | Self::NetFetch
            | Self::NetGet => (&[I32, I32], &[I32]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostScalar {
    I32,
    I64,
    F64,
}

impl HostOperation {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "value_log_v10" => Self::ValueLogV10,
            "value_typeof_v9" => Self::ValueTypeOfV9,
            "value_truthy_v9" => Self::ValueTruthyV9,
            "value_nullish_v9" => Self::ValueNullishV9,
            "value_strict_equal_v9" => Self::ValueStrictEqualV9,
            "value_nullish_equal_v9" => Self::ValueNullishEqualV9,
            "value_is_array_v9" => Self::ValueIsArrayV9,
            "value_is_error_class_v5" => Self::ValueIsErrorClassV5,
            "json_parse_v17" => Self::JsonParseV17,
            "json_stringify_v15" => Self::JsonStringifyV15,
            "primitive_to_string_v4" => Self::PrimitiveStringV4,
            "primitive_to_number_v4" => Self::PrimitiveNumberV4,
            "bigint_from_value_v3" => Self::BigIntFromValueV3,
            "value_log_v9" => Self::ValueLogV9,
            "value_typeof_v8" => Self::ValueTypeOfV8,
            "value_truthy_v8" => Self::ValueTruthyV8,
            "value_nullish_v8" => Self::ValueNullishV8,
            "value_strict_equal_v8" => Self::ValueStrictEqualV8,
            "value_nullish_equal_v8" => Self::ValueNullishEqualV8,
            "value_is_array_v8" => Self::ValueIsArrayV8,
            "value_is_error_class_v4" => Self::ValueIsErrorClassV4,
            "json_parse_v16" => Self::JsonParseV16,
            "json_stringify_v14" => Self::JsonStringifyV14,
            "primitive_to_string_v3" => Self::PrimitiveStringV3,
            "primitive_to_number_v3" => Self::PrimitiveNumberV3,
            "bigint_from_value_v2" => Self::BigIntFromValueV2,
            "primitive_compare" => Self::PrimitiveCompare,
            "promise_react" => Self::PromiseReact,
            "promise_settle" => Self::PromiseSettle,
            "scheduler_enqueue" => Self::SchedulerEnqueue,
            "promise_unhandled" => Self::PromiseUnhandled,
            "promise_handled" => Self::PromiseHandled,
            "primitive_equal" => Self::PrimitiveEqual,
            "bigint_width_v2" => Self::BigIntWidthV2,
            "bigint_to_string_v2" => Self::BigIntToStringV2,
            "bigint_from_value" => Self::BigIntFromValue,
            "value_log_v4" => Self::ValueLogV4,
            "value_typeof_v4" => Self::ValueTypeOfV4,
            "value_truthy_v4" => Self::ValueTruthyV4,
            "value_nullish_v4" => Self::ValueNullishV4,
            "value_strict_equal_v4" => Self::ValueStrictEqualV4,
            "value_nullish_equal_v4" => Self::ValueNullishEqualV4,
            "value_is_array_v4" => Self::ValueIsArrayV4,
            "value_is_error_class_v3" => Self::ValueIsErrorClassV3,
            "value_is_error_class_v2" => Self::ValueIsErrorClassV2,
            "value_is_error_class" => Self::ValueIsErrorClass,
            "bigint_compare_string" => Self::BigIntCompareString,
            "bigint_width" => Self::BigIntWidth,
            "bigint_binary" => Self::BigIntBinary,
            "bigint_unary" => Self::BigIntUnary,
            "bigint_from_number" => Self::BigIntFromNumber,
            "bigint_from_string" => Self::BigIntFromString,
            "bigint_to_string" => Self::BigIntToString,
            "bigint_to_number" => Self::BigIntToNumber,
            "bigint_compare" => Self::BigIntCompare,
            "bigint_compare_number" => Self::BigIntCompareNumber,
            "number_parse_int" => Self::NumberParseInt,
            "number_parse_float" => Self::NumberParseFloat,
            "number_to_fixed" => Self::NumberToFixed,
            "number_to_exponential" => Self::NumberToExponential,
            "number_to_precision" => Self::NumberToPrecision,
            "number_to_string" => Self::NumberToString,
            "primitive_to_number_v2" => Self::PrimitiveNumberV2,
            "primitive_to_number" => Self::PrimitiveNumber,
            "primitive_to_string_v2" => Self::PrimitiveStringV2,
            "primitive_to_string" => Self::PrimitiveString,
            "value_log_v8" => Self::ValueLogV8,
            "value_log_v7" => Self::ValueLogV7,
            "value_log_v6" => Self::ValueLogV6,
            "value_log_v5" => Self::ValueLogV5,
            "value_typeof_v7" => Self::ValueTypeOfV7,
            "value_typeof_v6" => Self::ValueTypeOfV6,
            "value_typeof_v5" => Self::ValueTypeOfV5,
            "value_truthy_v7" => Self::ValueTruthyV7,
            "value_truthy_v6" => Self::ValueTruthyV6,
            "value_truthy_v5" => Self::ValueTruthyV5,
            "value_nullish_v7" => Self::ValueNullishV7,
            "value_nullish_v6" => Self::ValueNullishV6,
            "value_nullish_v5" => Self::ValueNullishV5,
            "value_strict_equal_v7" => Self::ValueStrictEqualV7,
            "value_strict_equal_v6" => Self::ValueStrictEqualV6,
            "value_strict_equal_v5" => Self::ValueStrictEqualV5,
            "value_nullish_equal_v7" => Self::ValueNullishEqualV7,
            "value_nullish_equal_v6" => Self::ValueNullishEqualV6,
            "value_nullish_equal_v5" => Self::ValueNullishEqualV5,
            "value_is_array_v7" => Self::ValueIsArrayV7,
            "value_is_array_v6" => Self::ValueIsArrayV6,
            "value_is_array_v5" => Self::ValueIsArrayV5,
            "json_stringify_v13" => Self::JsonStringifyV13,
            "json_stringify_v12" => Self::JsonStringifyV12,
            "json_stringify_v11" => Self::JsonStringifyV11,
            "json_parse_v15" => Self::JsonParseV15,
            "json_parse_v14" => Self::JsonParseV14,
            "json_parse_v13" => Self::JsonParseV13,
            "json_parse_v12" => Self::JsonParseV12,
            "value_is_error" => Self::ValueIsError,
            "json_stringify_v10" => Self::JsonStringifyV10,
            "json_parse_v11" => Self::JsonParseV11,
            "value_log_v3" => Self::ValueLogV3,
            "value_log_v2" => Self::ValueLog,
            "value_typeof_v3" => Self::ValueTypeOfV3,
            "value_typeof_v2" => Self::ValueTypeOf,
            "value_truthy_v3" => Self::ValueTruthyV3,
            "value_truthy_v2" => Self::ValueTruthy,
            "value_nullish_v3" => Self::ValueNullishV3,
            "value_nullish_v2" => Self::ValueNullish,
            "value_strict_equal_v3" => Self::ValueStrictEqualV3,
            "value_strict_equal_v2" => Self::ValueStrictEqual,
            "value_nullish_equal_v3" => Self::ValueNullishEqualV3,
            "value_nullish_equal_v2" => Self::ValueNullishEqual,
            "value_is_array_v3" => Self::ValueIsArrayV3,
            "value_is_array_v2" => Self::ValueIsArray,
            "json_stringify_v8" => Self::JsonStringifyV8,
            "json_parse_v8" => Self::JsonParseV8,
            "json_parse_v10" => Self::JsonParseV10,
            "json_stringify_v9" => Self::JsonStringifyV9,
            "json_parse_v9" => Self::JsonParseV9,
            "union_log" => Self::UnionLog,
            "union_typeof" => Self::UnionTypeOf,
            "union_truthy" => Self::UnionTruthy,
            "union_nullish" => Self::UnionNullish,
            "union_strict_equal" => Self::UnionStrictEqual,
            "union_nullish_equal" => Self::UnionNullishEqual,
            "json_stringify_v6" => Self::JsonStringifyV6,
            "json_stringify_v7" => Self::JsonStringifyV7,
            "json_parse_v7" => Self::JsonParseV7,
            "json_parse_v6" => Self::JsonParseV6,
            "log_string16" => Self::LogString16,
            "env_has16" => Self::EnvHas16,
            "env_get16" => Self::EnvGet16,
            "env_print16" => Self::EnvPrint16,
            "fs_read16" => Self::FsRead16,
            "fs_print16" => Self::FsPrint16,
            "net_fetch16" => Self::NetFetch16,
            "net_get16" => Self::NetGet16,
            "json_stringify_v5" => Self::JsonStringifyV5,
            "json_parse_v5" => Self::JsonParseV5,
            "string_char_at16" => Self::StringCharAt16,
            "string_char_code_at16" => Self::StringCharCodeAt16,
            "log_f64_v2" => Self::LogFloatV2,
            "log_f64" => Self::LogFloat,
            "number_remainder" => Self::FloatRemainder,
            "number_power" => Self::FloatPower,
            "number_random" => Self::MathRandom,
            "number_acos" => Self::MathAcos,
            "number_acosh" => Self::MathAcosh,
            "number_asin" => Self::MathAsin,
            "number_asinh" => Self::MathAsinh,
            "number_atan" => Self::MathAtan,
            "number_atanh" => Self::MathAtanh,
            "number_atan2" => Self::MathAtan2,
            "number_cbrt" => Self::MathCbrt,
            "number_cos" => Self::MathCos,
            "number_cosh" => Self::MathCosh,
            "number_exp" => Self::MathExp,
            "number_expm1" => Self::MathExpm1,
            "number_log" => Self::MathLog,
            "number_log1p" => Self::MathLog1p,
            "number_log10" => Self::MathLog10,
            "number_log2" => Self::MathLog2,
            "number_sin" => Self::MathSin,
            "number_sinh" => Self::MathSinh,
            "number_tan" => Self::MathTan,
            "number_tanh" => Self::MathTanh,

            "process_exit_f64" => Self::ExitFloat,
            "log_i32" => Self::LogInt,
            "log_string" => Self::LogString,
            "time_now" => Self::Time,
            "env_has" => Self::EnvHas,
            "env_get" => Self::EnvGet,
            "env_print" => Self::EnvPrint,
            "fs_read" => Self::FsRead,
            "fs_print" => Self::FsPrint,
            "net_fetch" => Self::NetFetch,
            "net_get" => Self::NetGet,
            "process_exit" => Self::Exit,
            "json_stringify" => Self::JsonStringify,
            "json_parse" => Self::JsonParse,
            "json_parse_v3" => Self::JsonParseV3,
            "json_stringify_v3" => Self::JsonStringifyV3,
            "json_parse_v4" => Self::JsonParseV4,
            "json_stringify_v4" => Self::JsonStringifyV4,
            "log_undefined" => Self::LogUndefined,
            "http_stop" => Self::HttpStop,
            "http_service" => Self::HttpService,
            _ => return None,
        })
    }
    #[must_use]
    pub fn capability(self) -> Option<&'static str> {
        Some(match self {
            Self::ValueLogV10
            | Self::ValueTypeOfV9
            | Self::ValueTruthyV9
            | Self::ValueNullishV9
            | Self::ValueStrictEqualV9
            | Self::ValueNullishEqualV9
            | Self::ValueIsArrayV9
            | Self::ValueIsErrorClassV5
            | Self::JsonParseV17
            | Self::JsonStringifyV15
            | Self::PrimitiveStringV4
            | Self::PrimitiveNumberV4
            | Self::BigIntFromValueV3
            | Self::ValueLogV9
            | Self::ValueTypeOfV8
            | Self::ValueStrictEqualV8
            | Self::ValueIsErrorClassV4
            | Self::JsonStringifyV14
            | Self::PrimitiveStringV3
            | Self::PrimitiveNumberV3
            | Self::BigIntFromValueV2
            | Self::PromiseReact
            | Self::PromiseSettle
            | Self::SchedulerEnqueue
            | Self::PromiseUnhandled
            | Self::PromiseHandled
            | Self::PrimitiveCompare
            | Self::PrimitiveEqual
            | Self::BigIntWidthV2
            | Self::BigIntToStringV2
            | Self::BigIntFromValue
            | Self::BigIntCompareString
            | Self::BigIntWidth
            | Self::BigIntBinary
            | Self::BigIntUnary
            | Self::BigIntFromNumber
            | Self::BigIntFromString
            | Self::BigIntToString
            | Self::BigIntToNumber
            | Self::BigIntCompare
            | Self::BigIntCompareNumber
            | Self::NumberParseInt
            | Self::NumberParseFloat
            | Self::ValueIsErrorClassV3
            | Self::ValueIsErrorClassV2
            | Self::ValueIsErrorClass
            | Self::NumberToFixed
            | Self::NumberToExponential
            | Self::NumberToPrecision
            | Self::NumberToString
            | Self::PrimitiveNumberV2
            | Self::PrimitiveNumber
            | Self::PrimitiveStringV2
            | Self::PrimitiveString
            | Self::ValueLogV8
            | Self::ValueLogV7
            | Self::ValueLogV6
            | Self::ValueLogV5
            | Self::ValueLogV4
            | Self::ValueLogV3
            | Self::ValueLog
            | Self::ValueTypeOfV7
            | Self::ValueTypeOfV6
            | Self::ValueTypeOfV5
            | Self::ValueTypeOfV4
            | Self::ValueTypeOfV3
            | Self::ValueTypeOf
            | Self::ValueTruthyV8
            | Self::ValueTruthyV7
            | Self::ValueTruthyV6
            | Self::ValueTruthyV5
            | Self::ValueTruthyV4
            | Self::ValueTruthyV3
            | Self::ValueTruthy
            | Self::ValueNullishV8
            | Self::ValueNullishV7
            | Self::ValueNullishV6
            | Self::ValueNullishV5
            | Self::ValueNullishV4
            | Self::ValueNullishV3
            | Self::ValueNullish
            | Self::ValueStrictEqualV7
            | Self::ValueStrictEqualV6
            | Self::ValueStrictEqualV5
            | Self::ValueStrictEqualV4
            | Self::ValueStrictEqualV3
            | Self::ValueStrictEqual
            | Self::ValueNullishEqualV8
            | Self::ValueNullishEqualV7
            | Self::ValueNullishEqualV6
            | Self::ValueNullishEqualV5
            | Self::ValueNullishEqualV4
            | Self::ValueNullishEqualV3
            | Self::ValueNullishEqual
            | Self::ValueIsError
            | Self::ValueIsArrayV8
            | Self::ValueIsArrayV7
            | Self::ValueIsArrayV6
            | Self::ValueIsArrayV5
            | Self::ValueIsArrayV4
            | Self::ValueIsArrayV3
            | Self::ValueIsArray
            | Self::JsonStringifyV13
            | Self::JsonStringifyV12
            | Self::JsonStringifyV11
            | Self::JsonStringifyV10
            | Self::JsonStringifyV9
            | Self::JsonStringifyV8
            | Self::JsonParseV8
            | Self::JsonParseV16
            | Self::JsonParseV15
            | Self::JsonParseV14
            | Self::JsonParseV13
            | Self::JsonParseV12
            | Self::JsonParseV11
            | Self::JsonParseV10
            | Self::JsonParseV9
            | Self::UnionLog
            | Self::UnionTypeOf
            | Self::UnionTruthy
            | Self::UnionNullish
            | Self::UnionStrictEqual
            | Self::UnionNullishEqual
            | Self::JsonStringifyV7
            | Self::JsonParseV7
            | Self::JsonStringifyV6
            | Self::JsonParseV6
            | Self::LogString16
            | Self::JsonStringifyV5
            | Self::JsonParseV5
            | Self::StringCharAt16
            | Self::StringCharCodeAt16
            | Self::LogInt
            | Self::LogFloatV2
            | Self::LogFloat
            | Self::FloatRemainder
            | Self::FloatPower
            | Self::MathRandom
            | Self::MathAcos
            | Self::MathAcosh
            | Self::MathAsin
            | Self::MathAsinh
            | Self::MathAtan
            | Self::MathAtanh
            | Self::MathAtan2
            | Self::MathCbrt
            | Self::MathCos
            | Self::MathCosh
            | Self::MathExp
            | Self::MathExpm1
            | Self::MathLog
            | Self::MathLog1p
            | Self::MathLog10
            | Self::MathLog2
            | Self::MathSin
            | Self::MathSinh
            | Self::MathTan
            | Self::MathTanh
            | Self::LogString
            | Self::JsonStringify
            | Self::JsonParse
            | Self::JsonParseV3
            | Self::JsonStringifyV3
            | Self::JsonStringifyV4
            | Self::JsonParseV4
            | Self::LogUndefined
            | Self::HttpService => return None,
            Self::Time => "time.now",
            Self::EnvHas | Self::EnvHas16 => "env.has",
            Self::EnvGet | Self::EnvGet16 => "env.get",
            Self::EnvPrint | Self::EnvPrint16 => "env.print",
            Self::FsRead | Self::FsRead16 => "fs.read",
            Self::FsPrint | Self::FsPrint16 => "fs.print",
            Self::NetFetch | Self::NetFetch16 => "net.fetch",
            Self::NetGet | Self::NetGet16 => "net.get",
            Self::Exit | Self::ExitFloat => "process.exit",
            Self::HttpStop => "net.listen",
        })
    }
}
