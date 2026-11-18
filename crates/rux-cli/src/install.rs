//! Part of `rux setup`: installs the running `rux` in `<rux home>/bin` (so the downloaded file
//! can go) and puts that directory on the user's PATH, like rustup:
//!
//! - Windows: the user `Path` in the registry (`HKCU\Environment`), keeping `%VAR%` entries;
//! - elsewhere: `<rux home>/env` (a POSIX sh snippet), sourced from the shell startup files, and
//!   a fish `conf.d` file for fish users.

use std::path::Path;

use anyhow::{Context, Result};

/// Installs the CLI and, if `modify_path`, puts it on PATH.
pub fn install_cli(rux_home: &Path, modify_path: bool) -> Result<()> {
    let bin = rux_home.join("bin");
    let target = bin.join(format!("rux{}", std::env::consts::EXE_SUFFIX));
    let exe = std::env::current_exe().context("locating the running rux")?;

    if !same_file(&exe, &target) {
        // Installed some other way (e.g. `cargo install`) and already reachable: leave it.
        if exe.parent().is_some_and(on_path) {
            println!("✓ rux is on your PATH ({})", exe.display());
            return Ok(());
        }
        copy_cli(&exe, &target)?;
        println!("✓ installed rux in {}", target.display());
    }

    if !modify_path {
        println!(
            "  add {} to your PATH to run `rux` from anywhere",
            bin.display()
        );
    } else if on_path(&bin) {
        println!("✓ {} is on your PATH", bin.display());
    } else {
        add_to_path(rux_home, &bin)?;
    }
    Ok(())
}

/// Copies the running CLI to `target`, replacing an older one even while it runs.
fn copy_cli(exe: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target.parent().context("bin directory")?)?;
    // Written aside then renamed: a running binary can be renamed (Windows) / replaced (Unix),
    // not overwritten.
    let tmp = target.with_extension("new");
    std::fs::copy(exe, &tmp).with_context(|| format!("copying rux to {}", tmp.display()))?;
    if cfg!(windows) && target.exists() {
        let old = target.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(target, &old)
            .with_context(|| format!("moving the old {} aside", target.display()))?;
    }
    std::fs::rename(&tmp, target).with_context(|| format!("installing {}", target.display()))?;
    Ok(())
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// Whether `dir` is on this process's PATH.
fn on_path(dir: &Path) -> bool {
    let Ok(dir) = std::fs::canonicalize(dir) else {
        return false;
    };
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|p| std::fs::canonicalize(p).is_ok_and(|p| p == dir))
    })
}

#[cfg(windows)]
fn add_to_path(_rux_home: &Path, bin: &Path) -> Result<()> {
    if windows::add_to_user_var("Path", bin)? {
        println!(
            "→ added {} to your PATH: open a new terminal to use `rux`",
            bin.display()
        );
    }
    Ok(())
}

#[cfg(windows)]
mod windows {
    use std::path::Path;
    use std::process::Command;

    use anyhow::{Result, bail};

    use crate::util::output;

