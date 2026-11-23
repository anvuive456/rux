//! Helpers shared by the commands: running processes and writing generated files.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

/// Runs `cmd`, echoing it first, and fails if it exits unsuccessfully.
pub fn run(cmd: &mut Command) -> Result<()> {
    eprintln!("$ {cmd:?}");
    let status = cmd
        .status()
        .with_context(|| format!("spawning {:?}", cmd.get_program()))?;
    if !status.success() {
        bail!("command failed with {status}");
    }
    Ok(())
}

/// Runs a Cargo build and locates the application's build-script output for that build.
pub fn cargo_out_dir(cmd: &mut Command, manifest: &Path) -> Result<PathBuf> {
    let manifest = manifest.canonicalize()?;
    cmd.arg("--message-format=json-render-diagnostics")
        .stderr(Stdio::inherit());
    eprintln!("$ {cmd:?}");
    let output = cmd
        .output()
        .with_context(|| format!("spawning {:?}", cmd.get_program()))?;
    anyhow::ensure!(
        output.status.success(),
        "command failed with {}",
        output.status
    );

    let mut package = None;
    let mut outputs = std::collections::BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let Ok(message) = serde_json::from_str::<serde_json::Value>(line) else {
            eprintln!("{line}");
            continue;
        };
        let Some(id) = message["package_id"].as_str() else {
            continue;
        };
        match message["reason"].as_str() {
            Some("compiler-artifact")
                if message["manifest_path"].as_str().is_some_and(|path| {
                    Path::new(path)
                        .canonicalize()
                        .is_ok_and(|path| path == manifest)
                }) =>
            {
                package = Some(id.to_owned());
            }
            Some("build-script-executed") => {
                if let Some(out) = message["out_dir"].as_str() {
                    outputs.insert(id.to_owned(), PathBuf::from(out));
                }
            }
            _ => {}
        }
    }
    package
        .and_then(|id| outputs.remove(&id))
        .context("Cargo did not report an OUT_DIR for the app build script")
}

/// Runs `cmd` (output shown, command not echoed) and fails if it exits unsuccessfully.
pub fn run_quiet(cmd: &mut Command) -> Result<()> {
    let status = cmd
        .status()
        .with_context(|| format!("spawning {:?}", cmd.get_program()))?;
    if !status.success() {
        bail!("{:?} failed with {status}", cmd.get_program());
    }
    Ok(())
}

/// Runs `cmd` quietly and returns its trimmed stdout; fails with its stderr if it exits
/// unsuccessfully.
pub fn output(cmd: &mut Command) -> Result<String> {
    let out = cmd
        .output()
        .with_context(|| format!("spawning {:?}", cmd.get_program()))?;
    if !out.status.success() {
        bail!(
            "{cmd:?} failed with {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Finds `program` in `PATH`.
pub fn on_path(program: &str) -> Option<PathBuf> {
    let exe = format!("{program}{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&exe))
        .find(|p| p.is_file())
}

/// Whether `program arg` runs and exits successfully.
pub fn has(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Writes a file generated from Cargo.toml into a platform project.
pub fn write_generated(path: &Path, body: &str) -> Result<()> {
    std::fs::write(path, body).with_context(|| format!("writing {}", path.display()))?;
    println!("→ wrote {}", path.display());
    Ok(())
}
