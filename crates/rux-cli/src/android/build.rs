//! `rux build android`: writes `rux.properties` + `local.properties` and builds `lib<name>.so`
//! with cargo-ndk into the Gradle project.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};

use super::sdk::{SDK_HINT, ensure_ndk, sdk_dirs};
use crate::Platform;
use crate::app::AppCrate;
use crate::deps::Deps;
use crate::util::{run, run_quiet, write_generated};

/// ABI built when none is given.
const DEFAULT_ABI: &str = "arm64-v8a";

pub fn build(app: &AppCrate, release: bool, abis: &[String]) -> Result<PathBuf> {
    let default_abi = [DEFAULT_ABI.to_owned()];
    let abis = if abis.is_empty() { &default_abi } else { abis };
    anyhow::ensure!(
        app.meta.android_project.is_some() || app.generates_native_projects(),
        "missing `android-project` in [package.metadata.rux]"
    );
    let targets = abis
        .iter()
        .map(|abi| rust_target(abi))
        .collect::<Result<Vec<_>>>()?;
    let deps = Deps::for_build(app, Platform::Android, &targets)?;
    let ndk = ensure_ndk(app.meta.android_ndk_version.as_deref())?;
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&app.dir)
        .env("ANDROID_NDK_HOME", &ndk)
        .arg("ndk");
    for abi in abis {
        cmd.args(["-t", abi]);
    }
    let project = if let Some(path) = &app.meta.android_project {
        let project = app.dir.join(path);
        remove_legacy_libs(&project, &app.lib_name)?;
        cmd.arg("-o").arg(jni_libs(&project, release));
        build_args(&mut cmd, app, release);
        run(&mut cmd)?;
        project
    } else {
        let stage = app
            .target_dir()?
            .join(format!(".rux-jni-{}", std::process::id()));
        std::fs::create_dir_all(stage.parent().context("missing Cargo target directory")?)?;
        std::fs::create_dir(&stage)?;
        let result = (|| -> Result<PathBuf> {
            cmd.arg("-o").arg(&stage);
            build_args(&mut cmd, app, release);
            let project = ndk_out_dir(&mut cmd, &app.package, targets[0], true)?.join("android");
            anyhow::ensure!(
                project.join("settings.gradle.kts").is_file(),
                "Android project was not generated in {}",
                project.display()
            );
            let jni_libs = jni_libs(&project, release);
            for abi in abis {
                let dest = jni_libs.join(abi);
                std::fs::create_dir_all(&dest)?;
                for library in std::fs::read_dir(stage.join(abi))? {
                    let library = library?;
                    std::fs::copy(library.path(), dest.join(library.file_name()))?;
                }
            }
            Ok(project)
        })();
        let _ = std::fs::remove_dir_all(&stage);
        result?
    };
    let sources = crate::native::generate_android(&deps, &project)?;
    write_config(&project, app, &deps.android_runtime()?, &sources)?;
    write_sdk_dir(&project)?;
    println!("→ Android project: {}", project.display());
    Ok(project)
}

fn ndk_out_dir(cmd: &mut Command, package: &str, target: &str, trace: bool) -> Result<PathBuf> {
    cmd.env("RUX_REPORT_ANDROID_OUT_DIR", "1")
        .env("CARGO_TERM_COLOR", "never")
        .env("CARGO_TERM_QUIET", "false")
        .stderr(Stdio::piped());
    if trace {
        eprintln!("$ {cmd:?}");
    }
    let mut child = cmd.spawn().context("starting cargo ndk")?;
    let stderr = child.stderr.take().context("capturing cargo ndk stderr")?;
    let warning = format!("warning: {package}@");
    let marker = format!(": RUX_ANDROID_OUT_DIR[{target}]=");
    let mut out = None;
    for line in BufReader::new(stderr).lines() {
        let line = line?;
        if line.starts_with(&warning) && line.contains(": RUX_ANDROID_OUT_DIR[") {
            if let Some((_, path)) = line.split_once(&marker) {
                out = Some(PathBuf::from(path));
            }
        } else {
            eprintln!("{line}");
        }
    }
    let status = child.wait()?;
    anyhow::ensure!(status.success(), "command failed with {status}");
    out.context("Cargo did not report an OUT_DIR for the Android app build script")
}

fn jni_libs(project: &Path, release: bool) -> PathBuf {
    project
        .join("app/src")
        .join(if release { "release" } else { "debug" })
        .join("jniLibs")
}

fn build_args(cmd: &mut Command, app: &AppCrate, release: bool) {
    cmd.args(["build", "-p", &app.package, "--lib"])
        .args(app.cargo_args(Platform::Android));
    if release {
        cmd.arg("--release");
    }
}

