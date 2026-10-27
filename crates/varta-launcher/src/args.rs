use crate::Error;
use std::{ffi::OsString, path::PathBuf};

pub enum Mode {
    Run(Option<PathBuf>),
    Invoke {
        path: Option<PathBuf>,
        name: String,
        params: String,
    },
    Inspect(Option<PathBuf>),
    Validate(PathBuf),
}
pub struct Args {
    pub mode: Mode,
    pub policy: Option<PathBuf>,
    pub json: bool,
}
impl Args {
    pub fn parse(values: Vec<OsString>) -> Result<Self, Error> {
        let mut values = values.into_iter().peekable();
        let first = values.peek().and_then(|v| v.to_str());
        let mut mode = match first {
            Some("run") => {
                values.next();
                Mode::Run(Some(PathBuf::from(next(&mut values)?)))
            }
            Some("invoke") => {
                values.next();
                let path = Some(PathBuf::from(next(&mut values)?));
                let name = text(next(&mut values)?)?;
                Mode::Invoke {
                    path,
                    name,
                    params: "{}".into(),
                }
            }
            Some("--invoke") => {
                values.next();
                let name = text(next(&mut values)?)?;
                Mode::Invoke {
                    path: None,
                    name,
                    params: "{}".into(),
                }
            }
            Some("inspect") => {
                values.next();
                Mode::Inspect(Some(PathBuf::from(next(&mut values)?)))
            }
            Some("--inspect") => {
                values.next();
                Mode::Inspect(None)
            }
            Some(varta_common::launcher::VALIDATE_ARGUMENT) => {
                values.next();
                Mode::Validate(PathBuf::from(next(&mut values)?))
            }
            _ => Mode::Run(None),
        };
        let mut policy = None;
        let mut json = false;
        let mut parameters = false;
        while let Some(option) = values.next() {
            match option.to_str() {
                Some("--policy") if policy.is_none() && !matches!(mode, Mode::Validate(_)) => {
                    policy = Some(PathBuf::from(next(&mut values)?))
                }
                Some("--json")
                    if !json && matches!(mode, Mode::Invoke { .. } | Mode::Inspect(_)) =>
                {
                    json = true
                }
                Some("--params") if !parameters => {
                    let Mode::Invoke { params, .. } = &mut mode else {
                        return Err(Error::Usage("--params requires invocation"));
                    };
                    *params = text(next(&mut values)?)?;
                    parameters = true;
                }
                _ => {
                    return Err(Error::Usage(
                        "unknown, repeated or misplaced runtime argument; use --help",
                    ));
                }
            }
        }
        Ok(Self { mode, policy, json })
    }
}
fn next(values: &mut impl Iterator<Item = OsString>) -> Result<OsString, Error> {
    values
        .next()
        .ok_or(Error::Usage("missing runtime argument value"))
}
fn text(value: OsString) -> Result<String, Error> {
    value
        .into_string()
        .map_err(|_| Error::Usage("tool names and JSON must be UTF-8"))
}
