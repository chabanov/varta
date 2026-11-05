//! `varta-shell` arguments, parsed without a CLI framework (the shell is a
//! distribution binary and does not depend on `clap`).
use std::ffi::OsString;
use std::path::PathBuf;

/// The `--help` text.
pub const USAGE: &str = "Varta native shell\n\nUsage:\n  varta-shell ARTIFACT [--policy PATH] [--data-dir DIR] [--font PATH FAMILY] [--renderer cpu|gpu|auto]\n  varta-shell ARTIFACT --revoke CAPABILITY [--data-dir DIR]\n  varta-shell --help | --version\n\nARTIFACT is a packaged checked-f64-utf16-v38 view application written by\n'varta build': app.wasm beside app.manifest.json (and app.resources when it\nships fonts), or a fixed-footer container. Its manifest is the package\nrequest; nothing is ambient. Text uses the artifact's fonts.\n\nOptions:\n  --policy PATH      host ceiling (the varta-runtime host policy format); a\n                     resource must satisfy the request and the ceiling, and an\n                     empty ceiling denies everything\n  --data-dir DIR     host directory for application storage; each artifact\n                     hash gets its own directory (default: the platform's\n                     per-user application data directory)\n  --font PATH FAMILY explicit layout and render font for an artifact that\n                     ships no fonts (refused for one that does)\n  --renderer MODE    cpu (default), gpu or auto. gpu needs a build with the\n                     gpu feature and fails when the GPU does not initialize\n                     (a GPU lost later falls back to the CPU); auto uses the\n                     GPU when it initializes, otherwise the CPU\n  --revoke CAPABILITY store a denial of CAPABILITY for ARTIFACT and exit; a\n                     running shell of that artifact denies new operations of\n                     it, and later starts keep it denied\n\nPermissions: a required permission without a stored decision is asked once,\nin a separate host window, before the application starts; refusing it stops\nthe start. An optional permission is asked only when the application\nrequests it during a user gesture. Allow and Deny are stored per artifact\nhash under the storage directory, with an audit log of every decision.\nGranted copy_text and external links of a user gesture use the system\nclipboard and browser.\n";

/// One `varta-shell` invocation.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    Run(ShellArgs),
    /// Store a denial of a capability for an artifact.
    Revoke(RevokeArgs),
    Help,
    Version,
}

/// `--revoke`: which capability of which artifact, in which storage.
#[derive(Debug, PartialEq, Eq)]
pub struct RevokeArgs {
    pub artifact: PathBuf,
    pub capability: String,
    pub data_dir: Option<PathBuf>,
}

/// What to run and with which host inputs.
#[derive(Debug, PartialEq, Eq)]
pub struct ShellArgs {
    pub artifact: PathBuf,
    pub policy: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub font: Option<(PathBuf, String)>,
    pub renderer: Renderer,
}

/// The requested presentation backend.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Renderer {
    #[default]
    Cpu,
    Gpu,
    /// The GPU when it initializes, otherwise the CPU.
    Auto,
}