/// rux used to build into `src/main/jniLibs` for every build type; a library left there would
/// clash with the per-build-type one when Gradle merges them.
fn remove_legacy_libs(project: &Path, lib_name: &str) -> Result<()> {
    let main = project.join("app/src/main/jniLibs");
    let Ok(abis) = std::fs::read_dir(&main) else {
        return Ok(());
    };
    for abi in abis.flatten() {
        let lib = abi.path().join(format!("lib{lib_name}.so"));
        if lib.is_file() {
            std::fs::remove_file(&lib).with_context(|| format!("removing {}", lib.display()))?;
            println!("→ removed {} (now built per build type)", lib.display());
            // Keeps `jniLibs/<abi>` if it holds other libraries.
            let _ = std::fs::remove_dir(abi.path());
        }
    }
    let _ = std::fs::remove_dir(&main);
    Ok(())
}

/// Rebuilds just the app library for `abi` (debug) for hot reload; returns its path.
pub fn hot_library(app: &AppCrate, abi: &str, target_dir: &Path) -> Result<PathBuf> {
    let ndk = ensure_ndk(app.meta.android_ndk_version.as_deref())?;
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&app.dir)
        .env("ANDROID_NDK_HOME", &ndk)
        .args(["ndk", "-t", abi, "build", "-p", &app.package, "--lib"]);
    let target = rust_target(abi)?;
    cmd.args(app.cargo_args(Platform::Android));
    if app.generates_native_projects() && app.meta.android_project.is_none() {
        ndk_out_dir(&mut cmd, &app.package, target, false)?;
    } else {
        run_quiet(&mut cmd)?;
    }
    Ok(target_dir
        .join(target)
        .join("debug")
        .join(format!("lib{}.so", app.lib_name)))
}

/// Rust target triple of an Android ABI.
fn rust_target(abi: &str) -> Result<&'static str> {
    Ok(match abi {
        "arm64-v8a" => "aarch64-linux-android",
        "armeabi-v7a" => "armv7-linux-androideabi",
        "x86_64" => "x86_64-linux-android",
        "x86" => "i686-linux-android",
        _ => bail!("unsupported Android ABI `{abi}`"),
    })
}

/// Writes `<android>/rux.properties` (application id, version, the extra source directories of
/// native APIs, and the rux runtime library), read by `app/build.gradle.kts` and
/// `settings.gradle.kts`.
fn write_config(project: &Path, app: &AppCrate, runtime: &Path, sources: &[PathBuf]) -> Result<()> {
    let app_id = app.meta.android_app_id()?;
    let v = &app.version;
    let sources: Vec<_> = sources
        .iter()
        .map(|p| escape_property(&p.to_string_lossy()))
        .collect();
    let body = format!(
        "# Generated by `rux build android` from Cargo.toml — do not edit.\n\
         rux.applicationId={app_id}\nrux.versionName={}\nrux.versionCode={}\n\
         rux.sourceDirs={}\nrux.runtimeDir={}\n",
        v.name,
        v.code,
        sources.join(","),
        escape_property(&runtime.to_string_lossy()),
    );
    write_generated(&project.join("rux.properties"), &body)
}

/// `\` and `:` are escapes in .properties (Windows paths: `C\:\\Users\\...`).
fn escape_property(value: &str) -> String {
    value.replace('\\', "\\\\").replace(':', "\\:")
}

/// Points Gradle at the Android SDK (`sdk.dir` in `<android>/local.properties`), like Android
/// Studio does, keeping the file's other entries.
fn write_sdk_dir(project: &Path) -> Result<()> {
    let sdk = sdk_dirs().into_iter().next().context(SDK_HINT)?;
    let path = project.join("local.properties");
    let old = std::fs::read_to_string(&path).unwrap_or_default();
    let new = set_property(&old, "sdk.dir", &sdk.to_string_lossy());
    if new != old {
        std::fs::write(&path, new).with_context(|| format!("writing {}", path.display()))?;
        println!("→ wrote sdk.dir to {}", path.display());
    }
    Ok(())
}

/// Sets `key=value` in a `.properties` file's text, replacing an existing entry for `key`.
fn set_property(text: &str, key: &str, value: &str) -> String {
    let line = format!("{key}={}", escape_property(value));
    let mut lines: Vec<String> = Vec::new();
    let mut replaced = false;
    for l in text.lines() {
        let is_key = l
            .split_once('=')
            .is_some_and(|(k, _)| k.trim() == key && !l.trim_start().starts_with('#'));
        if !is_key {
            lines.push(l.to_owned());
        } else if !replaced {
            lines.push(line.clone());
            replaced = true;
        }
    }
    if !replaced {
        lines.push(line);
    }
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::set_property;

    #[test]
    fn sets_sdk_dir_keeping_other_entries() {
        assert_eq!(set_property("", "sdk.dir", "/sdk"), "sdk.dir=/sdk\n");
        assert_eq!(
            set_property("# c\nsdk.dir=/old\nfoo=1\n", "sdk.dir", "/sdk"),
            "# c\nsdk.dir=/sdk\nfoo=1\n"
        );
        assert_eq!(
            set_property("", "sdk.dir", r"C:\Android\sdk"),
            "sdk.dir=C\\:\\\\Android\\\\sdk\n"
        );
    }
}
