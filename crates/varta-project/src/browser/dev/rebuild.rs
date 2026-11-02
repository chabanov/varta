//! One serialized off-thread source check/build. Runtime authority is captured
//! at startup; source edits cannot reload or widen host policy/decision inputs.
use super::{BrowserDevOptions, Revision, install_bootstrap, runtime_inputs};
use crate::{
    Artifact, DecisionFile, HostPolicy, ProjectError, Result,
    browser::{BrowserArtifact, resources},
    modules::SourceLoader,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub(super) struct Rebuilder {
    config: PathBuf,
    policy: Option<HostPolicy>,
    decisions: Option<DecisionFile>,
    options: BrowserDevOptions,
    watch: SourceLoader,
    cancelled: Arc<AtomicBool>,
}
impl Rebuilder {
    pub(super) fn new(
        config: &Path,
        policy: Option<&HostPolicy>,
        decisions: Option<&DecisionFile>,
        options: BrowserDevOptions,
        cancelled: Arc<AtomicBool>,
    ) -> Result<(Self, Revision)> {
        let config = std::env::current_dir()?.join(config);
        let policy = policy
            .map(|policy| HostPolicy::from_manifest(policy.manifest().clone()))
            .transpose()?;
        let decisions = decisions.cloned();
        let (result, watch) = prepare(&config, policy.as_ref(), decisions.as_ref(), &options);
        let revision = result?;
        Ok((
            Self {
                config,
                policy,
                decisions,
                options,
                watch: watch.expect("prepared bundle has input observations"),
                cancelled,
            },
            revision,
        ))
    }
    pub(super) fn tick(mut self) -> (Self, Option<Result<Revision>>) {
        if self.cancelled.load(Ordering::Acquire) || !self.watch.inputs_changed() {
            return (self, None);
        }
        // Acknowledge before the attempt, never after: a fix made while a failed
        // build is running must remain visible on the next poll.
        self.watch.acknowledge_inputs();
        if self.cancelled.load(Ordering::Acquire) {
            return (self, None);
        }
        let (result, watch) = prepare(
            &self.config,
            self.policy.as_ref(),
            self.decisions.as_ref(),
            &self.options,
        );
        if let Some(watch) = watch {
            self.watch = watch;
        }
        (self, Some(result))
    }
}

fn prepare(
    config: &Path,
    policy: Option<&HostPolicy>,
    decisions: Option<&DecisionFile>,
    options: &BrowserDevOptions,
) -> (Result<Revision>, Option<SourceLoader>) {
    let (result, loader) = crate::compile_attempt(config, false, true);
    let result = result.and_then(|(config, base, compilation)| {
        let loader = loader.as_ref().expect("compiled program has source loader");
        if options.application && !config.language_profile.turn_budgets() {
            return Err(ProjectError::Usage(
                "browser application mode requires v38".into(),
            ));
        }
        let artifact = Artifact {
            wasm: compilation.wasm,
            manifest: config.manifest(),
        };
        let loaded = resources::load_with_loader(&config, &base, loader)?;
        let prepared = BrowserArtifact::prepare(&artifact, policy)?;
        let mut files = prepared.development_bundle(&loaded)?.into_files();
        install_bootstrap(
            &mut files,
            &runtime_inputs(&artifact, policy, decisions, options),
        )?;
        let revision = Revision::new(files)?;
        if loader.inputs_changed() {
            return Err(ProjectError::Usage(
                "browser development inputs changed during preparation; retry".into(),
            ));
        }
        Ok(revision)
    });
    (result, loader)
}

#[cfg(test)]
mod tests;
