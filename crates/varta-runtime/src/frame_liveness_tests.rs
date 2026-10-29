//! v35 `frame_liveness`: frozen cell-layout output of earlier profiles and a
//! deterministic cost benchmark of liveness frames against v34 cell frames.
//! Costs are guest fuel under the production budgets, never wall-clock time.
use super::*;
use varta_common::language::LanguageProfile;

const V31: LanguageProfile = LanguageProfile::CheckedF64Utf16V31;
const V33: LanguageProfile = LanguageProfile::CheckedF64Utf16V33;
const V34: LanguageProfile = LanguageProfile::CheckedF64Utf16V34;
const V35: LanguageProfile = LanguageProfile::CheckedF64Utf16V35;

/// FNV-1a over the module bytes: a stable digest for frozen-output pins.
fn digest(bytes: &[u8]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    bytes.iter().fold(OFFSET, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(PRIME)
    })
}
const HOOK_PROGRAM: &str = "function task(a: number, s: string): number {
  const first = __varta_suspend(a + s.length);
  let text = s + \"!\";
  const second = __varta_suspend(first * 2);
  try {
    text = text + String(second);
    const third = __varta_suspend(second + 1);
    return first + third + text.length;
  } finally {
    console.log(text);
  }
}
export function main(): void {}";

/// Module digests produced before liveness frames existed (commit 636c7c5).
/// Earlier profiles keep their frame layout and catalog byte for byte.
const FROZEN: [(&str, LanguageProfile, u64); 22] = [
    ("generator protocol", V33, 0x9d95_2d60_36ec_655d),
    ("generator protocol", V34, 0x40c4_5a5c_7f6f_f9cd),
    ("generator finally", V33, 0xb6d6_3499_fe71_a4c9),
    ("generator finally", V34, 0x0762_b206_7e20_2085),
    ("generator for_of", V33, 0x5977_5863_a3a4_4ead),
    ("generator for_of", V34, 0x35d3_3f14_2757_ebbe),
    ("generator values", V33, 0x5f2a_dbd3_96ca_0fde),
    ("generator values", V34, 0xf28b_ba5d_5a83_2e55),
    ("generator lifetime", V33, 0xed72_eacc_f260_197d),
    ("generator lifetime", V34, 0xe371_0600_ebdc_b869),
    ("generator guide", V33, 0x3372_3efe_1f8e_ead0),
    ("generator guide", V34, 0xba12_1656_749b_1378),
    ("promise ordering", V34, 0x2ea0_da3c_802f_c032),
    ("promise rejections", V34, 0xe179_b6f0_b072_6e4c),
    ("async order", V34, 0x6e60_cdf1_7974_c31d),
    ("async features", V34, 0x0c14_dbeb_0bed_3bb2),
    ("async guide", V34, 0xf4ca_ddd2_4666_3ad1),
    ("async values", V34, 0x01bc_22b1_2c01_ed7a),
    ("async lifetime", V34, 0x1eed_2af6_67fa_098a),
    ("hook program", V31, 0x1a94_8093_6f0f_737b),
    ("hook program", V33, 0xef50_f3cd_5691_f4d4),
    ("hook program", V34, 0x2e7a_6c58_837b_09bc),
];
fn source(name: &str) -> &'static str {
    match name {
        "generator protocol" => include_str!("generator_cases/protocol.ts"),
        "generator finally" => include_str!("generator_cases/finally.ts"),
        "generator for_of" => include_str!("generator_cases/for_of.ts"),
        "generator values" => include_str!("generator_cases/values.ts"),
        "generator lifetime" => include_str!("generator_cases/lifetime.ts"),
        "generator guide" => include_str!("generator_cases/guide.ts"),
        "promise ordering" => include_str!("promise_cases/ordering.ts"),
        "promise rejections" => include_str!("promise_cases/rejections.ts"),
        "async order" => include_str!("promise_cases/async_order.ts"),
        "async features" => include_str!("promise_cases/async_features.ts"),
        "async guide" => include_str!("promise_cases/guide.ts"),
        "async values" => include_str!("promise_cases/values.ts"),
        "async lifetime" => include_str!("promise_cases/lifetime.ts"),
        _ => HOOK_PROGRAM,
    }
}

#[test]
fn earlier_profiles_keep_byte_identical_frame_modules() {
    for (name, profile, expected) in FROZEN {
        let bytes = if name == "hook program" {
            varta_compiler::suspension_test_hook::compile_with_profile(HOOK_PROGRAM, profile)
        } else {
            varta_compiler::compile_typescript_with_profile(source(name), profile)
        }
        .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(digest(&bytes), expected, "{name} under {profile:?}");
    }
}

