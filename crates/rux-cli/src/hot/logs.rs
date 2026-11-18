//! Streams the running app's logs (a `adb logcat` / `simctl launch --console-pty` / `devicectl
//! … --console` process) to the terminal while `rux run` is attached.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};

/// A running log process; stopped when dropped (the app keeps running).
pub struct Logs {
    child: Child,
    stopping: Arc<AtomicBool>,
}

impl Logs {
    /// Spawns `cmd` and prints its output line by line. `ended` is printed if the process exits
    /// on its own (e.g. the app exited, for commands attached to the app's console).
    pub fn spawn(cmd: &mut Command, ended: &'static str) -> Result<Self> {
        cmd.stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Own process group: a Ctrl-C in the terminal must not reach it, since console-attached
        // commands forward signals to the app.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(cmd, 0);
        let mut child = cmd
            .spawn()
            .with_context(|| format!("spawning {:?}", cmd.get_program()))?;
        let stopping = Arc::new(AtomicBool::new(false));
        if let Some(stdout) = child.stdout.take() {
            forward(stdout, stopping.clone(), Some(ended));
        }
        if let Some(stderr) = child.stderr.take() {
            forward(stderr, stopping.clone(), None);
        }
        Ok(Self { child, stopping })
    }
}

impl Drop for Logs {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        // SIGKILL can't be forwarded to the app, so the app keeps running.
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn forward(
    stream: impl Read + Send + 'static,
    stopping: Arc<AtomicBool>,
    ended: Option<&'static str>,
) {
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stream);
        let mut line = Vec::new();
        while matches!(reader.read_until(b'\n', &mut line), Ok(n) if n > 0) {
            if !stopping.load(Ordering::SeqCst) {
                let text = String::from_utf8_lossy(&line);
                let mut out = std::io::stdout().lock();
                let _ = writeln!(out, "{}", text.trim_end_matches(['\r', '\n']));
                let _ = out.flush();
            }
            line.clear();
        }
        if let Some(ended) = ended
            && !stopping.load(Ordering::SeqCst)
        {
            println!("→ {ended}");
        }
    });
}

#[cfg(all(test, unix))]
mod tests {
    use std::process::Command;
    use std::time::{Duration, Instant};

    use super::Logs;

    #[test]
    fn stops_the_process_when_dropped() {
        let logs = Logs::spawn(Command::new("sleep").arg("30"), "ended").unwrap();
        let started = Instant::now();
        drop(logs);
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
