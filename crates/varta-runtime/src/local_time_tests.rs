//! V38 local time (`local_time`, K9 D4): `Date` in the host's time zone,
//! with the zone and default locale as recorded host inputs
//! (`intl_time_zone`, `intl_locale`) and offsets, transitions and names
//! computed by the linked `date_zone_offset`/`date_zone_name`.
//!
//! `local_time_cases/local.ts` runs once per zone and locale; each
//! `local.<zone>.<locale>.expected` is the output of Node 24.20.0 running the
//! same program (with `await main();` appended) under `TZ=<zone>` and
//! `LANG=<locale as POSIX>.UTF-8`, recorded once and checked in.
use super::*;
use varta_common::HostOperation as H;
use varta_common::host_inputs::HostInputs;
use varta_common::language::LanguageProfile;

const V38: LanguageProfile = LanguageProfile::CheckedF64Utf16V38;
const V37: LanguageProfile = LanguageProfile::CheckedF64Utf16V37;
const CASE: &str = include_str!("local_time_cases/local.ts");

macro_rules! expected {
    ($zone:literal, $file:literal, $locale:literal) => {
        (
            $zone,
            $locale,
            include_str!(concat!(
                "local_time_cases/local.",
                $file,
                ".",
                $locale,
                ".expected"
            )),
        )
    };
}

/// Zones on both hemispheres, with half-hour and 45-minute offsets, a
/// half-hour daylight shift (Lord Howe), ICU's rearguard daylight saving time
/// (Dublin), no daylight saving time now (Kolkata, Tokyo, São Paulo) and UTC,
/// in locales whose CLDR 48 names equal CLDR 49's.
const EXPECTED: &[(&str, &str, &str)] = &[
    expected!("America/New_York", "America_New_York", "en-US"),
    expected!("Europe/Kyiv", "Europe_Kyiv", "uk-UA"),
    expected!("Australia/Lord_Howe", "Australia_Lord_Howe", "en-US"),
    expected!("Asia/Kolkata", "Asia_Kolkata", "de-DE"),
    expected!("Europe/Dublin", "Europe_Dublin", "en-US"),
    expected!("UTC", "UTC", "en-US"),
    expected!("Pacific/Chatham", "Pacific_Chatham", "en-US"),
    expected!("America/St_Johns", "America_St_Johns", "fr-FR"),
    expected!("Asia/Tokyo", "Asia_Tokyo", "ja-JP"),
    expected!("America/Sao_Paulo", "America_Sao_Paulo", "en-US"),
];

fn compile(profile: LanguageProfile, source: &str) -> Vec<u8> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{profile:?}: {e}\n{source}"));
    wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    bytes
}

fn inputs(time_zone: &str, locale: &str) -> HostInputs {
    HostInputs {
        time_zone: time_zone.to_owned(),
        locale: locale.to_owned(),
    }
}

fn run(bytes: &[u8], host_inputs: HostInputs) -> RunReport {
    VartaRuntime::new().run_with(
        bytes,
        RunOptions {
            host_inputs,
            ..RunOptions::default()
        },
    )
}

