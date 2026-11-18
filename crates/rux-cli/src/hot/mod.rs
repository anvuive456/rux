//! The session `rux run` stays attached with: streams the app's logs and
//! reads single-key commands; in debug builds, saving a file hot reloads, `r` hot reloads and
//! `R` hot restarts. `q` detaches (the app keeps running).
//!
//! Each reload rebuilds only the app library, sends it to the app's dev client (see
//! [`server`]), which loads it into the running process (`rux_platform::dev`). Hot reload
//! keeps the app's state; hot restart constructs the app again.

mod keys;
mod logs;
mod server;
mod types;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{EventKind, RecursiveMode, Watcher};

pub use logs::Logs;
pub use server::DevServer;

use crate::app::AppCrate;
use keys::Keys;

/// How long the app has to connect after launch.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Editors often write a file in several steps; wait for them to settle.
const DEBOUNCE: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Reload,
    Restart,
}

enum Input {
    Changed,
    Key(char),
}

/// Whether the session can hot reload.
pub enum HotReload<'a> {
    /// The app's dev client connects to `server`; `build` rebuilds the app library for the
    /// device and returns its path.
    On {
        server: &'a DevServer,
        build: Box<dyn FnMut() -> Result<PathBuf> + 'a>,
    },
    /// Not available, with the reason shown to the user.
    Off(&'static str),
}

/// Runs the session until the user quits. `logs` (the app's log stream) is printed meanwhile
/// and stopped at the end.
pub fn session(app: &AppCrate, hot: HotReload, logs: Option<Logs>) -> Result<()> {
    let (tx, rx) = mpsc::channel();
    let mut hot = match hot {
        HotReload::On { server, build } => {
            println!("→ waiting for the app to connect…");
            server.wait_for_app(CONNECT_TIMEOUT)?;
            let watcher = watch(&app.dir, tx.clone())?;
            Ok((server, build, watcher))
        }
        HotReload::Off(why) => Err(why),
    };
    let keys = Keys::read(tx);
    let help = help(&keys, hot.is_ok());

    match &hot {
        Ok(_) => println!("\n🔥 Hot reload is on: save a file to hot reload.\n{help}\n"),
        Err(why) => println!("\nHot reload is off ({why}).\n{help}\n"),
    }
    let mut types = types::fingerprint(&app.dir);
    // Sources as of the last build, to skip events that don't change anything (editors touching
    // files, events delivered late).
    let mut built_sources = sources_hash(&app.dir);
    for input in &rx {
        let (mode, saved) = match input {
            Input::Changed => {
                // Coalesce the burst of events of one save.
                std::thread::sleep(DEBOUNCE);
                while let Ok(Input::Changed) = rx.try_recv() {}
                (Mode::Reload, true)
            }
            Input::Key('r') => (Mode::Reload, false),
            Input::Key('R') => (Mode::Restart, false),
            Input::Key('q') => break,
            Input::Key('c') => {
                print!("\x1b[2J\x1b[H");
                continue;
            }
            Input::Key('h' | '?') => {
                println!("{help}");
                continue;
            }
            Input::Key(_) => continue,
        };
        let (server, build, _) = match &mut hot {
            Ok(hot) => hot,
            Err(why) => {
                println!("   hot reload is off: {why}");
                continue;
            }
        };
        let sources = sources_hash(&app.dir);
        if saved && sources == built_sources {
            continue;
        }
        built_sources = sources;
        reload(app, server, build, &mut types, mode);
    }
    drop(keys); // restore the terminal
    drop(logs);
    println!("→ detached (the app keeps running)");
    Ok(())
}

/// The list of commands.
fn help(keys: &Keys, hot: bool) -> String {
    let k = |c| keys.hint(c);
    let mut help = String::from("   ");
    if hot {
        help += &format!(
            "{}  hot reload (keep state)   {}  hot restart   ",
            k('r'),
            k('R')
        );
    }
    help += &format!(
        "{}  clear   {}  help   {}  quit (the app keeps running)",
        k('c'),
        k('h'),
        k('q')
    );
    help
}

fn reload(
    app: &AppCrate,
    server: &DevServer,
    build: &mut Box<dyn FnMut() -> Result<PathBuf> + '_>,
    types: &mut types::Fingerprint,
    mode: Mode,
) {
    let started = Instant::now();
    let library = match build() {
        Ok(library) => library,
        Err(e) => {
            println!("✗ build failed ({e:#}): fix the error and save again");
            return;
        }
    };
    let new_types = types::fingerprint(&app.dir);
    let mut keep_state = mode == Mode::Reload;
    if types::native_apis_changed(types, &new_types) {
        println!(
            "   ⚠ a #[rux::host_api] declaration changed: its Kotlin / Swift side is only \
             regenerated by a full build, so quit (q) and `rux run` again"
        );
    }
    if keep_state && new_types != *types {
        println!("   type definitions changed → hot restart");
        keep_state = false;
    }
    *types = new_types;

    let result = std::fs::read(&library)
        .with_context(|| format!("reading {}", library.display()))
        .and_then(|bytes| server.send(if keep_state { "reload" } else { "restart" }, &bytes));
    let ms = started.elapsed().as_millis();
    match result {
        Ok(report) => match report.strip_prefix("ok ") {
            Some(report) => println!("✓ {report} ({ms} ms)"),
            None => println!("✗ {}", report.trim_start_matches("err ")),
        },
        Err(e) => println!("✗ {e:#}: is the app still running?"),
    }
}

/// Hash of the app's sources (`src/**/*.rs`) and manifest contents.
fn sources_hash(app_dir: &Path) -> u64 {
    fn visit(dir: &Path, files: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.is_dir() {
                visit(&path, files);
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
    }
    let mut files = vec![app_dir.join("Cargo.toml")];
    visit(&app_dir.join("src"), &mut files);
    files.sort();
    let mut hasher = DefaultHasher::new();
    for file in files {
        file.hash(&mut hasher);
        std::fs::read(&file).unwrap_or_default().hash(&mut hasher);
    }
    hasher.finish()
}

/// Watches the app's sources and manifest.
fn watch(app_dir: &Path, tx: mpsc::Sender<Input>) -> Result<notify::RecommendedWatcher> {
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        let relevant = matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) && event.paths.iter().any(|p| {
            p.extension().is_some_and(|e| e == "rs")
                || p.file_name().is_some_and(|n| n == "Cargo.toml")
        });
        if relevant {
            let _ = tx.send(Input::Changed);
        }
    })
    .context("watching the app's files")?;
    watcher.watch(&app_dir.join("src"), RecursiveMode::Recursive)?;
    watcher.watch(&app_dir.join("Cargo.toml"), RecursiveMode::NonRecursive)?;
    Ok(watcher)
}
