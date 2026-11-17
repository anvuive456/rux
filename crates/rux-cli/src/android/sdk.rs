//! Locating the Android SDK and NDK — and installing the NDK when it's missing, like the
//! Android Gradle Plugin does for Flutter's `ndkVersion` — so users don't have to install it
//! or set environment variables themselves.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::util::run;

/// NDK installed when none is (override with `android-ndk-version` in
/// `[package.metadata.rux]`). Latest stable as of 2026-11.
pub const DEFAULT_NDK_VERSION: &str = "30.0.16248370";

/// How to install the NDK by hand.
pub const NDK_HINT: &str = "install it in Android Studio: Settings > Languages & Frameworks > \
     Android SDK > SDK Tools > NDK (Side by side), or set ANDROID_NDK_HOME";

pub const SDK_HINT: &str = "Android SDK not found: install Android Studio, or set ANDROID_HOME";

/// Candidate SDK directories that exist: `ANDROID_HOME`, `ANDROID_SDK_ROOT`, then Android
/// Studio's default location.
pub fn sdk_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut dirs: Vec<PathBuf> = Vec::new();
    let candidates = ["ANDROID_HOME", "ANDROID_SDK_ROOT"]
        .iter()
        .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
        .chain(home.iter().map(|h| h.join("Library/Android/sdk"))) // macOS
        .chain(home.iter().map(|h| h.join("Android/Sdk"))); // Linux
    for dir in candidates {
        if dir.is_dir() && !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs
}

/// The NDK to build with, installing it into the SDK if needed:
/// 1. `ANDROID_NDK_HOME` / `ANDROID_NDK_ROOT` if set;
/// 2. with `version` (from Cargo.toml): exactly that version, installed if missing;
/// 3. otherwise the newest installed NDK, else [`DEFAULT_NDK_VERSION`], installed.
pub fn ensure_ndk(version: Option<&str>) -> Result<PathBuf> {
    if let Some(ndk) = ndk_from_env() {
        return Ok(ndk);
    }
    if version.is_none()
        && let Some(ndk) = find_ndk()
    {
        return Ok(ndk);
    }
    let sdk = sdk_dirs().into_iter().next().context(SDK_HINT)?;
    let version = version.unwrap_or(DEFAULT_NDK_VERSION);
    let ndk = sdk.join("ndk").join(version);
    if !is_ndk(&ndk) {
        install_ndk(&sdk, version)?;
        if !is_ndk(&ndk) {
            bail!(
                "NDK {version} was installed but {} is missing",
                ndk.display()
            );
        }
    }
    Ok(ndk)
}

/// The NDK: `ANDROID_NDK_HOME` / `ANDROID_NDK_ROOT` if set, else the newest NDK installed in
/// the SDK (`<sdk>/ndk/<version>`, or the legacy `<sdk>/ndk-bundle`).
pub fn find_ndk() -> Option<PathBuf> {
    if let Some(ndk) = ndk_from_env() {
        return Some(ndk);
    }
    sdk_dirs().into_iter().find_map(|sdk| {
        newest_ndk(&sdk.join("ndk")).or_else(|| {
            let bundle = sdk.join("ndk-bundle");
            is_ndk(&bundle).then_some(bundle)
        })
    })
}

fn ndk_from_env() -> Option<PathBuf> {
    ["ANDROID_NDK_HOME", "ANDROID_NDK_ROOT"]
        .iter()
        .filter_map(|v| std::env::var_os(v).map(PathBuf::from))
        .find(|dir| is_ndk(dir))
}

/// Installs `ndk/<version>` with the SDK's command-line tools: the Android CLI
/// (`android sdk install`) if present, else the older `sdkmanager`. Runs interactively, so
/// any license prompt is shown to the user.
fn install_ndk(sdk: &Path, version: &str) -> Result<()> {
    println!(
        "→ Android NDK {version} is not installed; installing it into {} (one-time, ~1 GB)",
        sdk.display()
    );
    let bin = sdk.join("cmdline-tools").join("latest").join("bin");
    let tool = |names: &[&str]| names.iter().map(|n| bin.join(n)).find(|p| p.is_file());

    let mut cmd = if let Some(android) = tool(&["android", "android.exe", "android.bat"]) {
        let mut cmd = Command::new(android);
        cmd.arg(format!("--sdk={}", sdk.display())).args([
            "sdk",
            "install",
            &format!("ndk/{version}"),
        ]);
        cmd
    } else if let Some(sdkmanager) = tool(&["sdkmanager", "sdkmanager.bat"]) {
        let mut cmd = Command::new(sdkmanager);
        cmd.arg(format!("--sdk_root={}", sdk.display()))
            .arg(format!("ndk;{version}"));
        // sdkmanager is a Java program; fall back to Android Studio's bundled JDK.
        if std::env::var_os("JAVA_HOME").is_none()
            && let Some(jbr) = android_studio_jdk()
        {
            cmd.env("JAVA_HOME", jbr);
        }
        cmd
    } else {
        bail!(
            "can't install the Android NDK automatically: the Android SDK Command-line Tools \
             are not in {} (SDK Manager > SDK Tools > Android SDK Command-line Tools). \
             Or {NDK_HINT}",
            bin.display()
        );
    };
    run(&mut cmd).context("installing the Android NDK")
}

/// Android Studio's bundled JDK (macOS), used when no JDK is configured.
pub fn android_studio_jdk() -> Option<PathBuf> {
    let home = PathBuf::from("/Applications/Android Studio.app/Contents/jbr/Contents/Home");
    (cfg!(target_os = "macos") && home.is_dir()).then_some(home)
}

/// A complete NDK install has `source.properties` at its root (an empty or half-downloaded
/// directory doesn't).
fn is_ndk(dir: &Path) -> bool {
    dir.join("source.properties").is_file()
}

/// The highest-versioned NDK in `<sdk>/ndk/` (e.g. `27.2.12479018` over `26.3.11579264`).
fn newest_ndk(ndk_dir: &Path) -> Option<PathBuf> {
    std::fs::read_dir(ndk_dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| is_ndk(p))
        .filter_map(|p| {
            let version: Vec<u64> = p
                .file_name()?
                .to_str()?
                .split('.')
                .map(|n| n.parse().ok())
                .collect::<Option<_>>()?;
            Some((version, p))
        })
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, p)| p)
}

#[cfg(test)]
mod tests {
    use super::newest_ndk;

    #[test]
    fn picks_newest_complete_ndk() {
        let root = std::env::temp_dir().join(format!("rux-ndk-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for (version, complete) in [
            ("26.3.11579264", true),
            ("27.2.12479018", true),
            ("28.0.1", false), // empty / half-downloaded
            ("not-a-version", true),
        ] {
            let dir = root.join(version);
            std::fs::create_dir_all(&dir).unwrap();
            if complete {
                std::fs::write(dir.join("source.properties"), "").unwrap();
            }
        }
        assert_eq!(newest_ndk(&root), Some(root.join("27.2.12479018")));
        assert_eq!(newest_ndk(&root.join("missing")), None);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
