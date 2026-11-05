//! The shell's real platform for the guest's granted effects: `copy_text`
//! writes the system clipboard and an external link activation hands its URL
//! to the system browser. The view reaches this sink only for a trusted user
//! gesture after the permission broker granted the exact operation
//! (`clipboard.write`, or a `ui.open_url` pattern of the request and the
//! ceiling), so every denial happens before any effect here; a failure here
//! reports `refused_by_platform`.
//!
//! The browser launcher is started detached and the opening succeeds once it
//! is spawned: view input handling never waits for it, because some
//! launchers (`xdg-open` on some Linux desktops) stay in the foreground until
//! the browser exits, and a real opening must never be reported as refused.
//! A reaper thread collects the launcher's exit; its status is not observed.
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use varta_view::{PlatformEffects, PlatformRefusal};

/// The system browser launcher: an absolute path, never a `PATH` lookup,
/// and the arguments that precede the URL. macOS `open -u` opens the URL
/// with the application that claims its scheme, even when the URL also
/// names a file path. The Linux launcher is not exercised by this crate's
/// tests or acceptance runs.
#[cfg(target_os = "macos")]
const BROWSER_LAUNCHER: Option<(&str, &[&str])> = Some(("/usr/bin/open", &["-u"]));
#[cfg(all(unix, not(target_os = "macos")))]
const BROWSER_LAUNCHER: Option<(&str, &[&str])> = Some(("/usr/bin/xdg-open", &[]));
/// No launcher is implemented for other hosts: every opening is refused.
#[cfg(not(unix))]
const BROWSER_LAUNCHER: Option<(&str, &[&str])> = None;
/// The process group of a detached launcher: a new group led by the
/// launcher itself, so signals sent to the shell's group (a terminal's
/// interrupt) do not reach the browser it starts.
#[cfg(unix)]
const OWN_PROCESS_GROUP: i32 = 0;
/// The name of the thread that collects a launcher's exit.
const REAPER_THREAD: &str = "varta-browser-launcher";

/// A program that hands a URL to the system browser.
#[derive(Debug, Clone)]
struct Launcher {
    program: PathBuf,
    arguments: Vec<String>,
}
impl Launcher {
    fn system() -> Option<Self> {
        BROWSER_LAUNCHER.map(|(program, arguments)| Self {
            program: PathBuf::from(program),
            arguments: arguments
                .iter()
                .map(|argument| (*argument).into())
                .collect(),
        })
    }

    /// Start the launcher for `url`, detached, without waiting for it: the
    /// opening succeeds when the launcher is spawned and is refused only
    /// when it cannot be started.
    fn open(&self, url: &str) -> Result<(), PlatformRefusal> {
        let mut command = Command::new(Path::new(&self.program));
        command
            .args(&self.arguments)
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(OWN_PROCESS_GROUP);
        }
        let child = command.spawn().map_err(|_| PlatformRefusal)?;
        reap(child);
        Ok(())
    }
}

/// Collect a detached launcher's exit on its own thread. If that thread
/// cannot be created the launcher still runs; only its exit status stays
/// uncollected until the shell exits.
fn reap(mut child: Child) {
    // The launcher was started, so the opening already succeeded; the
    // thread only waits, and failing to create it changes nothing else.
    let _ = std::thread::Builder::new()
        .name(REAPER_THREAD.into())
        .spawn(move || {
            let _ = child.wait();
        });
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
    use std::time::{Duration, Instant};

    const URL: &str = "https://docs.test/guide";
    /// Programs that exit successfully, fail, or stay in the foreground.
    const SUCCEEDS: &str = "/usr/bin/true";
    const FAILS: &str = "/usr/bin/false";
    const WAITS: &str = "/bin/sleep";
    /// How long the foreground launcher stays; the opening must return well
    /// before it exits.
    const WAIT_SECONDS: &str = "2";
    const FOREGROUND: Duration = Duration::from_secs(2);

    /// Effects whose browser is `program`; the clipboard is never touched.
    fn effects(program: &str, arguments: &[&str]) -> NativeEffects {
        NativeEffects {
            clipboard: Mutex::new(None),
            browser: Some(Launcher {
                program: program.into(),
                arguments: arguments
                    .iter()
                    .map(|argument| (*argument).into())
                    .collect(),
            }),
        }
    }

    #[test]
    fn a_spawned_launcher_is_an_opening_and_is_never_awaited() {
        assert_eq!(effects(SUCCEEDS, &[]).open_url(URL), Ok(()));
        // The launcher's exit status is not observed once it was started.
        assert_eq!(effects(FAILS, &[]).open_url(URL), Ok(()));
        // Only a launcher that cannot be started refuses the opening.
        assert_eq!(
            effects("/nonexistent/varta-browser", &[]).open_url(URL),
            Err(PlatformRefusal)
        );
        // A launcher that stays in the foreground (as `xdg-open` can) is an
        // opening, reported without waiting for it.
        let started = Instant::now();
        assert_eq!(effects(WAITS, &[WAIT_SECONDS]).open_url(URL), Ok(()));
        assert!(started.elapsed() < FOREGROUND);
    }

    #[test]
    fn only_absolute_web_urls_reach_the_launcher() {
        let effects = effects(SUCCEEDS, &[]);
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