/// Guest fuel of `_start` and the whole microtask checkpoint, or `None` when
/// the execution fails within the production budgets.
fn fuel(source: &str, profile: LanguageProfile) -> Option<u64> {
    let bytes = varta_compiler::compile_typescript_with_profile(source, profile)
        .unwrap_or_else(|e| panic!("{e}\n{source}"));
    let runtime = VartaRuntime::new();
    let module = runtime.prepare_module(&bytes).ok()?;
    let mut execution = execution::Execution::new(&runtime.engine, &[], None).ok()?;
    let instance = execution.instantiate_prepared(&module).ok()?;
    let store = &mut execution.store;
    let start = instance.get_func(&mut *store, "_start")?;
    start.call(&mut *store, &[], &mut []).ok()?;
    exceptions::finish(&instance, store).ok()?;
    scheduler::checkpoint(&instance, store).ok()?;
    Some(abi::FUEL - store.get_fuel().ok()?)
}
/// Marginal fuel of one more unit of work: `(fuel(2n) - fuel(n)) / n`.
fn marginal(program: impl Fn(usize) -> String, profile: LanguageProfile) -> u64 {
    const BASE: usize = 10;
    let fuel = |count| fuel(&program(count), profile).expect("within budget");
    (fuel(2 * BASE) - fuel(BASE)) / BASE as u64
}

fn awaits(count: usize) -> String {
    format!(
        "export async function main(): Promise<void> {{
  for (let i = 0; i < {count}; i++) {{
    await i;
  }}
}}"
    )
}
/// String temporaries that die before each `await`, and a running total and
/// label that cross it.
fn string_awaits(count: usize) -> String {
    format!(
        r#"async function work(label: string): Promise<number> {{
  let total = 0;
  for (let i = 0; i < {count}; i++) {{
    const a = label + String(i);
    const b = a + "-" + label;
    total = total + b.length;
    await null;
  }}
  return total;
}}
export async function main(): Promise<void> {{
  const total = await work("x");
  console.log(String(total));
}}"#
    )
}
fn yields(count: usize) -> String {
    format!(
        r#"function* count(n: number): Generator<number> {{
  for (let i = 0; i < n; i++) {{
    yield i;
  }}
}}
export function main(): void {{
  let total = 0;
  for (const value of count({count})) {{
    total = total + value;
  }}
  console.log(String(total));
}}"#
    )
}
/// `count` generators suspended at once, each holding two strings across its
/// first `yield`. Generators cannot be array elements, so each one is held by
/// a local of one level of a recursion and resumed on the way back.
fn suspended(count: usize) -> String {
    format!(
        r#"function* hold(label: string): Generator<string> {{
  const head = label + "-a";
  const tail = label + "-b";
  yield head;
  yield tail + head;
}}
function nest(depth: number): number {{
  if (depth === 0) {{
    return 0;
  }}
  const g = hold("g" + String(depth));
  g.next();
  const below = nest(depth - 1);
  const last = g.next();
  return below + (last.done ? 0 : 1);
}}
export function main(): void {{
  console.log(String(nest({count})));
}}"#
    )
}
/// The largest count for which `program` still completes within the
/// production budgets (binary search; the cost is monotone in the count).
fn maximum(program: impl Fn(usize) -> String, profile: LanguageProfile) -> usize {
    const CEILING: usize = 1 << 16;
    let (mut low, mut high) = (0, CEILING);
    while low < high {
        let middle = (low + high).div_ceil(2);
        if fuel(&program(middle), profile).is_some() {
            low = middle;
        } else {
            high = middle - 1;
        }
    }
    low
}

/// The figures recorded in architecture.md (v35 frame liveness) come from
/// this test; run it with `--nocapture` to print them.
#[test]
fn liveness_frames_cost_less_fuel_per_suspension_than_cell_frames() {
    let costs = [V34, V35].map(|profile| {
        (
            marginal(awaits, profile),
            marginal(string_awaits, profile),
            marginal(yields, profile),
            marginal(suspended, profile),
            maximum(suspended, profile),
        )
    });
    for (profile, (wait, text, step, generator, most)) in [V34, V35].iter().zip(costs) {
        println!(
            "{profile:?}: fuel per await {wait}, per string-loop await {text}, per yield {step}, \
             per suspended generator {generator}; at most {most} suspended generators"
        );
    }
    let [
        (wait, text, step, generator, most),
        (wait35, text35, step35, generator35, most35),
    ] = costs;
    // Liveness frames save fewer values and allocate no frame cells.
    assert!(wait35 < wait, "{wait35} >= {wait}");
    assert!(text35 < text, "{text35} >= {text}");
    assert!(step35 < step, "{step35} >= {step}");
    assert!(generator35 < generator, "{generator35} >= {generator}");
    assert!(most35 > most, "{most35} <= {most}");
    // The same programs behave identically under both layouts.
    for program in [awaits(3), string_awaits(3), yields(3), suspended(3)] {
        let lines = [V34, V35].map(|profile| {
            let bytes = varta_compiler::compile_typescript_with_profile(&program, profile).unwrap();
            VartaRuntime::new()
                .run_wasm_bytes(&bytes, &[])
                .unwrap()
                .lines
        });
        assert_eq!(lines[0], lines[1], "{program}");
    }
}
