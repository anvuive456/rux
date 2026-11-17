//! `rux` — build tool for rux apps on Android / iOS.
//!
//! Run inside the app crate directory (where `Cargo.toml` has `[package.metadata.rux]`):
//!
//! ```text
//! rux doctor
//! rux build android [--release] [--abi arm64-v8a --abi x86_64]
//! rux build ios [--release]
//! ```

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;

#[derive(Parser)]
#[command(name = "rux", version, about = "Build rux apps for native platforms")]
struct Cli {
    /// App crate directory (default: current directory).
    #[arg(long, global = true, default_value = ".")]
    path: PathBuf,

    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Check the required toolchains.
    Doctor,
    /// Build the native library and copy it into the platform project.
    Build {
        platform: Platform,
        #[arg(long)]
        release: bool,
        /// Android ABI (repeat to build several ABIs).
        #[arg(long = "abi", default_values = ["arm64-v8a"])]
        abis: Vec<String>,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum Platform {
    Android,
    Ios,
}

// --- app Cargo.toml -------------------------------------------------------

#[derive(Deserialize)]
struct Manifest {
    package: Package,
    lib: Option<Lib>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    #[serde(default)]
    metadata: Metadata,
}

#[derive(Deserialize, Default)]
struct Metadata {
    #[serde(default)]
    rux: RuxMeta,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
struct RuxMeta {
    android_project: Option<PathBuf>,
    ios_project: Option<PathBuf>,
}

#[derive(Deserialize)]
struct Lib {
    name: Option<String>,
}

struct AppCrate {
    dir: PathBuf,
    package: String,
    lib_name: String,
    meta: RuxMeta,
}

impl AppCrate {
    fn load(dir: &Path) -> Result<Self> {
        // Absolute, so paths stay valid when commands run with `current_dir(&self.dir)`.
        let dir =
            std::path::absolute(dir).with_context(|| format!("resolving {}", dir.display()))?;
        let path = dir.join("Cargo.toml");
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let m: Manifest =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        let lib_name = m
            .lib
            .and_then(|l| l.name)
            .unwrap_or_else(|| m.package.name.replace('-', "_"));
        Ok(Self {
            dir,
            package: m.package.name,
            lib_name,
            meta: m.package.metadata.rux,
        })
    }

    fn target_dir(&self) -> Result<PathBuf> {
        #[derive(Deserialize)]
        struct Meta {
            target_directory: PathBuf,
        }
        let out = Command::new("cargo")
            .args(["metadata", "--format-version=1", "--no-deps"])
            .current_dir(&self.dir)
            .output()
            .context("running cargo metadata")?;
        if !out.status.success() {
            bail!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let meta: Meta = serde_json::from_slice(&out.stdout).context("parsing cargo metadata")?;
        Ok(meta.target_directory)
    }
}

// --- commands --------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Doctor => doctor(),
        Cmd::Build {
            platform,
            release,
            abis,
        } => {
            let app = AppCrate::load(&cli.path)?;
            match platform {
                Platform::Android => build_android(&app, release, &abis),
                Platform::Ios => build_ios(&app, release),
            }
        }
    }
}

fn run(cmd: &mut Command) -> Result<()> {
    eprintln!("$ {cmd:?}");
    let status = cmd
        .status()
        .with_context(|| format!("spawning {:?}", cmd.get_program()))?;
    if !status.success() {
        bail!("command failed with {status}");
    }
    Ok(())
}

fn has(program: &str, arg: &str) -> bool {
    Command::new(program)
        .arg(arg)
        .output()
        .is_ok_and(|o| o.status.success())
}

fn doctor() -> Result<()> {
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
    check(
        std::env::var_os("ANDROID_NDK_HOME").is_some()
            || std::env::var_os("ANDROID_NDK_ROOT").is_some(),
        "ANDROID_NDK_HOME",
        "install the NDK via Android Studio's SDK Manager and set ANDROID_NDK_HOME",
    );

    println!("iOS");
    if cfg!(target_os = "macos") {
        for t in ["aarch64-apple-ios", "aarch64-apple-ios-sim"] {
            check(installed.contains(t), t, &format!("rustup target add {t}"));
        }
        check(has("xcodebuild", "-version"), "xcodebuild", "install Xcode");
        check(
            has("xcodegen", "--version"),
            "xcodegen (optional)",
            "brew install xcodegen",
        );
    } else {
        println!("  [--] building for iOS requires macOS + Xcode");
    }
    Ok(())
}

fn build_android(app: &AppCrate, release: bool, abis: &[String]) -> Result<()> {
    let project = app
        .meta
        .android_project
        .as_ref()
        .context("missing `android-project` in [package.metadata.rux]")?;
    let jni_libs = app.dir.join(project).join("app/src/main/jniLibs");

    let mut cmd = Command::new("cargo");
    cmd.arg("ndk").current_dir(&app.dir);
    for abi in abis {
        cmd.args(["-t", abi]);
    }
    cmd.arg("-o")
        .arg(&jni_libs)
        .args(["build", "-p", &app.package, "--lib"]);
    if release {
        cmd.arg("--release");
    }
    run(&mut cmd)?;
    println!(
        "→ copied lib{}.so into {}",
        app.lib_name,
        jni_libs.display()
    );
    Ok(())
}

fn build_ios(app: &AppCrate, release: bool) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("building for iOS requires macOS + Xcode");
    }
    let profile = if release { "release" } else { "debug" };
    let target_dir = app.target_dir()?;
    let lib_file = format!("lib{}.a", app.lib_name);

    let targets = ["aarch64-apple-ios", "aarch64-apple-ios-sim"];
    for t in targets {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(&app.dir)
            .args(["build", "-p", &app.package, "--lib", "--target", t]);
        if release {
            cmd.arg("--release");
        }
        run(&mut cmd)?;
    }

    let out = target_dir
        .join("ios")
        .join(format!("{}.xcframework", app.lib_name));
    if out.exists() {
        std::fs::remove_dir_all(&out)?;
    }
    let mut cmd = Command::new("xcodebuild");
    cmd.arg("-create-xcframework");
    for t in targets {
        cmd.arg("-library")
            .arg(target_dir.join(t).join(profile).join(&lib_file));
    }
    cmd.arg("-output").arg(&out);
    run(&mut cmd)?;

    println!("→ {}", out.display());
    if let Some(p) = &app.meta.ios_project {
        println!(
            "  open the project: cd {} && xcodegen && open *.xcodeproj",
            app.dir.join(p).display()
        );
    }
    Ok(())
}
