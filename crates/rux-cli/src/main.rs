//! `rux` — build tool for rux apps on Android / iOS.
//!
//! Run inside the app crate directory (where `Cargo.toml` has `[package.metadata.rux]`):
//!
//! ```text
//! rux setup [--no-modify-path] [-y]
//! rux new <dir> [--name <package>] [--app-id <id>] [--platforms android,ios]
//! rux doctor
//! rux build android [--release] [--abi arm64-v8a --abi x86_64]
//! rux build ios [--release]
//! rux run android [--release] [--device <serial>] [--abi <abi>] [--no-hot] [--detach]
//! rux run ios [--release] [--device <device / simulator name or id>] [--no-hot] [--detach]
//! ```
//!
//! `rux run` stays attached: it streams the app's logs, and debug builds
//! hot reload.
//!
//! The app version (`version = "x.y.z+n"`) and application id (`app-id` in
//! `[package.metadata.rux]`) come from Cargo.toml and are written to
//! `<android>/rux.properties` and `<ios>/rux.xcconfig` on every build.

mod android;
mod app;
mod app_id;
mod deps;
mod doctor;
mod hot;
mod install;
mod ios;
mod native;
mod new;
mod sdk;
mod toolchain;
mod util;
mod version;

use std::path::PathBuf;

use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::app::AppCrate;

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
    /// Create a Rust app with features for the selected native platforms.
    New {
        /// Directory to create (must not exist, or be empty).
        dir: PathBuf,
        /// Cargo package name. Default: the directory's name.
        #[arg(long)]
        name: Option<String>,
        /// Application id for Android and iOS. Default: `com.example.<name>`.
        #[arg(long)]
        app_id: Option<String>,
        /// Native platforms to enable for the app (comma-separated or repeated).
        #[arg(long, value_delimiter = ',', default_value = "android,ios")]
        platforms: Vec<Platform>,
    },
    /// Install `rux` in ~/.rux/bin and put it on PATH, install the rux library (SDK) of this
    /// CLI's version (linked into the app in the current directory / --path, if any), and the
    /// Rust tools it builds with (Rust toolchain + targets, cargo-ndk; asks first).
    Setup {
        /// Don't change PATH (the shell startup files / the user `Path` on Windows).
        #[arg(long)]
        no_modify_path: bool,
        /// Install the missing Rust tools without asking.
        #[arg(long, short)]
        yes: bool,
    },
    /// Check the required toolchains.
    Doctor,
    /// Generate native sources and build the library for selected platforms.
    Build {
        platform: Platform,
        #[command(flatten)]
        opts: BuildOpts,
    },
    /// Build the app, install it on a device / simulator and launch it.
    Run {
        platform: Platform,
        #[command(flatten)]
        opts: BuildOpts,
        /// Android: device serial (see `adb devices`). iOS: name or id of a connected device
        /// (see `xcrun devicectl list devices`) or simulator (see `xcrun simctl list devices`).
        /// Default: the only connected Android device / the booted simulator, else the newest
        /// iPhone simulator.
        #[arg(long, short)]
        device: Option<String>,
        /// Stay attached without hot reload (debug builds have it by default: save a file to
        /// hot reload; `r` hot reload, `R` hot restart).
        #[arg(long)]
        no_hot: bool,
        /// Just launch the app and exit, instead of staying attached (logs, hot reload).
        #[arg(long, conflicts_with = "no_hot")]
        detach: bool,
    },
}

#[derive(Args)]
struct BuildOpts {
    /// Build with the release profile / configuration.
    #[arg(long)]
    release: bool,
    /// Android ABI (repeat to build several). Default: arm64-v8a for `build`, the device's ABI
    /// for `run`.
    #[arg(long = "abi")]
    abis: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Platform {
    Android,
    Ios,
}

impl Platform {
    const ALL: [Self; 2] = [Self::Android, Self::Ios];

    fn name(self) -> &'static str {
        match self {
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }

    fn build(self, app: &AppCrate, release: bool, abis: &[String]) -> Result<()> {
        match self {
            Self::Android => android::build(app, release, abis).map(|_| ()),
            Self::Ios => ios::build(app, release).map(|_| ()),
        }
    }
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::New {
            dir,
            name,
            app_id,
            platforms,
        } => new::run(
            &cli.path.join(dir),
            name.as_deref(),
            app_id.as_deref(),
            &platforms,
        ),
        Cmd::Setup {
            no_modify_path,
            yes,
        } => sdk::setup(&cli.path, !no_modify_path, yes),
        Cmd::Doctor => doctor::run(),
        Cmd::Build { platform, opts } => {
            let app = AppCrate::load(&cli.path)?;
            app.require_platform(platform)?;
            sdk::ensure(&app.dir)?;
            platform.build(&app, opts.release, &opts.abis)
        }
        Cmd::Run {
            platform,
            opts,
            device,
            no_hot,
            detach,
        } => {
            let app = AppCrate::load(&cli.path)?;
            app.require_platform(platform)?;
            sdk::ensure(&app.dir)?;
            let device = device.as_deref();
            let (hot, attach) = (!no_hot, !detach);
            match platform {
                Platform::Android => {
                    android::run(&app, opts.release, &opts.abis, device, hot, attach)
                }
                Platform::Ios => ios::run(&app, opts.release, device, hot, attach),
            }
        }
    }
}
