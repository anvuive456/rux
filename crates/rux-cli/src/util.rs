//! Helpers shared by the commands: running processes and writing generated files.

use std::path::{Path, PathBuf};
use std::process::Command;

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
