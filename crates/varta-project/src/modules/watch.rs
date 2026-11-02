//! Opt-in development observations of actual bounded reads, including failures.
//! Rechecking never follows a path more permissively than the original loader.
use super::SourceLoader;
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io,
    path::{Path, PathBuf},
};

const MAX_OBSERVATIONS: usize = 4096;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Input {
    Read {
        path: PathBuf,
        maximum: usize,
        external: bool,
    },
    Probe(PathBuf),
}
#[derive(PartialEq, Eq)]
enum State {
    Bytes([u8; 32]),
    Exists(bool),
    Error(io::ErrorKind),
}
fn bytes(result: &io::Result<Vec<u8>>) -> State {
    match result {
        Ok(bytes) => State::Bytes(Sha256::digest(bytes).into()),
        Err(error) => State::Error(error.kind()),
    }
}
fn exists(result: &io::Result<bool>) -> State {
    match result {
        Ok(exists) => State::Exists(*exists),
        Err(error) => State::Error(error.kind()),
    }
}
fn external_read(path: &Path, maximum: usize) -> io::Result<Vec<u8>> {
    crate::artifact::read_bounded(path, maximum).map_err(|error| match error {
        crate::ProjectError::Io(error) => error,
        error => io::Error::new(io::ErrorKind::InvalidData, error),
    })
}
#[derive(Default)]
pub(super) struct Observations {
    inputs: RefCell<BTreeMap<Input, State>>,
    conflicted: Cell<bool>,
}
impl Observations {
    fn insert(&self, input: Input, state: State) -> io::Result<()> {
        let mut inputs = self.inputs.borrow_mut();
        if inputs.len() >= MAX_OBSERVATIONS && !inputs.contains_key(&input) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "development input observation limit exceeded",
            ));
        }
        // Conflicting repeated reads invalidate this attempt even if a writer
        // restores the original bytes before the final comparison.
        if inputs
            .get(&input)
            .is_some_and(|previous| previous != &state)
        {
            self.conflicted.set(true);
        }
        inputs.entry(input).or_insert(state);
        Ok(())
    }
    pub(super) fn read(
        &self,
        path: &Path,
        maximum: usize,
        external: bool,
        result: &io::Result<Vec<u8>>,
    ) -> io::Result<()> {
        self.insert(
            Input::Read {
                path: path.into(),
                maximum,
                external,
            },
            bytes(result),
        )
    }
    pub(super) fn probe(&self, path: &Path, result: &io::Result<bool>) -> io::Result<()> {
        self.insert(Input::Probe(path.into()), exists(result))
    }
}
impl SourceLoader {
    pub(crate) fn observe_inputs(&mut self) {
        self.watch = Some(Observations::default());
    }
    pub(crate) fn read_external(&self, path: &Path, maximum: usize) -> io::Result<Vec<u8>> {
        let result = external_read(path, maximum);
        if let Some(watch) = &self.watch {
            watch.read(path, maximum, true, &result)?;
        }
        result
    }
    /// Record the configuration bytes already read before the profile/root was known.
    pub(crate) fn observe_config(&self, path: &Path, bytes: Vec<u8>) -> io::Result<()> {
        if let Some(watch) = &self.watch {
            watch.read(path, crate::artifact::MAX_MANIFEST_BYTES, true, &Ok(bytes))?;
        }
        Ok(())
    }
    /// Hash only the bounded dependency inputs/probes observed by this compilation.
    /// Missing files and failed reads participate so fixing them can trigger retry.
    pub(crate) fn inputs_changed(&self) -> bool {
        self.watch.as_ref().is_some_and(|watch| {
            watch.conflicted.get()
                || watch.inputs.borrow().iter().any(|(input, previous)| {
                    let current = match input {
                        Input::Read {
                            path,
                            maximum,
                            external,
                        } => bytes(&if *external {
                            external_read(path, *maximum)
                        } else {
                            self.read_bytes_untracked(path, *maximum)
                        }),
                        Input::Probe(path) => exists(&self.exists_untracked(path)),
                    };
                    &current != previous
                })
        })
    }
    /// Baseline a failed attempt once, preventing repeated compilation of the
    /// same invalid input. Its failed/missing reads remain watched for recovery.
    pub(crate) fn acknowledge_inputs(&self) {
        if let Some(watch) = &self.watch {
            for (input, state) in watch.inputs.borrow_mut().iter_mut() {
                *state = match input {
                    Input::Read {
                        path,
                        maximum,
                        external,
                    } => bytes(&if *external {
                        external_read(path, *maximum)
                    } else {
                        self.read_bytes_untracked(path, *maximum)
                    }),
                    Input::Probe(path) => exists(&self.exists_untracked(path)),
                };
            }
            watch.conflicted.set(false);
        }
    }
}

#[cfg(test)]
mod tests;
