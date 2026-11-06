//! Fixed-width SIMD admission
//! ([`LanguageProfile::simd128`](varta_common::language::LanguageProfile::simd128)).
//! The engine enables `simd128` and disables relaxed SIMD (whose results
//! differ between hosts) for every module; a module of any other profile,
//! or one without a profile, must contain no `v128` value or SIMD
//! instruction at all, so its hosts need no SIMD.
use super::error;
use crate::RuntimeError;
use wasmparser::{Operator, Parser, Payload, ValType};

/// Reject a module that holds a `v128` type, global, local or instruction
/// unless `admitted`.
pub(super) fn check(bytes: &[u8], admitted: bool) -> Result<(), RuntimeError> {
    if admitted {
        return Ok(());
    }
    let rejected = || error("SIMD (simd128) is not admitted by the module's profile");
    for payload in Parser::new(0).parse_all(bytes) {
        match payload.map_err(|e| error(&e.to_string()))? {
            Payload::TypeSection(reader) => {
                for ty in reader.into_iter_err_on_gc_types() {
                    let ty = ty.map_err(|e| error(&e.to_string()))?;
                    if ty.params().iter().chain(ty.results()).any(is_vector) {
                        return Err(rejected());
                    }
                }
            }
            Payload::GlobalSection(reader) => {
                for global in reader {
                    let global = global.map_err(|e| error(&e.to_string()))?;
                    if is_vector(&global.ty.content_type) {
                        return Err(rejected());
                    }
                }
            }
            Payload::CodeSectionEntry(body) => {
                for local in body
                    .get_locals_reader()
                    .map_err(|e| error(&e.to_string()))?
                {
                    let (_, ty) = local.map_err(|e| error(&e.to_string()))?;
                    if is_vector(&ty) {
                        return Err(rejected());
                    }
                }
                for operator in body
                    .get_operators_reader()
                    .map_err(|e| error(&e.to_string()))?
                {
                    let operator = operator.map_err(|e| error(&e.to_string()))?;
                    if vector_operator(&operator) {
                        return Err(rejected());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn is_vector(ty: &ValType) -> bool {
    matches!(ty, ValType::V128)
}

/// Whether `operator` belongs to the SIMD or the relaxed SIMD proposal.
fn vector_operator(operator: &Operator<'_>) -> bool {
    macro_rules! proposal {
        ($(@$proposal:ident $name:ident $({ $($arg:ident: $argty:ty),* })? => $visit:ident ($($ann:tt)*) )*) => {
            match operator {
                $(Operator::$name $({ $($arg: _),* })? => {
                    matches!(stringify!($proposal), "simd" | "relaxed_simd")
                })*
                _ => false,
            }
        }
    }
    wasmparser::for_each_operator!(proposal)
}

#[cfg(test)]
mod tests {
    use crate::VartaRuntime;
    use varta_common::language::LanguageProfile as P;

    const SIMD_PREFIX: u8 = 0xfd;
    const V128_CONST: u8 = 0x0c;
    const V128_ANY_TRUE: u8 = 0x53;
    /// `i8x16.relaxed_swizzle` (sub-opcode 256, LEB128).
    const RELAXED_SWIZZLE: [u8; 2] = [0x80, 0x02];
    const DROP: u8 = 0x1a;
    const END: u8 = 0x0b;
    const V128: u8 = 0x7b;
    const FUNCTION_TYPE: u8 = 0x60;
    const TYPE_SECTION: u8 = 1;
    const FUNCTION_SECTION: u8 = 3;
    const CODE_SECTION: u8 = 10;

    /// A generic module (no profile) of one `() -> ()` function with
    /// `locals` (count, type) and the body `code` (sizes below 128 bytes).
    fn module(locals: &[(u8, u8)], code: &[u8]) -> Vec<u8> {
        let mut body = vec![locals.len() as u8];
        for &(count, ty) in locals {
            body.extend([count, ty]);
        }
        body.extend(code);
        body.push(END);
        let mut bytes = varta_common::abi::WASM_HEADER.to_vec();
        for (id, content) in [
            (TYPE_SECTION, vec![1, FUNCTION_TYPE, 0, 0]),
            (FUNCTION_SECTION, vec![1, 0]),
            (CODE_SECTION, [vec![1, body.len() as u8], body].concat()),
        ] {
            bytes.extend([id, content.len() as u8]);
            bytes.extend(content);
        }
        bytes
    }
    fn vector() -> Vec<u8> {
        [vec![SIMD_PREFIX, V128_CONST], vec![0; 16]].concat()
    }

    /// Outside a profile that admits it, a module holds no SIMD instruction
    /// and no `v128` local; relaxed SIMD is rejected by the engine for every
    /// module; a scalar module passes.
    #[test]
    fn simd_is_admitted_only_by_its_profile() {
        let runtime = VartaRuntime::new();
        let any_true = [vector(), vec![SIMD_PREFIX, V128_ANY_TRUE, DROP]].concat();
        for bytes in [module(&[], &any_true), module(&[(1, V128)], &[])] {
            let error = runtime.validate_module(&bytes).unwrap_err().to_string();
            assert!(error.contains("SIMD (simd128) is not admitted"), "{error}");
            assert!(super::check(&bytes, true).is_ok());
        }
        let relaxed = [
            vector(),
            vector(),
            vec![SIMD_PREFIX],
            RELAXED_SWIZZLE.to_vec(),
            vec![DROP],
        ]
        .concat();
        let error = runtime
            .validate_module(&module(&[], &relaxed))
            .unwrap_err()
            .to_string();
        assert!(!error.contains("simd128"), "{error}");
        runtime.validate_module(&module(&[], &[])).unwrap();
        assert!(P::CheckedF64Utf16V38.simd128());
        assert!(!P::CheckedF64Utf16V37.simd128());
    }

    /// A v38 program's compiled JSON decoders use SIMD and validate; the
    /// same program under v37 holds none.
    #[test]
    fn v38_json_decoders_hold_simd_and_validate() {
        let source = "interface R { s: string } export function main(): void {
            const r = JSON.parse('{\"s\":\"text\"}') as R; console.log(r.s); }";
        let runtime = VartaRuntime::new();
        for profile in [P::CheckedF64Utf16V38, P::CheckedF64Utf16V37] {
            let bytes = varta_compiler::compile_typescript_with_profile(source, profile).unwrap();
            runtime.validate_module(&bytes).unwrap();
            assert_eq!(
                super::check(&bytes, false).is_err(),
                profile.simd128(),
                "{profile:?}"
            );
        }
    }
}
