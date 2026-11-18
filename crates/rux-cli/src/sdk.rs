//! The rux SDK: the `rux` crate, the crates it depends on and the Android / iOS runtimes,
//! embedded in the CLI (see `build.rs`), so the CLI and the library are always one version.
//!
//! `rux setup` installs it in `~/.rux/sdk/<version>` (`$RUX_HOME/sdk/<version>`) and links it
//! into the app as `.rux/sdk` (gitignored), which the app depends on:
//!
//! ```toml
//! rux = { version = "=0.2.0", path = ".rux/sdk" }
//! ```
//!
//! Like Flutter's SDK, nothing is published to a registry; unlike it, each app states the
//! version it needs, and a CLI of another version refuses to build it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/sdk.rs"));
}

/// The SDK (and CLI) version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where an app links the SDK, relative to the app directory (as written in Cargo.toml).
pub const APP_LINK: &str = ".rux/sdk";

/// The SDK link of the app at `app_dir`, with native separators (`cmd` takes `/` for a switch).
fn link_path(app_dir: &Path) -> PathBuf {
    APP_LINK
        .split('/')
        .fold(app_dir.to_path_buf(), |p, c| p.join(c))
}

/// File in an installed SDK holding the hash of its contents.
const MARKER: &str = ".rux-sdk";

const RELEASES: &str = "https://github.com/anvuive456/rux/releases";

/// `$RUX_HOME`, else `~/.rux`.
pub fn home() -> Result<PathBuf> {
    if let Some(home) = std::env::var_os("RUX_HOME").filter(|h| !h.is_empty()) {
        // Absolute, with native separators (e.g. `C:/x` set from Git Bash).
        return Ok(std::path::absolute(PathBuf::from(home))?);
    }
    let user = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
        .filter(|h| !h.is_empty())
        .context("cannot find your home directory: set RUX_HOME")?;
    Ok(PathBuf::from(user).join(".rux"))
}

/// `rux setup`: installs the SDK, and links it into the app at `dir` if there is one.
/// Also installs the CLI itself in `~/.rux/bin` (on PATH if `modify_path`), and the missing
/// Rust tools (asking first unless `yes`).
pub fn setup(dir: &Path, modify_path: bool, yes: bool) -> Result<()> {
    let dir = &std::path::absolute(dir)?;
    let home = home()?;
    crate::install::install_cli(&home, modify_path)?;
    let (sdk, fresh) = install(&home)?;
    if !fresh {
        println!("✓ rux SDK {VERSION} is installed in {}", sdk.display());
    }
    if dir.join("Cargo.toml").is_file() {
        if required_version(dir)?.is_some() {
            check_app(dir)?;
            link(dir, &sdk)?;
        } else {
            println!(
                "  ({} doesn't use the SDK: no `rux = {{ …, path = \"{APP_LINK}\" }}` dependency)",
                dir.display()
            );
        }
    }
    crate::toolchain::setup(yes)?;
    Ok(())
}

/// Before building an app: if it uses the SDK, checks its version and (re)links the SDK when
/// missing or outdated.
pub fn ensure(app_dir: &Path) -> Result<()> {
    if required_version(app_dir)?.is_none() {
        return Ok(());
    }
    check_app(app_dir)?;
    if !is_current(&link_path(app_dir)) {
        let (sdk, _) = install(&home()?)?;
        link(app_dir, &sdk)?;
    }
    Ok(())
}

/// Installs the SDK under `home` unless it's already there; returns its directory and whether
/// it was (re)installed.
pub fn install(home: &Path) -> Result<(PathBuf, bool)> {
    let dir = home.join("sdk").join(VERSION);
    if is_current(&dir) {
        return Ok((dir, false));
    }
    // Written aside, then moved in place: an interrupted install leaves no half SDK.
    let tmp = home
        .join("sdk")
        .join(format!(".tmp-{}", std::process::id()));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    for (rel, contents) in embedded::FILES {
        let path = tmp.join(rel);
        std::fs::create_dir_all(path.parent().context("SDK file path")?)?;
        std::fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
    }
    std::fs::write(tmp.join(MARKER), embedded::HASH)?;
    if dir.exists() {
        // Another build of the same version (built from source): replace it.
        std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))?;
    }
    std::fs::rename(&tmp, &dir).with_context(|| format!("installing {}", dir.display()))?;
    println!("✓ installed the rux SDK {VERSION} in {}", dir.display());
    Ok((dir, true))
}

/// Whether `dir` holds this CLI's SDK.
fn is_current(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join(MARKER)).is_ok_and(|h| h == embedded::HASH)
}

/// Links `<app>/.rux/sdk` to `sdk` (replacing an older link).
pub fn link(app_dir: &Path, sdk: &Path) -> Result<()> {
    let link = link_path(app_dir);
    std::fs::create_dir_all(link.parent().context("link path")?)?;
    match std::fs::symlink_metadata(&link) {
        Ok(m) if m.file_type().is_symlink() => remove_link(&link)?,
        Ok(_) => bail!(
            "{} exists and isn't a link to the rux SDK: remove it and run `rux setup`",
            link.display()
        ),
        Err(_) => {}
    }
    make_link(sdk, &link)
        .with_context(|| format!("linking {} to {}", link.display(), sdk.display()))?;
    println!("→ linked {} to the rux SDK {VERSION}", link.display());
    Ok(())
}

