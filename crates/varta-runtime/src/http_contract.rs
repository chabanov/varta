use crate::RuntimeError;
use varta_common::http::{HttpContract, HttpError, SECTION};

pub(crate) fn read(bytes: &[u8]) -> Result<Option<HttpContract>, RuntimeError> {
    let mut contract = None;
    for payload in wasmparser::Parser::new(0).parse_all(bytes) {
        if let wasmparser::Payload::CustomSection(section) =
            payload.map_err(wasmtime::Error::new)?
            && section.name() == SECTION
        {
            if contract.is_some() {
                return Err(HttpError::Contract("duplicate service section".into()).into());
            }
            contract = Some(HttpContract::parse(section.data())?);
        }
    }
    Ok(contract)
}
