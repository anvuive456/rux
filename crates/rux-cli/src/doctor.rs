//! `rux doctor`: checks the required toolchains.

use std::path::Path;
use std::process::Command;

use anyhow::Result;

use crate::android::sdk::{DEFAULT_NDK_VERSION, NDK_HINT, find_ndk, sdk_dirs};
use crate::util::has;

pub fn run() -> Result<()> {
    let installed = Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();

    let check = |ok: bool, label: &str, hint: &str| {
        if ok {
            println!("  [ok] {label}");
        } else {
            println!("  [--] {label}\n       → {hint}");
        }
    };

    println!("Android");
    for t in ["aarch64-linux-android", "x86_64-linux-android"] {
        check(installed.contains(t), t, &format!("rustup target add {t}"));
    }
    check(
        has("cargo", "ndk") || has("cargo-ndk", "--version"),
        "cargo-ndk",
        "cargo install cargo-ndk",
    );
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

    println!("iOS");
    if cfg!(target_os = "macos") {
        for t in ["aarch64-apple-ios", "aarch64-apple-ios-sim"] {
            check(installed.contains(t), t, &format!("rustup target add {t}"));
        }
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