#[cfg(unix)]
fn make_link(target: &Path, link: &Path) -> Result<()> {
    Ok(std::os::unix::fs::symlink(target, link)?)
}

#[cfg(unix)]
fn remove_link(link: &Path) -> Result<()> {
    Ok(std::fs::remove_file(link)?)
}

/// A directory symlink needs Developer Mode or admin rights; a junction needs neither.
#[cfg(windows)]
fn make_link(target: &Path, link: &Path) -> Result<()> {
    // `mklink` takes `/` for a switch: native separators only.
    let (target, link) = (std::path::absolute(target)?, std::path::absolute(link)?);
    if std::os::windows::fs::symlink_dir(&target, &link).is_ok() {
        return Ok(());
    }
    crate::util::output(
        std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&link)
            .arg(&target),
    )?;
    Ok(())
}

#[cfg(windows)]
fn remove_link(link: &Path) -> Result<()> {
    Ok(std::fs::remove_dir(link)?)
}

/// The `rux` version the app at `dir` requires, if it uses the SDK
/// (`rux = { version = "=X.Y.Z", path = ".rux/sdk" }`).
fn required_version(dir: &Path) -> Result<Option<String>> {
    let path = dir.join("Cargo.toml");
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let manifest: toml::Table =
        toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
    let Some(rux) = manifest
        .get("dependencies")
        .and_then(|d| d.get("rux"))
        .and_then(|r| r.as_table())
    else {
        return Ok(None);
    };
    if rux.get("path").and_then(|p| p.as_str()) != Some(APP_LINK) {
        return Ok(None);
    }
    let version = rux
        .get("version")
        .and_then(|v| v.as_str())
        .with_context(|| format!("`rux` in {} has no `version`", path.display()))?;
    Ok(Some(version.trim_start_matches('=').trim().to_owned()))
}

fn check_app(dir: &Path) -> Result<()> {
    if let Some(required) = required_version(dir)? {
        check_version(&required)?;
    }
    Ok(())
}

/// One version for everything: the app's `rux` must be this CLI's.
fn check_version(required: &str) -> Result<()> {
    if required != VERSION {
        bail!(
            "this app uses rux {required}, but this `rux` is {VERSION}.\n  \
             Either install rux {required} ({RELEASES}/tag/v{required}), or move the app to \
             {VERSION}: set `version = \"={VERSION}\"` for `rux` in Cargo.toml."
        );
    }
    Ok(())
}

/// For `rux doctor`.
pub fn status() -> String {
    match home() {
        Ok(home) if is_current(&home.join("sdk").join(VERSION)) => {
            format!("installed in {}", home.join("sdk").join(VERSION).display())
        }
        _ => "not installed".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rux-sdk-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn embeds_the_sdk() {
        let has = |p: &str| embedded::FILES.iter().any(|(f, _)| *f == p);
        assert!(has("Cargo.toml") && has("src/lib.rs"));
        assert!(has("crates/rux-core/Cargo.toml"));
        assert!(has("platforms/android/rux-runtime/build.gradle.kts"));
        assert!(has("platforms/ios/Package.swift"));
        assert!(
            !embedded::FILES
                .iter()
                .any(|(f, _)| f.starts_with("crates/rux-cli/"))
        );
    }

    #[test]
    fn installs_and_links() {
        let home = temp("home");
        let (sdk, fresh) = install(&home).unwrap();
        assert!(fresh && sdk.join("platforms/ios/Package.swift").is_file());
        assert!(!install(&home).unwrap().1, "already installed");

        let app = temp("app");
        link(&app, &sdk).unwrap();
        link(&app, &sdk).unwrap(); // replaces its own link
        assert!(is_current(&link_path(&app)));

        remove_link(&link_path(&app)).unwrap();
        assert!(
            sdk.join("Cargo.toml").is_file(),
            "removing the link keeps the SDK"
        );
        std::fs::remove_dir_all(&app).unwrap();
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn reads_and_checks_the_required_version() {
        let app = temp("version");
        let write = |dep: &str| {
            std::fs::write(
                app.join("Cargo.toml"),
                format!("[package]\nname = \"a\"\n[dependencies]\nrux = {dep}\n"),
            )
            .unwrap()
        };
        write(&format!(
            "{{ version = \"={VERSION}\", path = \"{APP_LINK}\" }}"
        ));
        assert_eq!(required_version(&app).unwrap().as_deref(), Some(VERSION));
        assert!(check_app(&app).is_ok());

        write(&format!(
            "{{ version = \"=0.0.1\", path = \"{APP_LINK}\" }}"
        ));
        assert!(
            check_app(&app)
                .unwrap_err()
                .to_string()
                .contains("uses rux 0.0.1")
        );

        write("{ workspace = true }");
        assert_eq!(required_version(&app).unwrap(), None);
        std::fs::remove_dir_all(&app).unwrap();
    }
}
