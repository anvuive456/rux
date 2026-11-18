//! `rux` — build tool for rux apps on Android / iOS.
//!
//! Run inside the app crate directory (where `Cargo.toml` has `[package.metadata.rux]`):
//!
//! ```text
//! rux doctor
//! rux build android [--release] [--abi arm64-v8a --abi x86_64]
//! rux build ios [--release]
//! rux run android [--release] [--device <serial>] [--abi <abi>]
//! rux run ios [--release] [--device <simulator name or UDID>]
//! ```
//!
//! `rux run` in debug builds stays attached with hot reload, like `flutter run`.
//!
//! The app version (`version = "x.y.z+n"`) and application id (`app-id` in
//! `[package.metadata.rux]`) come from Cargo.toml and are written to
//! `<android>/rux.properties` and `<ios>/rux.xcconfig` on every build.

mod android;
mod app;
mod app_id;
mod doctor;
mod hot;
mod ios;
mod native;
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
        /// Android: device serial (see `adb devices`). iOS: simulator name or UDID
        /// (see `xcrun simctl list devices`). Default: the only connected Android device /
        /// the booted simulator, else the newest iPhone simulator.
        #[arg(long, short)]
        device: Option<String>,
        /// Just launch the app, without the hot reload session (debug builds have it by
        /// default: save a file to hot reload; `r` hot reload, `R` hot restart, `q` quit).
        #[arg(long)]
        no_hot: bool,
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
        } => {
            let app = AppCrate::load(&cli.path)?;
            let device = device.as_deref();
            let hot = !no_hot;
            match platform {
                Platform::Android => android::run(&app, opts.release, &opts.abis, device, hot),
                Platform::Ios => ios::run(&app, opts.release, device, hot),
            }
        }
    }
}