    /// Prepends `dir` to the user environment variable `name` (a `;` list) unless it's there.
    /// Reads the raw value so `%VAR%` entries survive, keeps its registry type, and broadcasts
    /// the change so new terminals see it. Returns whether it changed anything.
    pub fn add_to_user_var(name: &str, dir: &Path) -> Result<bool> {
        let out = powershell(
            r#"
$name = $env:RUX_VAR; $dir = $env:RUX_DIR
$key = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)
$old = $key.GetValue($name, '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
$parts = @($old -split ';' | Where-Object { $_ -ne '' })
if ($parts -contains $dir -or $parts -contains "$dir\") { 'present'; exit 0 }
if ($old -eq '') { $kind = [Microsoft.Win32.RegistryValueKind]::ExpandString }
else { $kind = $key.GetValueKind($name) }
$key.SetValue($name, ((@($dir) + $parts) -join ';'), $kind)
$key.Close()
# Setting a variable through .NET broadcasts WM_SETTINGCHANGE (Explorer, new terminals).
[Environment]::SetEnvironmentVariable('RUX_SETUP', '1', 'User')
[Environment]::SetEnvironmentVariable('RUX_SETUP', $null, 'User')
'added'
"#,
            name,
            dir,
        )?;
        match out.as_str() {
            "added" => Ok(true),
            "present" => Ok(false),
            _ => bail!("unexpected output while updating PATH: {out}"),
        }
    }

    #[cfg(test)]
    pub fn read_user_var(name: &str) -> Result<String> {
        powershell(
            "[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment').GetValue(\
             $env:RUX_VAR, '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)",
            name,
            Path::new(""),
        )
    }

    #[cfg(test)]
    pub fn delete_user_var(name: &str) -> Result<()> {
        powershell(
            "[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $true)\
             .DeleteValue($env:RUX_VAR, $false)",
            name,
            Path::new(""),
        )
        .map(drop)
    }

    /// Runs a PowerShell script; values go through the environment, never into the script text.
    fn powershell(script: &str, var: &str, dir: &Path) -> Result<String> {
        output(
            Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", script])
                .env("RUX_VAR", var)
                .env("RUX_DIR", dir),
        )
    }
}

#[cfg(not(windows))]
fn add_to_path(rux_home: &Path, bin: &Path) -> Result<()> {
    let user_home = std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(std::path::PathBuf::from)
        .context("HOME is not set")?;
    let shell = std::env::var("SHELL").unwrap_or_default();
    let changed = unix::add_to_path(&user_home, rux_home, bin, &shell)?;
    let env = rux_home.join("env");
    if changed.is_empty() {
        println!("✓ {} is set up to be on your PATH", bin.display());
    } else {
        let files: Vec<_> = changed.iter().map(|f| f.display().to_string()).collect();
        println!(
            "→ added {} to your PATH (in {})",
            bin.display(),
            files.join(", ")
        );
    }
    println!(
        "  open a new terminal to use `rux`, or run now: . \"{}\"",
        env.display()
    );
    Ok(())
}

#[cfg(any(not(windows), test))]
#[cfg_attr(windows, allow(dead_code))]
mod unix {
    use std::path::{Path, PathBuf};

    use anyhow::{Context, Result};

    /// Writes `<rux home>/env` and sources it from the startup files of the user's shells;
    /// returns the files it changed.
    pub fn add_to_path(
        user_home: &Path,
        rux_home: &Path,
        bin: &Path,
        shell: &str,
    ) -> Result<Vec<PathBuf>> {
        let bin_q = sh_quote(&bin.to_string_lossy());
        let env = rux_home.join("env");
        write(
            &env,
            &format!(
                "#!/bin/sh\n# Added by `rux setup`: puts the rux CLI on PATH.\n\
                 case \":${{PATH}}:\" in\n    *:{bin_q}:*) ;;\n    *) export PATH={bin_q}\":$PATH\" ;;\nesac\n"
            ),
        )?;

        let line = format!(". {}  # rux\n", sh_quote(&env.to_string_lossy()));
        // `.profile`: login shells (and bash when it has no `.bash_profile`).
        let mut files = vec![user_home.join(".profile")];
        // bash reads `.bash_profile` instead of `.profile` if it exists, and `.bashrc` for
        // interactive non-login shells.
        for f in [".bash_profile", ".bashrc"] {
            if user_home.join(f).is_file() || (f == ".bashrc" && shell.ends_with("bash")) {
                files.push(user_home.join(f));
            }
        }
        // zsh doesn't read `.profile`; `.zshenv` is read by every zsh (macOS' default shell).
        if shell.ends_with("zsh") || user_home.join(".zshrc").is_file() {
            files.push(user_home.join(".zshenv"));
        }

        let mut changed = Vec::new();
        for file in files {
            let old = std::fs::read_to_string(&file).unwrap_or_default();
            if old.contains(&line) {
                continue;
            }
            let sep = if old.is_empty() || old.ends_with('\n') {
                ""
            } else {
                "\n"
            };
            write(&file, &format!("{old}{sep}{line}"))?;
            changed.push(file);
        }

        let fish = user_home.join(".config/fish");
        if fish.is_dir() || shell.ends_with("fish") {
            let conf = fish.join("conf.d/rux.fish");
            let body = format!(
                "# Added by `rux setup`: puts the rux CLI on PATH.\n\
                 contains {bin_q} $PATH; or set -gx PATH {bin_q} $PATH\n"
            );
            if std::fs::read_to_string(&conf).ok().as_deref() != Some(&body) {
                write(&conf, &body)?;
                changed.push(conf);
            }
        }
        Ok(changed)
    }

