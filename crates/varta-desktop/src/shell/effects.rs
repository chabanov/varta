//! The shell's real platform for the guest's granted effects: `copy_text`
//! writes the system clipboard and an external link activation hands its URL
//! to the system browser. The view reaches this sink only for a trusted user
//! gesture after the permission broker granted the exact operation
//! (`clipboard.write`, or a `ui.open_url` pattern of the request and the
//! ceiling), so every denial happens before any effect here; a failure here
//! reports `refused_by_platform`.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use varta_view::{PlatformEffects, PlatformRefusal};

/// The system browser launcher: an absolute path, never a `PATH` lookup,
/// and the arguments that precede the URL. macOS `open -u` opens the URL
/// with the application that claims its scheme, even when the URL also
/// names a file path.
#[cfg(target_os = "macos")]
const BROWSER_LAUNCHER: Option<(&str, &[&str])> = Some(("/usr/bin/open", &["-u"]));
#[cfg(all(unix, not(target_os = "macos")))]
const BROWSER_LAUNCHER: Option<(&str, &[&str])> = Some(("/usr/bin/xdg-open", &[]));
/// No launcher is implemented for other hosts: every opening is refused.
#[cfg(not(unix))]
const BROWSER_LAUNCHER: Option<(&str, &[&str])> = None;
/// How long a launcher may run before the opening counts as refused (it is
/// then killed). The launcher hands the URL to the browser and exits; it
/// does not wait for the browser.
const LAUNCH_DEADLINE: Duration = Duration::from_secs(5);
/// How often the launcher's exit is polled.
const LAUNCH_POLL: Duration = Duration::from_millis(10);

/// A program that hands a URL to the system browser.
#[derive(Debug, Clone)]
struct Launcher {
    program: PathBuf,
    arguments: Vec<String>,
    deadline: Duration,
}
impl Launcher {
    fn system() -> Option<Self> {
        BROWSER_LAUNCHER.map(|(program, arguments)| Self {
            program: PathBuf::from(program),
            arguments: arguments
                .iter()
                .map(|argument| (*argument).into())
                .collect(),
            deadline: LAUNCH_DEADLINE,
        })
    }

    /// Run the launcher for `url` and wait, bounded, for its exit status.
    fn open(&self, url: &str) -> Result<(), PlatformRefusal> {
        let mut child = Command::new(Path::new(&self.program))
            .args(&self.arguments)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|_| PlatformRefusal)?;
        let until = Instant::now() + self.deadline;
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(_)) => return Err(PlatformRefusal),
                Ok(None) if Instant::now() < until => std::thread::sleep(LAUNCH_POLL),
                Ok(None) | Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(PlatformRefusal);
                }
            }
        }
    }
}

/// An absolute `https:`/`http:` URL without control characters or
/// whitespace: the only input handed to a launcher, so it can never be
/// read as an option or a file path.
fn launchable(url: &str) -> bool {
    varta_common::view::attributes::external(url)
        && !url.chars().any(|ch| ch.is_control() || ch.is_whitespace())
}

/// The system clipboard (created on first use) and the system browser.
pub struct NativeEffects {
    clipboard: Mutex<Option<arboard::Clipboard>>,
    browser: Option<Launcher>,
}
impl std::fmt::Debug for NativeEffects {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeEffects")
            .field("browser", &self.browser)
            .finish_non_exhaustive()
    }
}
impl Default for NativeEffects {
    fn default() -> Self {
        Self {
            clipboard: Mutex::new(None),
            browser: Launcher::system(),
        }
    }
}
impl PlatformEffects for NativeEffects {
    fn write_clipboard(&self, text: &str) -> Result<(), PlatformRefusal> {
        let mut clipboard = self.clipboard.lock().map_err(|_| PlatformRefusal)?;
        if clipboard.is_none() {
            *clipboard = Some(arboard::Clipboard::new().map_err(|_| PlatformRefusal)?);
        }
        clipboard
            .as_mut()
            .ok_or(PlatformRefusal)?
            .set_text(text)
            .map_err(|_| PlatformRefusal)
    }
    fn open_url(&self, url: &str) -> Result<(), PlatformRefusal> {
        if !launchable(url) {
            return Err(PlatformRefusal);
        }
        self.browser.as_ref().ok_or(PlatformRefusal)?.open(url)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    const URL: &str = "https://docs.test/guide";
    /// Programs that exit successfully, fail, or outlive the deadline.
    const SUCCEEDS: &str = "/usr/bin/true";
    const FAILS: &str = "/usr/bin/false";
    const WAITS: &str = "/bin/sleep";
    const WAIT_SECONDS: &str = "30";
    const SHORT_DEADLINE: Duration = Duration::from_millis(100);

    /// Effects whose browser is `program`; the clipboard is never touched.
    fn effects(program: &str, arguments: &[&str], deadline: Duration) -> NativeEffects {
        NativeEffects {
            clipboard: Mutex::new(None),
            browser: Some(Launcher {
                program: program.into(),
                arguments: arguments
                    .iter()
                    .map(|argument| (*argument).into())
                    .collect(),
                deadline,
            }),
        }
    }

    #[test]
    fn the_launcher_exit_status_is_the_outcome() {
        assert_eq!(
            effects(SUCCEEDS, &[], LAUNCH_DEADLINE).open_url(URL),
            Ok(())
        );
        assert_eq!(
            effects(FAILS, &[], LAUNCH_DEADLINE).open_url(URL),
            Err(PlatformRefusal)
        );
        assert_eq!(
            effects("/nonexistent/varta-browser", &[], LAUNCH_DEADLINE).open_url(URL),
            Err(PlatformRefusal)
        );
        // A launcher that outlives its deadline is killed and refused.
        let started = Instant::now();
        assert_eq!(
            effects(WAITS, &[WAIT_SECONDS], SHORT_DEADLINE).open_url(URL),
            Err(PlatformRefusal)
        );
        assert!(started.elapsed() < LAUNCH_DEADLINE);
    }

    #[test]
    fn only_absolute_web_urls_reach_the_launcher() {
        let effects = effects(SUCCEEDS, &[], LAUNCH_DEADLINE);
        for url in [
            "file:///etc/passwd",
            "-a Calculator",
            "/Applications",
            "javascript:alert(1)",
            "https://docs.test/a b",
            "https://docs.test/\nb",
        ] {
            assert_eq!(effects.open_url(url), Err(PlatformRefusal), "{url}");
        }
        assert!(launchable("http://127.0.0.1:8080/path?q=1#top"));
        let none = NativeEffects {
            clipboard: Mutex::new(None),
            browser: None,
        };
        assert_eq!(none.open_url(URL), Err(PlatformRefusal));
    }
}
