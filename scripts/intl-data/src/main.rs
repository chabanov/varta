//! The generator of the v38 locale data blobs (`scripts/build-intl-data.py`).
//!
//! Every payload of every exported marker is loaded from ICU4X's compiled
//! data (the data crates' baked macros, implemented on [`Gen`] with
//! iteration), serialized in postcard 1 form, and written as one segment per
//! marker into its service's blob (`varta_common::intl_data`). `intl.json`
//! records the data versions and the sizes per service and marker.
extern crate alloc;

mod markers;

use icu_provider::prelude::*;
use icu_provider::{IterableDataProvider, fallback::LocaleFallbackPriority};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    process::ExitCode,
};
use varta_common::intl_data::{
    AVAILABLE_LOCALES_HASH, BlobWriter, Priority, SegmentWriter, key, marker_flag,
};

/// The `icu::` paths the baked macros name.
pub mod icu {
    pub use icu_calendar as calendar;
    pub use icu_casemap as casemap;
    pub use icu_collator as collator;
    pub use icu_collections as collections;
    pub use icu_datetime as datetime;
    pub use icu_decimal as decimal;
    pub use icu_experimental as experimental;
    pub use icu_list as list;
    pub use icu_locale as locale;
    pub use icu_plurals as plurals;
    pub use icu_segmenter as segmenter;
    pub use icu_time as time;
}

/// The provider the baked macros are implemented on.
#[derive(Debug)]
pub struct Gen;

/// The blobs being written, with their size records.
#[derive(Default)]
pub struct Exporter {
    blobs: BTreeMap<&'static str, BlobWriter>,
    records: BTreeMap<&'static str, Vec<Value>>,
    errors: Vec<String>,
}

impl Exporter {
    pub fn marker<M>(&mut self, service: &'static str)
    where
        M: DataMarker,
        Gen: IterableDataProvider<M>,
        for<'a> <M::DataStruct as yoke::Yokeable<'a>>::Output: serde::Serialize,
    {
        match Gen.iter_ids() {
            Ok(ids) => self.export::<M>(service, ids),
            Err(error) => self.errors.push(format!("{}: {error}", M::INFO.id.name())),
        }
    }

    /// A singleton marker: its one root payload.
    pub fn singleton<M>(&mut self, service: &'static str)
    where
        M: DataMarker,
        Gen: DataProvider<M>,
        for<'a> <M::DataStruct as yoke::Yokeable<'a>>::Output: serde::Serialize,
    {
        if !M::INFO.is_singleton {
            self.errors
                .push(format!("{}: not a singleton", M::INFO.id.name()));
            return;
        }
        self.export::<M>(service, BTreeSet::from([DataIdentifierCow::default()]));
    }

    fn export<M>(&mut self, service: &'static str, ids: BTreeSet<DataIdentifierCow<'static>>)
    where
        M: DataMarker,
        Gen: DataProvider<M>,
        for<'a> <M::DataStruct as yoke::Yokeable<'a>>::Output: serde::Serialize,
    {
        let info = M::INFO;
        let name = info.id.name();
        let hash = u32::from_le_bytes(info.id.hashed().to_bytes());
        if hash == AVAILABLE_LOCALES_HASH {
            self.errors.push(format!("{name}: reserved hash"));
            return;
        }
        let mut payloads = Vec::new();
        let mut checksum = None;
        for id in &ids {
            let request = DataRequest {
                id: id.as_borrowed(),
                ..Default::default()
            };
            let response = match DataProvider::<M>::load(&Gen, request) {
                Ok(response) => response,
                Err(error) => {
                    self.errors.push(format!("{name} {id:?}: {error}"));
                    return;
                }
            };
            if response.metadata.checksum.is_some() {
                checksum = response.metadata.checksum;
            }
            let bytes = match postcard::to_allocvec(response.payload.get()) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.errors.push(format!("{name} {id:?}: {error}"));
                    return;
                }
            };
            let locale = id.locale.to_string();
            payloads.push((key(&locale, id.marker_attributes.as_str()), bytes));
        }
        let largest = payloads
            .iter()
            .map(|(_, bytes)| bytes.len())
            .max()
            .unwrap_or(0);
        let mut segment = SegmentWriter::new(checksum);
        for (key, bytes) in payloads {
            segment.insert(key, bytes);
        }
        let segment = match segment.finish() {
            Ok(segment) => segment,
            Err(error) => {
                self.errors.push(format!("{name}: {error}"));
                return;
            }
        };
        let priority = match info.fallback_config.priority {
            LocaleFallbackPriority::Script => Priority::Script,
            LocaleFallbackPriority::Region => Priority::Region,
            _ => Priority::Language,
        };
        let flags = if info.is_singleton {
            marker_flag::SINGLETON
        } else {
            0
        };
        self.records.entry(service).or_default().push(json!({
            "name": name,
            "hash": hash,
            "entries": ids.len(),
            "bytes": segment.len(),
            "largestPayload": largest,
        }));
        self.blobs
            .entry(service)
            .or_default()
            .insert(hash, priority, flags, name, segment);
    }
}

fn main() -> ExitCode {
    let Some(output) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("usage: varta-intl-data <output directory>");
        return ExitCode::FAILURE;
    };
    let mut exporter = Exporter::default();
    markers::export(&mut exporter);
    if !exporter.errors.is_empty() {
        for error in &exporter.errors {
            eprintln!("varta-intl-data: {error}");
        }
        return ExitCode::FAILURE;
    }
    let mut services = serde_json::Map::new();
    for (service, writer) in exporter.blobs {
        let bytes = match writer.finish() {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!("varta-intl-data: {service}: {error}");
                return ExitCode::FAILURE;
            }
        };
        if let Err(error) = std::fs::write(output.join(format!("{service}.bin")), &bytes) {
            eprintln!("varta-intl-data: {service}: {error}");
            return ExitCode::FAILURE;
        }
        services.insert(
            service.into(),
            json!({
                "bytes": bytes.len(),
                "markers": exporter.records.remove(service).unwrap_or_default(),
            }),
        );
    }
    let metadata = json!({
        "format": varta_common::intl_data::FORMAT_VERSION,
        "cldr": icu_datetime_data::cldr_tag!(),
        "icu4x": "2.4.0",
        "services": services,
    });
    let text = match serde_json::to_string_pretty(&metadata) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("varta-intl-data: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = std::fs::write(output.join("intl.json"), text + "\n") {
        eprintln!("varta-intl-data: {error}");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