    /// Single-quoted for sh and fish (`'` → `'\''`).
    fn sh_quote(s: &str) -> String {
        format!("'{}'", s.replace('\'', r"'\''"))
    }

    fn write(path: &Path, body: &str) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, body).with_context(|| format!("writing {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rux-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn replaces_an_installed_cli() {
        let dir = temp("copy");
        let (exe, target) = (dir.join("downloaded"), dir.join("bin/rux"));
        std::fs::write(&exe, "v1").unwrap();
        copy_cli(&exe, &target).unwrap();
        std::fs::write(&exe, "v2").unwrap();
        copy_cli(&exe, &target).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "v2");
        assert!(same_file(&target, &dir.join("bin/../bin/rux")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sources_the_env_file_from_shell_startup_files() {
        let (user, rux) = (temp("user"), temp("rux"));
        std::fs::write(user.join(".bashrc"), "alias ll='ls -l'").unwrap();
        std::fs::create_dir_all(user.join(".config/fish")).unwrap();
        let bin = rux.join("bin");

        let changed = unix::add_to_path(&user, &rux, &bin, "/bin/zsh").unwrap();
        let names: Vec<_> = changed
            .iter()
            .map(|f| {
                f.strip_prefix(&user)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect();
        assert_eq!(
            names,
            [
                ".profile",
                ".bashrc",
                ".zshenv",
                ".config/fish/conf.d/rux.fish"
            ]
        );

        let bashrc = std::fs::read_to_string(user.join(".bashrc")).unwrap();
        assert!(bashrc.starts_with("alias ll='ls -l'\n. '"), "{bashrc}");
        let env = std::fs::read_to_string(rux.join("env")).unwrap();
        assert!(env.contains(&format!("export PATH='{}'\":$PATH\"", bin.display())));

        // Idempotent.
        assert!(
            unix::add_to_path(&user, &rux, &bin, "/bin/zsh")
                .unwrap()
                .is_empty()
        );
        std::fs::remove_dir_all(&user).unwrap();
        std::fs::remove_dir_all(&rux).unwrap();
    }

    /// Exercises the registry code on a throwaway variable, not the real `Path`.
    #[cfg(windows)]
    #[test]
    fn adds_to_a_user_variable_keeping_unexpanded_entries() {
        let var = format!("RUX_TEST_PATH_{}", std::process::id());
        let dir = Path::new(r"C:\rux test\bin");
        windows::delete_user_var(&var).unwrap();
        assert!(windows::add_to_user_var(&var, dir).unwrap());
        assert!(
            !windows::add_to_user_var(&var, dir).unwrap(),
            "already there"
        );
        assert_eq!(windows::read_user_var(&var).unwrap(), r"C:\rux test\bin");

        // Existing `%VAR%` entries stay unexpanded.
        windows::delete_user_var(&var).unwrap();
        let other = Path::new(r"%USERPROFILE%\tools");
        windows::add_to_user_var(&var, other).unwrap();
        windows::add_to_user_var(&var, dir).unwrap();
        assert_eq!(
            windows::read_user_var(&var).unwrap(),
            r"C:\rux test\bin;%USERPROFILE%\tools"
        );
        windows::delete_user_var(&var).unwrap();
    }
}
