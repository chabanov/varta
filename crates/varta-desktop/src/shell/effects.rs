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
//! No thread waits for a launcher: the shell keeps at most
//! [`MAX_RUNNING_LAUNCHERS`] started launchers, collects the exit of those
//! that finished before each new opening (their status is not observed), and
//! refuses an opening while that many still run.
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
/// The most launchers that may still run at once. A launcher that stays in
/// the foreground holds its place until it exits, so repeated link
/// activations cannot accumulate processes without bound.
const MAX_RUNNING_LAUNCHERS: usize = 8;

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
    fn open(&self, url: &str) -> Result<Child, PlatformRefusal> {
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
        command.spawn().map_err(|_| PlatformRefusal)
    }
}

/// Started launchers whose exit has not been collected yet.
#[derive(Debug, Default)]
struct Launched {
    running: Vec<Child>,
}
impl Launched {
    /// Collect the exit of every launcher that finished, without waiting,
    /// and return how many still run. A launcher whose state cannot be read
    /// keeps its place, so the bound holds.
    fn collect(&mut self) -> usize {
        self.running
            .retain_mut(|child| !matches!(child.try_wait(), Ok(Some(_))));
        self.running.len()
    }

    /// Start `launcher` for `url` unless [`MAX_RUNNING_LAUNCHERS`] still run.
    fn open(&mut self, launcher: &Launcher, url: &str) -> Result<(), PlatformRefusal> {
        if self.collect() >= MAX_RUNNING_LAUNCHERS {
            return Err(PlatformRefusal);
        }
        self.running.push(launcher.open(url)?);
        Ok(())
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
    launched: Mutex<Launched>,
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
            launched: Mutex::default(),
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
        let launcher = self.browser.as_ref().ok_or(PlatformRefusal)?;
        self.launched
            .lock()
            .map_err(|_| PlatformRefusal)?
            .open(launcher, url)
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
    /// A shell that sleeps and ignores the URL it receives as `$1` (`sleep`
    /// itself would exit at once on the extra operand).
    const WAITS: &str = "/bin/sh";
    /// The foreground launcher's arguments: it stays for two seconds, and
    /// the opening must return well before it exits.
    const FOREGROUND_LAUNCHER: &[&str] = &["-c", "sleep 2", "launcher"];
    const FOREGROUND: Duration = Duration::from_secs(2);
    /// How often a test checks whether finished launchers were collected.
    const POLL: Duration = Duration::from_millis(10);

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
            launched: Mutex::default(),
        }
    }

    /// Stop the launchers this test started (only their own PIDs).
    fn stop(effects: &NativeEffects) {
        let mut launched = effects.launched.lock().unwrap();
        for child in &mut launched.running {
            let _ = child.kill();
            let _ = child.wait();
        }
        launched.running.clear();
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
        let waiting = effects(WAITS, FOREGROUND_LAUNCHER);
        assert_eq!(waiting.open_url(URL), Ok(()));
        assert!(started.elapsed() < FOREGROUND);
        stop(&waiting);
    }

    #[test]
    fn launchers_that_stay_in_the_foreground_are_bounded_and_finished_ones_free_their_place() {
        // While the bound's worth of launchers runs, a further opening is
        // refused before anything is started.
        let waiting = effects(WAITS, FOREGROUND_LAUNCHER);
        for _ in 0..MAX_RUNNING_LAUNCHERS {
            assert_eq!(waiting.open_url(URL), Ok(()));
        }
        assert_eq!(waiting.open_url(URL), Err(PlatformRefusal));
        assert_eq!(
            waiting.launched.lock().unwrap().running.len(),
            MAX_RUNNING_LAUNCHERS
        );
        stop(&waiting);
        assert_eq!(waiting.open_url(URL), Ok(()));
        stop(&waiting);
        // Launchers that exit are collected before the next opening, so
        // openings beyond the bound keep succeeding and no exit is left
        // uncollected beyond it.
        let finishing = effects(SUCCEEDS, &[]);
        for _ in 0..MAX_RUNNING_LAUNCHERS {
            assert_eq!(finishing.open_url(URL), Ok(()));
        }
        let deadline = Instant::now() + FOREGROUND;
        while finishing.launched.lock().unwrap().collect() != 0 {
            assert!(
                Instant::now() < deadline,
                "finished launchers were not collected"
            );
            std::thread::sleep(POLL);
        }
        for _ in 0..MAX_RUNNING_LAUNCHERS {
            assert_eq!(finishing.open_url(URL), Ok(()));
        }
        stop(&finishing);
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
            launched: Mutex::default(),
        };
        assert_eq!(none.open_url(URL), Err(PlatformRefusal));
    }
}
