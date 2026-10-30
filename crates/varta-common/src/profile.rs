//! Development fuel profiling (`varta dev --profile-fuel`).
//!
//! A profile build is a separately compiled, instrumented copy of the program:
//! every guest function (user code, engine preludes and compiler helpers)
//! calls the `enter` probe on entry and the `exit` probe before each return.
//! The runtime's probes read the store's remaining fuel, attribute the
//! difference to the active function and refund the probe's own cost, so the
//! profiled run consumes exactly the fuel of the uninstrumented program and
//! hits the same budget at the same point. Probe imports live outside the
//! `varta` host namespace, are accepted only by the runtime's profiling entry
//! point and never by artifact validation, so a profile build cannot be
//! packaged or run as an ordinary program; ordinary builds contain no probe
//! and pay nothing.
use serde::Serialize;

/// Import module of the probes.
pub const MODULE: &str = "varta_profile";
/// Custom section naming the code-section functions of a profile build.
pub const NAMES_SECTION: &str = "varta.profile.names";
/// Longest recorded function name, in bytes.
pub const MAX_NAME_BYTES: usize = 256;
/// Largest names section, in bytes.
pub const MAX_NAMES_BYTES: usize = 1 << 20;

/// The probe imports, appended after the host imports in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    Enter,
    Exit,
}
impl Probe {
    pub const ALL: [Self; 2] = [Self::Enter, Self::Exit];
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Enter => "enter",
            Self::Exit => "exit",
        }
    }
    /// Position after the last host import.
    #[must_use]
    pub fn offset(self) -> u32 {
        Self::ALL
            .iter()
            .position(|probe| *probe == self)
            .unwrap_or(0) as u32
    }
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|probe| probe.name() == name)
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProfileError {
    #[error("malformed fuel profile names")]
    MalformedNames,
}

/// `[count:u32]` then `count` entries `[len:u32][utf8]`, little-endian.
#[must_use]
pub fn encode_names(names: &[String]) -> Vec<u8> {
    let mut bytes = (names.len() as u32).to_le_bytes().to_vec();
    for name in names {
        let mut end = name.len().min(MAX_NAME_BYTES);
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        bytes.extend((end as u32).to_le_bytes());
        bytes.extend(&name.as_bytes()[..end]);
    }
    bytes
}

pub fn decode_names(bytes: &[u8]) -> Result<Vec<String>, ProfileError> {
    if bytes.len() > MAX_NAMES_BYTES {
        return Err(ProfileError::MalformedNames);
    }
    let mut rest = bytes;
    let word = |rest: &mut &[u8]| -> Result<usize, ProfileError> {
        let (head, tail) = rest
            .split_first_chunk::<4>()
            .ok_or(ProfileError::MalformedNames)?;
        *rest = tail;
        Ok(u32::from_le_bytes(*head) as usize)
    };
    let count = word(&mut rest)?;
    // Every entry takes at least its length word.
    if count > rest.len() / 4 {
        return Err(ProfileError::MalformedNames);
    }
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let len = word(&mut rest)?;
        if len > MAX_NAME_BYTES || len > rest.len() {
            return Err(ProfileError::MalformedNames);
        }
        let (name, tail) = rest.split_at(len);
        names.push(
            std::str::from_utf8(name)
                .map_err(|_| ProfileError::MalformedNames)?
                .to_owned(),
        );
        rest = tail;
    }
    if !rest.is_empty() {
        return Err(ProfileError::MalformedNames);
    }
    Ok(names)
}

/// Fuel attributed to one guest function.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FunctionFuel {
    pub name: String,
    /// Completed and unfinished activations.
    pub calls: u64,
    /// Fuel of the function's own instructions and the host work it caused.
    #[serde(rename = "selfFuel")]
    pub self_fuel: u64,
    /// Fuel including callees; a recursive function counts its outermost
    /// activations only.
    #[serde(rename = "totalFuel")]
    pub total_fuel: u64,
}

/// Per-function fuel of one profiled execution, by descending self fuel.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct FuelProfile {
    /// Fuel the whole execution consumed, as without instrumentation.
    pub consumed: u64,
    /// The execution's fuel budget.
    pub budget: u64,
    pub functions: Vec<FunctionFuel>,
    /// Fuel outside every guest function: module instantiation (data
    /// segments and initial memory) and host work between guest calls.
    pub outside: u64,
    /// Activations still open when the execution stopped (a trap or an
    /// exhausted budget); their fuel is attributed up to that point.
    pub unfinished: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_reject_malformed_sections() {
        let names = vec![
            "main (main.ts:1:1)".to_owned(),
            String::new(),
            "é".repeat(200),
        ];
        let decoded = decode_names(&encode_names(&names)).unwrap();
        assert_eq!(decoded[..2], names[..2]);
        // Long names are cut at a character boundary.
        assert!(decoded[2].len() <= MAX_NAME_BYTES && names[2].starts_with(&decoded[2]));
        let bytes = encode_names(&names);
        for malformed in [
            &bytes[..bytes.len() - 1],
            &bytes[..3],
            &[0xff, 0xff, 0xff, 0x7f][..],
            &[1, 0, 0, 0, 1, 0, 0, 0, 0xff][..],
        ] {
            assert_eq!(decode_names(malformed), Err(ProfileError::MalformedNames));
        }
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(decode_names(&trailing), Err(ProfileError::MalformedNames));
        assert_eq!(Probe::from_name("exit"), Some(Probe::Exit));
        assert_eq!(Probe::Exit.offset(), 1);
        assert_eq!(Probe::from_name("log"), None);
    }
}
