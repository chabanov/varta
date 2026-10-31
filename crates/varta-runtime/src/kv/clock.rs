//! The KV clock: the one time source Varta's own KV code reads.
//!
//! It stamps `expireIn` as Deno does: a `set` with `expireIn` milliseconds
//! stores `expiration_ms` = the clock's instant of the commit plus
//! `expireIn`. Every mutation of one commit shares one reading.
//!
//! Whether a stored entry has expired is decided by the storage engine, not
//! by this clock: `denokv_sqlite` reads the system clock itself when a
//! writable database's worker collects expired entries (at open, and on its
//! own schedule afterwards), as every Deno process sharing the file does.
//! Reads return what the file holds, so an entry may still be read after its
//! expiration until it is collected, and a read-only database is never
//! collected. Production therefore stamps on the system clock too; tests pin
//! the clock to a fixed instant ([`Clock::Fixed`]) so a stamp is
//! reproducible.
use std::time::SystemTime;

/// Where the KV code reads the time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum Clock {
    /// The system clock (production).
    #[default]
    System,
    /// A fixed instant (tests).
    #[cfg(test)]
    Fixed(SystemTime),
}

impl Clock {
    /// The current instant on this clock.
    pub(crate) fn now(self) -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from(match self {
            Self::System => SystemTime::now(),
            #[cfg(test)]
            Self::Fixed(at) => at,
        })
    }
}

/// The expiration of an entry set at `now` with `expireIn` = `milliseconds`,
/// or `None` when it is outside the representable range.
pub(crate) fn expire_at(
    now: chrono::DateTime<chrono::Utc>,
    milliseconds: u64,
) -> Option<chrono::DateTime<chrono::Utc>> {
    i64::try_from(milliseconds)
        .ok()
        .and_then(chrono::TimeDelta::try_milliseconds)
        .and_then(|delta| now.checked_add_signed(delta))
}
