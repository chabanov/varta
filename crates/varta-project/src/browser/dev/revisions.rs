//! Immutable, content-addressed delivery generations. Paths never fall back to
//! the newest bundle when a requested generation has expired.
use super::{File, INDEX, LOADER, media_type};
use crate::{ProjectError, Result};
use hyper::{body::Bytes, header};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

const PREFIX: &str = "/__varta/";
const RETAINED_REVISIONS: usize = 2;

pub(super) struct Revision {
    pub(super) id: String,
    pub(super) files: BTreeMap<String, File>,
}
impl Revision {
    pub(super) fn new(mut files: BTreeMap<String, Vec<u8>>) -> Result<Self> {
        let mut hash = Sha256::new();
        hash.update(b"varta-browser-dev-revision-v1\0");
        for (name, bytes) in &files {
            hash.update((name.len() as u64).to_le_bytes());
            hash.update(name.as_bytes());
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(bytes);
        }
        let id = format!("{:x}", hash.finalize());
        let html = files
            .get_mut(INDEX)
            .ok_or_else(|| ProjectError::Artifact("missing development page".into()))?;
        let text = std::str::from_utf8(html).map_err(|e| ProjectError::Artifact(e.to_string()))?;
        let source = format!(" data-varta-dev src=\"./{LOADER}\"");
        if text.matches(&source).count() != 1 {
            return Err(ProjectError::Artifact(
                "browser development revision boundary changed".into(),
            ));
        }
        *html = text
            .replace(
                &source,
                &format!(" data-varta-dev src=\"{PREFIX}{id}/{LOADER}\""),
            )
            .into_bytes();
        let files = files
            .into_iter()
            .map(|(name, bytes)| {
                let file = File {
                    media: media_type(&name),
                    length: header::HeaderValue::from_str(&bytes.len().to_string())
                        .map_err(|e| ProjectError::Artifact(e.to_string()))?,
                    etag: header::HeaderValue::from_str(&format!(
                        "\"{}\"",
                        varta_runtime::artifact_identity(&bytes)
                    ))
                    .map_err(|e| ProjectError::Artifact(e.to_string()))?,
                    bytes: Bytes::from(bytes),
                };
                Ok((name, file))
            })
            .collect::<Result<_>>()?;
        Ok(Self { id, files })
    }
    pub(super) fn prefix(&self) -> String {
        format!("{PREFIX}{}/", self.id)
    }
}

pub(super) struct Delivery {
    revisions: VecDeque<Arc<Revision>>,
    diagnostic: Option<String>,
}
impl Delivery {
    pub(super) fn new(revision: Revision) -> Self {
        let mut delivery = Self {
            revisions: VecDeque::new(),
            diagnostic: None,
        };
        delivery.publish(revision);
        delivery
    }
    pub(super) fn latest(&self) -> &Revision {
        self.revisions.front().expect("delivery is initialized")
    }
    pub(super) fn complete(&mut self, result: Result<Revision>) {
        match result {
            Ok(revision) => {
                self.diagnostic = None;
                self.publish(revision);
            }
            Err(error) => {
                let mut message = error.to_string();
                let mut end = message.len().min(16 * 1024);
                while !message.is_char_boundary(end) {
                    end -= 1;
                }
                message.truncate(end);
                self.diagnostic = Some(message);
            }
        }
    }
    pub(super) fn status(&self) -> File {
        let bytes = serde_json::to_vec(
            &serde_json::json!({"revision":self.latest().id,"error":self.diagnostic}),
        )
        .expect("status consists of JSON strings");
        File {
            length: header::HeaderValue::from_str(&bytes.len().to_string())
                .expect("bounded decimal length"),
            etag: header::HeaderValue::from_static("\"dev-status\""),
            media: "application/json",
            bytes: Bytes::from(bytes),
        }
    }
    pub(super) fn publish(&mut self, revision: Revision) -> bool {
        if self
            .revisions
            .front()
            .is_some_and(|current| current.id == revision.id)
        {
            return false;
        }
        self.revisions.retain(|previous| previous.id != revision.id);
        self.revisions.push_front(Arc::new(revision));
        self.revisions.truncate(RETAINED_REVISIONS);
        true
    }
    pub(super) fn file(&self, path: &str) -> Option<&File> {
        if path == "/" || path == "/index.html" {
            return self.latest().files.get(INDEX);
        }
        let (id, name) = path.strip_prefix(PREFIX)?.split_once('/')?;
        let revision = self.revisions.iter().find(|revision| revision.id == id)?;
        revision
            .files
            .get(if name.is_empty() { INDEX } else { name })
    }
}

#[cfg(test)]
mod tests;
