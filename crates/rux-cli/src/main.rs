//! `rux` — build tool for rux apps on Android / iOS.
//!
//! Run inside the app crate directory (where `Cargo.toml` has `[package.metadata.rux]`):
//!
//! ```text
//! rux setup [--no-modify-path]
//! rux new <dir> [--name <package>] [--app-id <id>]
//! rux doctor
//! rux build android [--release] [--abi arm64-v8a --abi x86_64]
//! rux build ios [--release]
//! rux run android [--release] [--device <serial>] [--abi <abi>] [--no-hot] [--detach]
//! rux run ios [--release] [--device <device / simulator name or id>] [--no-hot] [--detach]
//! ```
//!
//! `rux run` stays attached like `flutter run`: it streams the app's logs, and debug builds
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
    /// Create a new app (Rust code, headless tests, Android and iOS projects).
    New {
        /// Directory to create (must not exist, or be empty).
        dir: PathBuf,
        /// Cargo package name. Default: the directory's name.
        #[arg(long)]
        name: Option<String>,
        /// Application id for Android and iOS. Default: `com.example.<name>`.
        #[arg(long)]
        app_id: Option<String>,
    },
    /// Install `rux` in ~/.rux/bin and put it on PATH, install the rux library (SDK) of this
    /// CLI's version, and link it into the app in the current directory (or --path) if any.
    Setup {
        /// Don't change PATH (the shell startup files / the user `Path` on Windows).
        #[arg(long)]
        no_modify_path: bool,
    },
    /// Check the required toolchains.
    Doctor,
    /// Build the native library and copy it into the platform project.
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

#[derive(Clone, Copy, ValueEnum)]
enum Platform {
    Android,
    Ios,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::New { dir, name, app_id } => {
            new::run(&cli.path.join(dir), name.as_deref(), app_id.as_deref())
        }
        Cmd::Setup { no_modify_path } => sdk::setup(&cli.path, !no_modify_path),
        Cmd::Doctor => doctor::run(),
        Cmd::Build { platform, opts } => {
            let app = AppCrate::load(&cli.path)?;
            match platform {
                Platform::Android => android::build(&app, opts.release, &opts.abis),
                Platform::Ios => ios::build(&app, opts.release),
            }
        }
        Cmd::Run {
            platform,
            opts,
            device,
            no_hot,
            detach,
        } => {
            let app = AppCrate::load(&cli.path)?;
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
