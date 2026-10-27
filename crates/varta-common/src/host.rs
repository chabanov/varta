//! Canonical operation names and Wasm signatures shared by compiler and runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HostOperation {
    JsonParseV14,
    JsonParseV13,
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
    LogFloat,
    FloatRemainder,
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
    #[must_use]
    pub fn returns_string(self) -> bool {
        matches!(
            self,
            Self::ValueTypeOfV6
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
        self.returns_string()
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
            Self::ValueLogV6 => "value_log_v6",
            Self::ValueLogV5 => "value_log_v5",
            Self::ValueLogV4 => "value_log_v4",
            Self::ValueLogV3 => "value_log_v3",
            Self::ValueLog => "value_log_v2",
            Self::ValueTypeOfV6 => "value_typeof_v6",
            Self::ValueTypeOfV5 => "value_typeof_v5",
            Self::ValueTypeOfV4 => "value_typeof_v4",
            Self::ValueTypeOfV3 => "value_typeof_v3",
            Self::ValueTypeOf => "value_typeof_v2",
            Self::ValueTruthyV6 => "value_truthy_v6",
            Self::ValueTruthyV5 => "value_truthy_v5",
            Self::ValueTruthyV4 => "value_truthy_v4",
            Self::ValueTruthyV3 => "value_truthy_v3",
            Self::ValueTruthy => "value_truthy_v2",
            Self::ValueNullishV6 => "value_nullish_v6",
            Self::ValueNullishV5 => "value_nullish_v5",
            Self::ValueNullishV4 => "value_nullish_v4",
            Self::ValueNullishV3 => "value_nullish_v3",
            Self::ValueNullish => "value_nullish_v2",
            Self::ValueStrictEqualV6 => "value_strict_equal_v6",
            Self::ValueStrictEqualV5 => "value_strict_equal_v5",
            Self::ValueStrictEqualV4 => "value_strict_equal_v4",
            Self::ValueStrictEqualV3 => "value_strict_equal_v3",
            Self::ValueStrictEqual => "value_strict_equal_v2",
            Self::ValueNullishEqualV6 => "value_nullish_equal_v6",
            Self::ValueNullishEqualV5 => "value_nullish_equal_v5",
            Self::ValueNullishEqualV4 => "value_nullish_equal_v4",
            Self::ValueNullishEqualV3 => "value_nullish_equal_v3",
            Self::ValueNullishEqual => "value_nullish_equal_v2",
            Self::ValueIsErrorClassV2 => "value_is_error_class_v2",
            Self::ValueIsErrorClass => "value_is_error_class",
            Self::ValueIsError => "value_is_error",
            Self::ValueIsArrayV6 => "value_is_array_v6",
            Self::ValueIsArrayV5 => "value_is_array_v5",
            Self::ValueIsArrayV4 => "value_is_array_v4",
            Self::ValueIsArrayV3 => "value_is_array_v3",
            Self::ValueIsArray => "value_is_array_v2",
            Self::JsonStringifyV8 => "json_stringify_v8",
            Self::JsonParseV8 => "json_parse_v8",
            Self::JsonParseV14 => "json_parse_v14",
            Self::JsonParseV13 => "json_parse_v13",
            Self::JsonParseV12 => "json_parse_v12",
            Self::JsonParseV11 => "json_parse_v11",
            Self::JsonParseV10 => "json_parse_v10",
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
            Self::LogFloat => "log_f64",
            Self::FloatRemainder => "number_remainder",
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
    pub fn signature(self) -> (usize, usize) {
        let (params, results) = self.scalar_signature();
        (params.len(), results.len())
    }
    #[must_use]
    pub fn minimum_abi(self) -> u32 {
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
            Self::ValueIsErrorClassV2 | Self::ValueIsErrorClass => (&[I64, I32], &[I32]),
            Self::ValueLogV6
            | Self::ValueLogV5
            | Self::ValueLogV4
            | Self::ValueLogV3
            | Self::ValueLog
            | Self::UnionLog => (&[I64], &[]),
            Self::ValueTypeOfV6
            | Self::ValueTypeOfV5
            | Self::ValueTypeOfV4
            | Self::ValueTypeOfV3
            | Self::ValueTypeOf
            | Self::ValueTruthyV6
            | Self::ValueTruthyV5
            | Self::ValueTruthyV4
            | Self::ValueTruthyV3
            | Self::ValueTruthy
            | Self::ValueNullishV6
            | Self::ValueNullishV5
            | Self::ValueNullishV4
            | Self::ValueNullishV3
            | Self::ValueNullish
            | Self::ValueIsError
            | Self::ValueIsArrayV6
            | Self::ValueIsArrayV5
            | Self::ValueIsArrayV4
            | Self::ValueIsArrayV3
            | Self::ValueIsArray
            | Self::UnionTypeOf
            | Self::UnionTruthy
            | Self::UnionNullish => (&[I64], &[I32]),
            Self::ValueStrictEqualV6
            | Self::ValueStrictEqualV5
            | Self::ValueStrictEqualV4
            | Self::ValueStrictEqualV3
            | Self::ValueStrictEqual
            | Self::ValueNullishEqualV6
            | Self::ValueNullishEqualV5
            | Self::ValueNullishEqualV4
            | Self::ValueNullishEqualV3
            | Self::ValueNullishEqual
            | Self::UnionStrictEqual
            | Self::UnionNullishEqual => (&[I64, I64], &[I32]),
            Self::LogFloat | Self::ExitFloat => (&[F64], &[]),
            Self::FloatRemainder => (&[F64, F64], &[F64]),
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
            "value_log_v4" => Self::ValueLogV4,
            "value_typeof_v4" => Self::ValueTypeOfV4,
            "value_truthy_v4" => Self::ValueTruthyV4,
            "value_nullish_v4" => Self::ValueNullishV4,
            "value_strict_equal_v4" => Self::ValueStrictEqualV4,
            "value_nullish_equal_v4" => Self::ValueNullishEqualV4,
            "value_is_array_v4" => Self::ValueIsArrayV4,
            "value_is_error_class_v2" => Self::ValueIsErrorClassV2,
            "value_is_error_class" => Self::ValueIsErrorClass,
            "value_log_v6" => Self::ValueLogV6,
            "value_log_v5" => Self::ValueLogV5,
            "value_typeof_v6" => Self::ValueTypeOfV6,
            "value_typeof_v5" => Self::ValueTypeOfV5,
            "value_truthy_v6" => Self::ValueTruthyV6,
            "value_truthy_v5" => Self::ValueTruthyV5,
            "value_nullish_v6" => Self::ValueNullishV6,
            "value_nullish_v5" => Self::ValueNullishV5,
            "value_strict_equal_v6" => Self::ValueStrictEqualV6,
            "value_strict_equal_v5" => Self::ValueStrictEqualV5,
            "value_nullish_equal_v6" => Self::ValueNullishEqualV6,
            "value_nullish_equal_v5" => Self::ValueNullishEqualV5,
            "value_is_array_v6" => Self::ValueIsArrayV6,
            "value_is_array_v5" => Self::ValueIsArrayV5,
            "json_stringify_v12" => Self::JsonStringifyV12,
            "json_stringify_v11" => Self::JsonStringifyV11,
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
            "log_f64" => Self::LogFloat,
            "number_remainder" => Self::FloatRemainder,
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
            Self::ValueIsErrorClassV2
            | Self::ValueIsErrorClass
            | Self::ValueLogV6
            | Self::ValueLogV5
            | Self::ValueLogV4
            | Self::ValueLogV3
            | Self::ValueLog
            | Self::ValueTypeOfV6
            | Self::ValueTypeOfV5
            | Self::ValueTypeOfV4
            | Self::ValueTypeOfV3
            | Self::ValueTypeOf
            | Self::ValueTruthyV6
            | Self::ValueTruthyV5
            | Self::ValueTruthyV4
            | Self::ValueTruthyV3
            | Self::ValueTruthy
            | Self::ValueNullishV6
            | Self::ValueNullishV5
            | Self::ValueNullishV4
            | Self::ValueNullishV3
            | Self::ValueNullish
            | Self::ValueStrictEqualV6
            | Self::ValueStrictEqualV5
            | Self::ValueStrictEqualV4
            | Self::ValueStrictEqualV3
            | Self::ValueStrictEqual
            | Self::ValueNullishEqualV6
            | Self::ValueNullishEqualV5
            | Self::ValueNullishEqualV4
            | Self::ValueNullishEqualV3
            | Self::ValueNullishEqual
            | Self::ValueIsError
            | Self::ValueIsArrayV6
            | Self::ValueIsArrayV5
            | Self::ValueIsArrayV4
            | Self::ValueIsArrayV3
            | Self::ValueIsArray
            | Self::JsonStringifyV12
            | Self::JsonStringifyV11
            | Self::JsonStringifyV10
            | Self::JsonStringifyV9
            | Self::JsonStringifyV8
            | Self::JsonParseV8
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
            | Self::LogFloat
            | Self::FloatRemainder
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