impl Renderer {
    /// Every renderer with its `--renderer` name.
    const NAMES: [(&'static str, Self); 3] =
        [("cpu", Self::Cpu), ("gpu", Self::Gpu), ("auto", Self::Auto)];

    /// The renderer a `--renderer` value names.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, renderer)| *renderer)
    }

    /// Whether the window should try the GPU backend first.
    #[must_use]
    pub fn prefers_gpu(self) -> bool {
        matches!(self, Self::Gpu | Self::Auto)
    }
}

/// Why the arguments were refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ArgsError {
    #[error("missing packaged artifact; use --help")]
    MissingArtifact,
    #[error("{0} needs a value; use --help")]
    MissingValue(&'static str),
    #[error("{0} is repeated; use --help")]
    Repeated(&'static str),
    #[error("unknown argument {0:?}; use --help")]
    Unknown(OsString),
    #[error("{0} must be UTF-8")]
    NotUtf8(&'static str),
    #[error("unknown renderer {0:?}; use cpu, gpu or auto")]
    Renderer(String),
    #[error("{0} takes only ARTIFACT and --data-dir; use --help")]
    Exclusive(&'static str),
}

const POLICY: &str = "--policy";
const DATA_DIR: &str = "--data-dir";
const FONT: &str = "--font";
const RENDERER: &str = "--renderer";
const REVOKE: &str = "--revoke";
const HELP: [&str; 2] = ["--help", "-h"];
const VERSION: [&str; 2] = ["--version", "-V"];
const OPTION_PREFIX: &str = "-";

/// Parse the arguments after the program name.
pub fn parse(values: Vec<OsString>) -> Result<Command, ArgsError> {
    if let [only] = values.as_slice() {
        match only.to_str() {
            Some(flag) if HELP.contains(&flag) => return Ok(Command::Help),
            Some(flag) if VERSION.contains(&flag) => return Ok(Command::Version),
            _ => {}
        }
    }
    let mut values = values.into_iter();
    let mut artifact = None;
    let mut policy = None;
    let mut data_dir = None;
    let mut font = None;
    let mut renderer = None;
    let mut revoke = None;
    while let Some(value) = values.next() {
        match value.to_str() {
            Some(POLICY) => set(&mut policy, POLICY, path(&mut values, POLICY)?)?,
            Some(DATA_DIR) => set(&mut data_dir, DATA_DIR, path(&mut values, DATA_DIR)?)?,
            Some(FONT) => {
                let file = path(&mut values, FONT)?;
                let family = values
                    .next()
                    .ok_or(ArgsError::MissingValue(FONT))?
                    .into_string()
                    .map_err(|_| ArgsError::NotUtf8("the font family"))?;
                set(&mut font, FONT, (file, family))?;
            }
            Some(RENDERER) => {
                let mode = values
                    .next()
                    .ok_or(ArgsError::MissingValue(RENDERER))?
                    .into_string()
                    .map_err(|_| ArgsError::NotUtf8(RENDERER))?;
                let selected = Renderer::from_name(&mode).ok_or(ArgsError::Renderer(mode))?;
                set(&mut renderer, RENDERER, selected)?;
            }
            Some(REVOKE) => {
                let capability = values
                    .next()
                    .ok_or(ArgsError::MissingValue(REVOKE))?
                    .into_string()
                    .map_err(|_| ArgsError::NotUtf8(REVOKE))?;
                set(&mut revoke, REVOKE, capability)?;
            }
            Some(other) if other.starts_with(OPTION_PREFIX) => {
                return Err(ArgsError::Unknown(value));
            }
            _ if artifact.is_none() => artifact = Some(PathBuf::from(value)),
            _ => return Err(ArgsError::Unknown(value)),
        }
    }
    let artifact = artifact.ok_or(ArgsError::MissingArtifact)?;
    if let Some(capability) = revoke {
        if policy.is_some() || font.is_some() || renderer.is_some() {
            return Err(ArgsError::Exclusive(REVOKE));
        }
        return Ok(Command::Revoke(RevokeArgs {
            artifact,
            capability,
            data_dir,
        }));
    }
    Ok(Command::Run(ShellArgs {
        artifact,
        policy,
        data_dir,
        font,
        renderer: renderer.unwrap_or_default(),
    }))
}

fn path(
    values: &mut impl Iterator<Item = OsString>,
    option: &'static str,
) -> Result<PathBuf, ArgsError> {
    values
        .next()
        .map(PathBuf::from)
        .ok_or(ArgsError::MissingValue(option))
}

fn set<T>(slot: &mut Option<T>, option: &'static str, value: T) -> Result<(), ArgsError> {
    if slot.replace(value).is_some() {
        return Err(ArgsError::Repeated(option));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(values: &[&str]) -> Result<Command, ArgsError> {
        parse(values.iter().map(OsString::from).collect())
    }

    #[test]
    fn parses_an_artifact_with_every_host_input() {
        assert_eq!(
            parse_all(&[
                "app.wasm",
                "--policy",
                "ceiling.json",
                "--data-dir",
                "/data",
                "--font",
                "font.ttf",
                "Fixture Sans",
                "--renderer",
                "auto",
            ]),
            Ok(Command::Run(ShellArgs {
                artifact: "app.wasm".into(),
                policy: Some("ceiling.json".into()),
                data_dir: Some("/data".into()),
                font: Some(("font.ttf".into(), "Fixture Sans".into())),
                renderer: Renderer::Auto,
            }))
        );
        assert_eq!(
            parse_all(&["app.wasm"]),
            Ok(Command::Run(ShellArgs {
                artifact: "app.wasm".into(),
                policy: None,
                data_dir: None,
                font: None,
                renderer: Renderer::Cpu,
            }))
        );
        assert_eq!(parse_all(&["--help"]), Ok(Command::Help));
        assert_eq!(
            parse_all(&["app.wasm", "--revoke", "net.request", "--data-dir", "/data"]),
            Ok(Command::Revoke(RevokeArgs {
                artifact: "app.wasm".into(),
                capability: "net.request".into(),
                data_dir: Some("/data".into()),
            }))
        );
        assert_eq!(parse_all(&["-V"]), Ok(Command::Version));
    }

    #[test]
    fn refuses_missing_repeated_unknown_and_misplaced_arguments() {
        assert_eq!(parse_all(&[]), Err(ArgsError::MissingArtifact));
        assert_eq!(
            parse_all(&["--policy", "p.json"]),
            Err(ArgsError::MissingArtifact)
        );
        assert_eq!(
            parse_all(&["app.wasm", "--policy"]),
            Err(ArgsError::MissingValue(POLICY))
        );
        assert_eq!(
            parse_all(&["app.wasm", "--font", "font.ttf"]),
            Err(ArgsError::MissingValue(FONT))
        );
        assert_eq!(
            parse_all(&["app.wasm", "--policy", "a", "--policy", "b"]),
            Err(ArgsError::Repeated(POLICY))
        );
        assert_eq!(
            parse_all(&["app.wasm", "other.wasm"]),
            Err(ArgsError::Unknown("other.wasm".into()))
        );
        assert_eq!(
            parse_all(&["app.wasm", "--decisions", "d.json"]),
            Err(ArgsError::Unknown("--decisions".into()))
        );
        assert_eq!(
            parse_all(&["app.wasm", "--help"]),
            Err(ArgsError::Unknown("--help".into()))
        );
        assert_eq!(
            parse_all(&["app.wasm", "--renderer", "vulkan"]),
            Err(ArgsError::Renderer("vulkan".into()))
        );
        assert_eq!(
            parse_all(&["app.wasm", "--revoke"]),
            Err(ArgsError::MissingValue(REVOKE))
        );
        assert_eq!(
            parse_all(&["--revoke", "net.request"]),
            Err(ArgsError::MissingArtifact)
        );
        assert_eq!(
            parse_all(&["app.wasm", "--revoke", "a.b", "--revoke", "c.d"]),
            Err(ArgsError::Repeated(REVOKE))
        );
        assert_eq!(
            parse_all(&["app.wasm", "--revoke", "net.request", "--policy", "p.json"]),
            Err(ArgsError::Exclusive(REVOKE))
        );
    }
}
