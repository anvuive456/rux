//! The Rust tools rux builds with: the toolchain apps pin (`rust-toolchain.toml`, the CLI's
//! `rust-version`) with the mobile targets, and `cargo-ndk`. `rux setup` installs what's
//! missing (asking first), `rux doctor` reports it. Rust itself (rustup) is up to the user.

use std::io::{BufRead, IsTerminal, Write};
use std::process::Command;

use anyhow::{Context, Result};

use crate::util::{has, run};

/// The Rust version rux and its apps build with.
pub const RUST_VERSION: &str = env!("CARGO_PKG_RUST_VERSION");

pub const ANDROID_TARGETS: [&str; 2] = ["aarch64-linux-android", "x86_64-linux-android"];
pub const IOS_TARGETS: [&str; 2] = ["aarch64-apple-ios", "aarch64-apple-ios-sim"];

pub const RUSTUP_HINT: &str = "install Rust from https://rustup.rs, then run `rux setup`";

/// The targets this machine can build for: iOS needs macOS.
pub fn wanted_targets() -> Vec<&'static str> {
    let mut targets = ANDROID_TARGETS.to_vec();
    if cfg!(target_os = "macos") {
        targets.extend(IOS_TARGETS);
    }
    targets
}

pub fn has_rustup() -> bool {
    has("rustup", "--version")
}

/// What `rux setup` would install.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Missing {
    /// The whole toolchain (with all wanted targets).
    pub toolchain: bool,
    /// Targets of an installed toolchain.
    pub targets: Vec<&'static str>,
    pub cargo_ndk: bool,
}

impl Missing {
    /// Checks this machine (needs rustup).
    pub fn check() -> Self {
        let targets = installed_targets();
        let toolchain = targets.is_none();
        let targets = targets.unwrap_or_default();
        Self {
            toolchain,
            targets: if toolchain {
                Vec::new()
            } else {
                wanted_targets()
                    .into_iter()
                    .filter(|t| !targets.iter().any(|i| i == t))
                    .collect()
            },
            cargo_ndk: !has_cargo_ndk(),
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.toolchain && self.targets.is_empty() && !self.cargo_ndk
    }

    /// Human-readable list.
    pub fn describe(&self) -> Vec<String> {
        let mut items = Vec::new();
        if self.toolchain {
            items.push(format!(
                "Rust {RUST_VERSION} with the targets {}",
                wanted_targets().join(", ")
            ));
        }
        if !self.targets.is_empty() {
            items.push(format!(
                "Rust {RUST_VERSION} targets {}",
                self.targets.join(", ")
            ));
        }
        if self.cargo_ndk {
            items.push("cargo-ndk (builds Rust for Android)".to_owned());
        }
        items
    }

    /// The commands that install it.
    pub fn commands(&self) -> Vec<Command> {
        let mut cmds = Vec::new();
        if self.toolchain {
            let mut cmd = Command::new("rustup");
            cmd.args(["toolchain", "install", RUST_VERSION, "--profile", "minimal"])
                .args(["--component", "rustfmt,clippy"])
                .args(["--target", &wanted_targets().join(",")]);
            cmds.push(cmd);
        }
        if !self.targets.is_empty() {
            let mut cmd = Command::new("rustup");
            cmd.args(["target", "add", "--toolchain", RUST_VERSION])
                .args(&self.targets);
            cmds.push(cmd);
        }
        if self.cargo_ndk {
            let mut cmd = Command::new("cargo");
            cmd.arg(format!("+{RUST_VERSION}"))
                .args(["install", "cargo-ndk", "--locked"]);
            cmds.push(cmd);
        }
        cmds
    }
}

/// Targets installed for [`RUST_VERSION`]; `None` if that toolchain isn't installed.
pub fn installed_targets() -> Option<Vec<String>> {
    // `rustup +<toolchain>` would install a missing toolchain: check the list first.
    let list = rustup(&["toolchain", "list"])?;
    let installed = list.lines().any(|l| {
        let name = l.split_whitespace().next().unwrap_or_default();
        name == RUST_VERSION || name.starts_with(&format!("{RUST_VERSION}-"))
    });
    if !installed {
        return None;
    }
    let targets = rustup(&[&format!("+{RUST_VERSION}"), "target", "list", "--installed"])?;
    Some(targets.lines().map(str::to_owned).collect())
}

fn rustup(args: &[&str]) -> Option<String> {
    let out = Command::new("rustup")
        .args(args)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn has_cargo_ndk() -> bool {
    has("cargo-ndk", "--version")
        || Command::new("cargo")
            .args(["ndk", "--version"])
            .output()
            .is_ok_and(|o| o.status.success())
}

/// Part of `rux setup`: lists what's missing and installs it, asking first unless `yes`.
pub fn setup(yes: bool) -> Result<()> {
    if !has_rustup() {
        println!("⚠ Rust (rustup) isn't installed: {RUSTUP_HINT}");
        return Ok(());
    }
    let missing = Missing::check();
    if missing.is_empty() {
        println!("✓ Rust {RUST_VERSION}, its targets and cargo-ndk are installed");
        return Ok(());
    }
    println!("rux also needs:");
    for item in missing.describe() {
        println!("  - {item}");
    }
    if !(yes || confirm("Install them now?")?) {
        println!("  skipped: run `rux setup -y` to install them, or:");
        for cmd in missing.commands() {
            println!("    {}", show(&cmd));
        }
        return Ok(());
    }
    for mut cmd in missing.commands() {
        run(&mut cmd).context("installing the Rust tools")?;
    }
    println!("✓ installed the Rust tools");
    Ok(())
}

/// `[Y/n]` on a terminal; `false` when not interactive.
fn confirm(question: &str) -> Result<bool> {
    if !std::io::stdin().is_terminal() {
        return Ok(false);
    }
    print!("{question} [Y/n] ");
    std::io::stdout().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "" | "y" | "yes"
    ))
}

/// `program arg …` for display.
fn show(cmd: &Command) -> String {
    std::iter::once(cmd.get_program())
        .chain(cmd.get_args())
        .map(|a| a.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_the_install_commands() {
        let all = Missing {
            toolchain: true,
            targets: Vec::new(),
            cargo_ndk: true,
        };
        let cmds: Vec<_> = all.commands().iter().map(show).collect();
        assert_eq!(
            cmds,
            [
                format!(
                    "rustup toolchain install {RUST_VERSION} --profile minimal --component \
                     rustfmt,clippy --target {}",
                    wanted_targets().join(",")
                ),
                format!("cargo +{RUST_VERSION} install cargo-ndk --locked"),
            ]
        );

        let targets = Missing {
            targets: vec!["x86_64-linux-android"],
            ..Default::default()
        };
        assert_eq!(
            targets.commands().iter().map(show).collect::<Vec<_>>(),
            [format!(
                "rustup target add --toolchain {RUST_VERSION} x86_64-linux-android"
            )]
        );
        assert!(Missing::default().is_empty());
        assert!(Missing::default().commands().is_empty());
    }
}
