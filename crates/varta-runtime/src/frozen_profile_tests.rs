//! Earlier profiles keep their bytes and diagnostics while v38 changes its own
//! layout and limits: every runtime case file compiled under v37 and v36 (and
//! the legacy default profile) must produce exactly the module bytes or the
//! diagnostic text pinned before the v38 program-limits batch.
use varta_common::language::LanguageProfile;

/// FNV-1a, 64-bit: a stable digest independent of the Rust hasher.
fn fnv(state: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *state ^= u64::from(*byte);
        *state = state.wrapping_mul(0x0100_0000_01b3);
    }
}

/// The case directories that existed before the v38 batch (133 files). A new
/// stream adds its own directory; repin only from a build of the commit that
/// registered v38 (8fc9965), never from code under change.
const CASE_DIRECTORIES: [&str; 20] = [
    "async_generator_cases",
    "async_method_cases",
    "class_generic_cases",
    "collection_cases",
    "function_field_cases",
    "generator_cases",
    "language_core_cases",
    "net_cases",
    "promise_cases",
    "promise_value_cases",
    "regexp_cases",
    "timer_cases",
    "unknown_cases",
    "v34_cross_cases",
    "v35_cases",
    "v36_cases",
    "v36_cross_cases",
    "v37_integration_cases",
    "value15_cases",
    "web_cases",
];

fn case_files() -> Vec<std::path::PathBuf> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files: Vec<_> = CASE_DIRECTORIES
        .iter()
        .map(|dir| root.join(dir))
        .flat_map(|dir| {
            std::fs::read_dir(dir)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "ts"))
                .collect::<Vec<_>>()
        })
        .collect();
    files.sort();
    files
}

fn digest(profile: Option<LanguageProfile>) -> (u64, usize, usize) {
    let mut state = 0xcbf2_9ce4_8422_2325;
    let (mut compiled, mut rejected) = (0, 0);
    for path in case_files() {
        let source = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_str().unwrap();
        fnv(&mut state, name.as_bytes());
        let result = match profile {
            Some(profile) => varta_compiler::compile_typescript_with_profile(&source, profile),
            None => varta_compiler::compile_typescript(&source),
        };
        match result {
            Ok(bytes) => {
                compiled += 1;
                fnv(&mut state, &bytes);
            }
            Err(error) => {
                rejected += 1;
                fnv(&mut state, error.to_string().as_bytes());
            }
        }
    }
    (state, compiled, rejected)
}

/// V38 changes only layouts and limits (static region, table-driven resume
/// dispatch, registry limits): every pre-v38 case that runs under v37 runs
/// under v38 with the same output, exit code or error. Network cases need
/// their loopback servers and are covered by their own tests.
#[test]
fn every_v37_case_runs_identically_under_v38() {
    use varta_common::budget::BudgetKind;
    let grants: Vec<varta_common::CapabilityGrant> =
        [varta_common::scheduler::TIMER_CAPABILITY, "process.exit"]
            .into_iter()
            .map(|id| varta_common::CapabilityGrant {
                id: id.into(),
                value: varta_common::CapabilityValue::Boolean(true),
            })
            .collect();
    let run = |bytes: &[u8]| {
        crate::scheduler::with_virtual_clock(|| {
            crate::VartaRuntime::new()
                .run_wasm_bytes(bytes, &grants)
                .map(|output| (output.lines, output.exit_code))
                // A trap is compared by kind: function indices in the
                // backtrace differ, and a case that exhausts the production
                // fuel budget does so later under v38, whose instantiation
                // is not charged to the guest. V38 (`turn_budgets`) reports
                // an exhausted production budget as the typed
                // `BudgetExhausted` of its turn; it is compared by the budget
                // it names.
                .map_err(|error| {
                    let exhausted =
                        |budget: BudgetKind| format!("{} budget exhausted", budget.label());
                    if let crate::RuntimeError::Budget(error) = &error {
                        return exhausted(error.budget);
                    }
                    let error = error.to_string();
                    let trap = error
                        .rsplit_once("wasm trap: ")
                        .map_or(error.as_str(), |(_, trap)| trap);
                    match trap {
                        "all fuel consumed by WebAssembly" => exhausted(BudgetKind::Fuel),
                        crate::turns::HOST_BUDGET_EXHAUSTED => exhausted(BudgetKind::HostCalls),
                        crate::turns::DEADLINE_EXCEEDED | "interrupt" => {
                            exhausted(BudgetKind::Deadline)
                        }
                        crate::turns::OUTPUT_BUDGET_EXHAUSTED => exhausted(BudgetKind::Output),
                        other => other.to_owned(),
                    }
                })
        })
    };
    let mut compared = 0;
    for path in case_files() {
        if path.parent().is_some_and(|dir| dir.ends_with("net_cases")) {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let Ok(older) = varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V37,
        ) else {
            continue;
        };
        let newer = varta_compiler::compile_typescript_with_profile(
            &source,
            LanguageProfile::CheckedF64Utf16V38,
        )
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let (newer, older) = (run(&newer), run(&older));
        match &older {
            // V38 (`codegen_v2`) may finish where v37 ran out of fuel or
            // host calls; it never runs out where v37 finished.
            Err(error)
                if [BudgetKind::Fuel, BudgetKind::HostCalls]
                    .iter()
                    .any(|budget| *error == format!("{} budget exhausted", budget.label())) =>
            {
                if let Err(newer) = &newer {
                    assert_eq!(newer, error, "{}", path.display());
                }
            }
            _ => assert_eq!(newer, older, "{}", path.display()),
        }
        compared += 1;
    }
    assert!(compared > 100, "{compared}");
}

#[test]
fn v37_v36_and_legacy_modules_and_diagnostics_stay_byte_identical() {
    let expected = [
        (
            Some(LanguageProfile::CheckedF64Utf16V37),
            (8_526_664_588_381_694_690, 133, 0),
        ),
        (
            Some(LanguageProfile::CheckedF64Utf16V36),
            (13_809_780_362_723_175_373, 84, 49),
        ),
        (None, (18_355_709_462_230_491_895, 0, 133)),
    ];
    let actual: Vec<_> = expected
        .iter()
        .map(|(profile, _)| (*profile, digest(*profile)))
        .collect();
    assert_eq!(actual, expected);
}
