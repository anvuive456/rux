//! `rux doctor`: checks the required toolchains.

use std::path::Path;

use anyhow::Result;

use crate::android::java;
use crate::android::sdk::{DEFAULT_NDK_VERSION, NDK_HINT, find_ndk, sdk_dirs};
use crate::toolchain::{
    self, ANDROID_TARGETS, IOS_TARGETS, RUST_VERSION, RUSTUP_HINT, has_cargo_ndk,
};
use crate::util::has;

pub fn run() -> Result<()> {
    let check = |ok: bool, label: &str, hint: &str| {
        if ok {
            println!("  [ok] {label}");
        } else {
            println!("  [--] {label}\n       → {hint}");
        }
    };

    println!("rux {}", crate::sdk::VERSION);
    let sdk = crate::sdk::status();
    check(
        !sdk.starts_with("not"),
        &format!("rux SDK: {sdk}"),
        "rux setup",
    );

    // The targets of the toolchain apps build with, not of the default one.
    println!("Rust");
    let rustup = toolchain::has_rustup();
    check(rustup, "rustup", RUSTUP_HINT);
    let targets = if rustup {
        toolchain::installed_targets()
    } else {
        None
    };
    check(
        targets.is_some(),
        &format!("Rust {RUST_VERSION}"),
        "rux setup (or it's installed on the first build)",
    );
    let targets = targets.unwrap_or_default();
    let target_check = |t: &str| {
        check(
            targets.iter().any(|i| i == t),
            &format!("{t} (Rust {RUST_VERSION})"),
            "rux setup (or it's installed on the first build)",
        );
    };

    println!("Android");
    ANDROID_TARGETS.iter().for_each(|t| target_check(t));
    check(has_cargo_ndk(), "cargo-ndk", "rux setup");
    let sdk = sdk_dirs().into_iter().next();
    check(
        sdk.is_some(),
        &found("Android SDK", sdk.as_deref()),
        "install Android Studio, or set ANDROID_HOME to your SDK",
    );
    let ndk = find_ndk();
    check(
        ndk.is_some(),
        &found("Android NDK", ndk.as_deref()),
        &format!(
            "`rux build android` installs NDK {DEFAULT_NDK_VERSION} automatically; or {NDK_HINT}"
        ),
    );
    match java::find() {
        None => check(false, "Java (for Gradle)", java::HINT),
        Some(jdk) => match jdk.version() {
            Some(v) => check(
                v >= java::MIN_VERSION,
                &format!("Java {v} ({})", jdk.describe()),
                &format!("Gradle needs Java {}+: {}", java::MIN_VERSION, java::HINT),
            ),
            None => check(
                false,
                &format!("Java ({})", jdk.describe()),
                &format!("can't run {}: {}", jdk.exe.display(), java::HINT),
            ),
        },
    }

    println!("iOS");
    if cfg!(target_os = "macos") {
        IOS_TARGETS.iter().for_each(|t| target_check(t));
        check(has("xcodebuild", "-version"), "xcodebuild", "install Xcode");
        check(
            has("xcodegen", "--version"),
            "xcodegen (needed by `rux run ios`)",
            "brew install xcodegen",
        );
    } else {
        println!("  [--] building for iOS requires macOS + Xcode");
    }
    Ok(())
}

/// `label (path)` when found, else `label`.
fn found(label: &str, path: Option<&Path>) -> String {
    match path {
        Some(p) => format!("{label} ({})", p.display()),
        None => label.to_owned(),
    }
}
