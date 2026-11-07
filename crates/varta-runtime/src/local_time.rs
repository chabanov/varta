//! V38 local time (`local_time`, K9 D4): the recorded host inputs.
//!
//! `intl_time_zone` and `intl_locale` return the instance's zone identifier
//! and locale tag ([`varta_common::host_inputs::HostInputs`]), which the
//! embedder supplied in [`crate::RunOptions::host_inputs`] (UTC and `en-US`
//! by default) and the run report records. Both are capability-free and
//! charged to the host-call budget like any import; nothing here reads the
//! clock or computes an offset: the linked `date_zone_offset` and
//! `date_zone_name` do that in guest code.
use crate::{RuntimeContext, host};
use varta_common::HostOperation;
use varta_common::host_inputs::HostInputs;
use wasmtime::{Caller, Linker};

/// The link the system zone setting is on POSIX systems (macOS and Linux).
const LOCALTIME_LINK: &str = "/etc/localtime";
/// The directory component after which a zoneinfo path names the zone.
const ZONEINFO: &str = "zoneinfo/";
/// The environment variables a POSIX locale is read from, in ICU's order
/// for the message category (`uprv_getPOSIXIDForCategory`).
const LOCALE_VARIABLES: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];

/// The native host's zone and locale, as Node 24 (ICU) reads them: the zone
/// from `TZ` (an identifier, `:identifier` or a zoneinfo path), else the
/// zoneinfo file `/etc/localtime` links to, else UTC; the locale from
/// `LC_ALL`, `LC_MESSAGES` or `LANG` (the first set), else `en-US`. Values
/// that are not well-formed settings fall back to the defaults; whether the
/// zone exists is decided by the guest's data.
#[must_use]
pub fn system_host_inputs() -> HostInputs {
    let defaults = HostInputs::default();
    let zone = std::env::var("TZ")
        .ok()
        .filter(|zone| !zone.is_empty())
        .map(|zone| zone.trim_start_matches(':').to_owned())
        .or_else(|| {
            std::fs::read_link(LOCALTIME_LINK)
                .ok()
                .and_then(|path| path.to_str().map(ToOwned::to_owned))
        })
        .map(|zone| match zone.rfind(ZONEINFO) {
            Some(at) => zone[at + ZONEINFO.len()..].to_owned(),
            None => zone,
        });
    let locale = LOCALE_VARIABLES
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .and_then(|name| HostInputs::locale_from_posix(&name));
    let time_zone = zone
        .filter(|zone| {
            HostInputs {
                time_zone: zone.clone(),
                ..HostInputs::default()
            }
            .validate()
            .is_ok()
        })
        .unwrap_or(defaults.time_zone);
    let locale = locale
        .filter(|locale| {
            HostInputs {
                locale: locale.clone(),
                ..HostInputs::default()
            }
            .validate()
            .is_ok()
        })
        .unwrap_or(defaults.locale);
    HostInputs { time_zone, locale }
}

pub(super) fn register(linker: &mut Linker<RuntimeContext>) -> Result<(), wasmtime::Error> {
    for operation in [HostOperation::IntlTimeZone, HostOperation::IntlLocale] {
        linker.func_wrap(
            varta_common::abi::HOST_MODULE,
            operation.name(),
            move |mut caller: Caller<'_, RuntimeContext>| -> Result<i32, wasmtime::Error> {
                host::reset(&mut caller)?;
                let inputs = &caller.data().inputs;
                let text = if operation == HostOperation::IntlTimeZone {
                    inputs.time_zone.clone()
                } else {
                    inputs.locale.clone()
                };
                match host::result16(&mut caller, text.as_bytes()) {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        host::failure(&mut caller, error)?;
                        Ok(0)
                    }
                }
            },
        )?;
    }
    Ok(())
}
