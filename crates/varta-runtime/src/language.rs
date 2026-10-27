//! Check source-profile negotiation before native compilation/instantiation.
use crate::RuntimeError;
use varta_common::{
    abi,
    language::{
        self, LanguageContract, LanguageError, LanguageInspection, LanguageOrigin, LanguageProfile,
    },
};
use wasmparser::{Parser, Payload};

pub(crate) fn inspect(bytes: &[u8]) -> Result<LanguageInspection, RuntimeError> {
    let mut host = None;
    let mut contract = None;
    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|e| LanguageError(e.to_string()))?;
        if let Payload::CustomSection(section) = payload {
            match section.name() {
                abi::SECTION => {
                    let version = section.data().try_into().ok().map(u32::from_le_bytes);
                    if host.is_some() || !version.is_some_and(abi::supported) {
                        return Err(LanguageError(
                            "unsupported or duplicate Wasm ABI version".into(),
                        )
                        .into());
                    }
                    host = version;
                }
                language::SECTION => {
                    if contract.is_some() {
                        return Err(LanguageError("duplicate language section".into()).into());
                    }
                    contract = Some(LanguageContract::parse(section.data())?);
                }
                _ => {}
            }
        }
    }
    match (contract, host) {
        (Some(contract), Some(version)) if contract.profile.host_abi() == version => {
            Ok(LanguageInspection {
                profile: Some(contract.profile),
                origin: LanguageOrigin::Declared,
            })
        }
        (Some(_), _) => Err(LanguageError(
            "declared profile requires its matching host ABI section".into(),
        )
        .into()),
        (None, Some(abi::VERSION)) => Ok(LanguageInspection {
            profile: Some(LanguageProfile::CheckedI32Utf8V1),
            origin: LanguageOrigin::LegacyHostAbi,
        }),
        (None, None) => Ok(LanguageInspection {
            profile: None,
            origin: LanguageOrigin::Unspecified,
        }),
        (None, Some(_)) => Err(LanguageError(
            "nonlegacy host ABI requires an explicit language profile".into(),
        )
        .into()),
    }
}
