#![forbid(unsafe_code)]
#![no_std]
//! The time zone and locale layer of v38 `Date` and `Intl` (K9 D4), shared
//! by every host through the linked guest intrinsic library.
//!
//! - [`zone`]: named time zones of the IANA Time Zone Database (TZif data,
//!   [`zone::DATA_VERSION`]) — offsets at instants and of local times as
//!   ECMA-262 §21.4.1.20–26 define them;
//! - [`posix`]: the POSIX TZ rules of TZif footers;
//! - [`names`]: the localized zone name of `Date.prototype.toString`, from
//!   ICU4X with CLDR data;
//! - [`data`]: the ICU4X data provider over the module's linked locale data
//!   segments (`varta_common::intl_data`).
//!
//! The host supplies only its zone identifier and locale tag (recorded host
//! inputs); every offset, transition and name is computed here, so the
//! native runtime and the browser host agree on every output.
extern crate alloc;

pub mod calendar;
pub mod data;
pub mod names;
pub mod posix;
pub mod zone;
