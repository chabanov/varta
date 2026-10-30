//! Fresh OS entropy per call: no shared PRNG, guest seeds or replayable heap state.
use crate::RuntimeContext;
use varta_common::HostOperation;
use wasmtime::{Caller, Linker};

#[derive(Default)]
pub(crate) struct RandomSource {
    // Fault/edge injection is store-local and absent from production builds.
    #[cfg(test)]
    pub(crate) scripted: Option<std::collections::VecDeque<Result<u64, &'static str>>>,
}

impl RandomSource {
    fn word(&mut self) -> Result<u64, wasmtime::Error> {
        #[cfg(test)]
        if let Some(words) = &mut self.scripted {
            return words
                .pop_front()
                .unwrap_or(Err("test entropy exhausted"))
                .map_err(wasmtime::Error::msg);
        }
        getrandom::u64()
            .map_err(|error| wasmtime::Error::msg(format!("random entropy unavailable: {error}")))
    }
}

fn sample(word: u64) -> f64 {
    // Every 53-bit integer is exactly representable. Power-of-two scaling is
    // exact, so the greatest sample is 1 - 2^-53 and zero always has positive sign.
    ((word >> 11) as f64) * (1.0 / 9007199254740992.0)
}

fn draw(
    context: &mut RuntimeContext,
    entropy: impl FnOnce(&mut RuntimeContext) -> Result<u64, wasmtime::Error>,
) -> Result<f64, wasmtime::Error> {
    context.tick()?;
    let word = entropy(context)?;
    context.check_budget()?;
    Ok(sample(word))
}

/// The clock capability `Date.now()` (v37 `date_now`) requires.
pub(crate) const CLOCK_CAPABILITY: &str = "time.now";

pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    linker.func_wrap(
        "varta",
        HostOperation::DateNow.name(),
        |mut caller: Caller<'_, RuntimeContext>| -> Result<f64, wasmtime::Error> {
            caller.data_mut().tick()?;
            // Checked before reading the clock; JavaScript has no failure
            // path for Date.now(), so a denial ends the execution.
            if !caller.data().policy.enabled(CLOCK_CAPABILITY) {
                return Err(wasmtime::Error::new(
                    varta_common::VartaError::CapabilityDenied {
                        capability: CLOCK_CAPABILITY.into(),
                        target: "Date.now".into(),
                    },
                ));
            }
            let elapsed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(wasmtime::Error::new)?;
            // Whole milliseconds, like a JavaScript time value.
            Ok(elapsed.as_millis() as f64)
        },
    )?;
    linker.func_wrap(
        "varta",
        HostOperation::MathRandom.name(),
        |mut caller: Caller<'_, RuntimeContext>| -> Result<f64, wasmtime::Error> {
            draw(caller.data_mut(), |context| context.random.word())
        },
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_does_not_return_a_sample_if_entropy_access_outlives_the_deadline() {
        let runtime = crate::VartaRuntime::new();
        let mut execution = crate::execution::Execution::new(&runtime.engine, &[], None).unwrap();
        let error = draw(execution.store.data_mut(), |context| {
            // Simulate elapsed time inside the provider without a timing race.
            context.deadline = std::time::Instant::now();
            Ok(1 << 63)
        })
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("host operation budget exhausted")
        );
        assert_eq!(execution.store.data().host_calls, 1);
    }

    #[test]
    fn random_samples_use_53_bits_with_exact_endpoints_and_no_rounding_to_one() {
        assert_eq!(sample(0).to_bits(), 0);
        assert_eq!(sample(2047).to_bits(), 0);
        assert_eq!(sample(2048), 2.0_f64.powi(-53));
        assert_eq!(sample(u64::MAX).to_bits(), 1.0_f64.to_bits() - 1);
        for bit in 11..64 {
            assert_eq!(sample(1_u64 << bit), 2.0_f64.powi(bit - 64));
        }
        // Sweep 16 high and low bits independently; each discarded low bit
        // leaves the sample unchanged and every selected lattice point is exact.
        for value in 0..=u16::MAX {
            let numerator = (u64::from(value) << 37) | u64::from(value);
            let x = sample((numerator << 11) | 2047);
            assert!((0.0..1.0).contains(&x));
            assert!(!x.is_sign_negative());
            assert_eq!(x * 9007199254740992.0, numerator as f64);
            assert_eq!(x, sample(numerator << 11));
        }
    }
}