fn logs(report: RunReport) -> Vec<String> {
    let output = report
        .into_result()
        .unwrap_or_else(|error| panic!("{error:?}"));
    output
        .lines
        .iter()
        .flat_map(|line| {
            line.strip_prefix("varta log: ")
                .unwrap_or_else(|| panic!("not a log line: {line}"))
                .split('\n')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn local_time_matches_node_in_every_zone() {
    let bytes = compile(V38, CASE);
    for (zone, locale, expected) in EXPECTED {
        let expected: Vec<&str> = expected.lines().collect();
        assert_eq!(
            logs(run(&bytes, inputs(zone, locale))),
            expected,
            "{zone} {locale}"
        );
    }
}

#[test]
fn the_report_records_the_host_inputs_and_defaults_are_utc_and_en_us() {
    let source = "export function main(): void { console.log(new Date(0).toString()); }";
    let bytes = compile(V38, source);
    let report = VartaRuntime::new().run_with(&bytes, RunOptions::default());
    assert_eq!(report.host_inputs, Some(HostInputs::default()));
    assert_eq!(
        logs(report),
        ["Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)"]
    );
    let report = run(&bytes, inputs("Europe/Kyiv", "uk-UA"));
    assert_eq!(report.host_inputs, Some(inputs("Europe/Kyiv", "uk-UA")));
    assert_eq!(
        logs(report),
        ["Thu Jan 01 1970 03:00:00 GMT+0300 (за східноєвропейським стандартним часом)"]
    );
}

/// An identifier the bundled tzdata does not know is UTC
/// (SystemTimeZoneIdentifier answers an available zone); a locale without
/// data falls back to the root locale's names.
#[test]
fn unknown_zones_are_utc_and_identifiers_ignore_case() {
    let source = "export function main(): void { const d = new Date(Date.UTC(2024, 6, 1, 12)); \
         console.log(`${d.getHours()} ${d.getTimezoneOffset()} ${d.toTimeString()}`); }";
    let bytes = compile(V38, source);
    assert_eq!(
        logs(run(&bytes, inputs("Mars/Olympus_Mons", "en-US"))),
        ["12 0 12:00:00 GMT+0000 (Coordinated Universal Time)"]
    );
    assert_eq!(
        logs(run(&bytes, inputs("america/new_york", "en-US"))),
        ["8 240 08:00:00 GMT-0400 (Eastern Daylight Time)"]
    );
}

/// ECMA-262 §21.4.4.11: getTimezoneOffset is (t − LocalTime(t)) / msPerMinute,
/// a fraction for New York's local mean time (−4:56:02). V8 truncates it to
/// 296; Varta follows the specification. The `GMT−0456` of `toString` drops
/// the seconds in both (TimeZoneString uses HourFromTime and MinFromTime).
#[test]
fn fractional_offsets_follow_the_specification() {
    let source = "export function main(): void { const d = new Date(Date.UTC(1800, 5, 1)); \
         console.log(`${d.getTimezoneOffset()} ${d.getSeconds()} ${d.toTimeString()}`); }";
    let bytes = compile(V38, source);
    assert_eq!(
        logs(run(&bytes, inputs("America/New_York", "en-US"))),
        ["296.03333333333336 58 19:03:58 GMT-0456 (Eastern Daylight Time)"]
    );
}

#[test]
fn malformed_host_inputs_fail_before_the_instance() {
    let bytes = compile(
        V38,
        "export function main(): void { console.log(`${new Date(0).getHours()}`); }",
    );
    for bad in [
        inputs("", "en"),
        inputs("UTC", "en_US"),
        inputs("Europe/Kyiv\u{0}", "uk"),
    ] {
        let report = run(&bytes, bad);
        assert!(
            matches!(report.failure, Some(RuntimeError::HostInput(_))),
            "{:?}",
            report.failure
        );
        assert!(report.output.lines.is_empty());
    }
}

/// The host inputs are imported, capability-free and read once; the zone
/// operations are linked (never imported); a program without local time
/// imports neither.
#[test]
fn v38_imports_the_inputs_and_links_the_zone_operations() {
    for operation in [H::IntlTimeZone, H::IntlLocale] {
        assert_eq!(operation.capability(), None);
        assert!(!operation.linked() && operation.returns_string());
        assert_eq!(
            operation.scalar_signature(),
            (&[][..], &[varta_common::HostScalar::I32][..])
        );
        assert_eq!(operation.minimum_abi(), V38.host_abi());
        assert_eq!(H::from_name(operation.name()), Some(operation));
    }
    for operation in [H::DateZoneOffset, H::DateZoneName] {
        assert!(operation.linked() && V38.links(operation));
        assert_eq!(operation.capability(), None);
    }
    let names = |source: &str| -> Vec<String> {
        let bytes = compile(V38, source);
        let module = wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
        module.imports().map(|i| i.name().to_owned()).collect()
    };
    let local = names("export function main(): void { console.log(new Date(0).toString()); }");
    assert!(local.contains(&"intl_time_zone".to_owned()), "{local:?}");
    assert!(local.contains(&"intl_locale".to_owned()), "{local:?}");
    assert!(
        !local.iter().any(|name| name.starts_with("date_zone_")),
        "{local:?}"
    );
    let utc = names("export function main(): void { console.log(`${Date.UTC(2024, 0)}`); }");
    assert!(!utc.iter().any(|name| name.starts_with("intl_")), "{utc:?}");
}

/// Each instance reads the zone once, on first use, and every later local
/// operation reuses it.
#[test]
fn the_inputs_are_read_once_per_instance() {
    let source = "export function main(): void { for (let i = 0; i < 50; i++) { new Date(2024, 0, i).toString(); } \
         console.log('done'); }";
    let bytes = compile(V38, source);
    let report = run(&bytes, inputs("Europe/Kyiv", "uk-UA"));
    let budget = report.budget.clone().unwrap();
    assert_eq!(logs(report), ["done"]);
    let calls: u64 = budget
        .turns
        .iter()
        .map(|turn| u64::from(turn.host_calls))
        .sum();
    // Two input reads and the log line.
    assert_eq!(calls, 3, "{budget:?}");
}

/// V37 keeps its UTC-only `Date` and never imports the inputs.
#[test]
fn v37_keeps_the_utc_date() {
    let source = "export function main(): void { console.log(new Date(0).toISOString()); }";
    let bytes = compile(V37, source);
    let module = wasmtime::Module::new(&wasmtime::Engine::default(), &bytes).unwrap();
    assert!(module.imports().all(|i| !i.name().starts_with("intl_")));
    let error = varta_compiler::compile_typescript_with_profile(
        "export function main(): void { console.log(`${new Date(0).getHours()}`); }",
        V37,
    )
    .err()
    .map(|e| e.to_string())
    .unwrap_or_default();
    assert!(!error.is_empty());
}
