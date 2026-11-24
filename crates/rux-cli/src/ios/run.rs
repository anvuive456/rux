//! `rux run ios`: builds the xcframework + app, installs it on a simulator or a physical device,
//! launches it and stays attached (app output + hot reload on simulators in debug builds)
//! unless detached.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::devicectl::Device;
use super::signing::development_team;
use super::simctl::Simulator;
use crate::Platform;
use crate::app::AppCrate;
use crate::hot::{self, DevServer, HotReload, Logs};
use crate::util::{on_path, run as run_cmd, run_quiet};

/// Where the app runs.
enum Target {
    Simulator(Simulator),
    Device(Device),
}

pub fn run(
    app: &AppCrate,
    release: bool,
    device: Option<&str>,
    hot: bool,
    attach: bool,
) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("running on iOS requires macOS + Xcode");
    }
    // Fail on config / tool / device problems before the (slow) builds.
    let bundle_id = app.meta.ios_bundle_id()?;
    let target = pick_target(device)?;
    let team = match &target {
        Target::Simulator(sim) => {
            println!("→ simulator {} ({})", sim.name, sim.udid);
            None
        }
        Target::Device(device) => {
            println!("→ device {} ({})", device.name, device.udid);
            Some(development_team(app)?)
        }
    };

    let project = super::build(app, release)?.context("missing iOS project")?;
    // The .xcodeproj is generated (and gitignored) when the project uses XcodeGen.
    let xcodegen = if project.join("project.yml").is_file() {
        Some(on_path("xcodegen").context(
            "`xcodegen` not found (the iOS project uses project.yml): brew install xcodegen",
        )?)
    } else {
        None
    };

    if let Some(xcodegen) = xcodegen {
        run_cmd(Command::new(xcodegen).arg("generate").current_dir(&project))?;
    }
    let xcodeproj = find_xcodeproj(&project)?;
    let scheme = xcodeproj
        .file_stem()
        .and_then(|s| s.to_str())
        .context("invalid .xcodeproj name")?
        .to_owned();

    let configuration = if release { "Release" } else { "Debug" };
    let (sdk, udid) = match &target {
        Target::Simulator(sim) => ("iphonesimulator", &sim.udid),
        Target::Device(device) => ("iphoneos", &device.udid),
    };
    let derived_data = app.target_dir()?.join("ios").join("DerivedData");
    let mut xcodebuild = Command::new("xcodebuild");
    xcodebuild
        .arg("-project")
        .arg(&xcodeproj)
        .args(["-scheme", &scheme, "-configuration", configuration])
        .args(["-sdk", sdk])
        .args(["-destination", &format!("id={udid}")])
        .arg("-derivedDataPath")
        .arg(&derived_data);
    if let Some(team) = &team {
        // Automatic signing: let Xcode create / update the provisioning profile and register
        // the device with the team.
        xcodebuild
            .args([
                "-allowProvisioningUpdates",
                "-allowProvisioningDeviceRegistration",
            ])
            .arg(format!("DEVELOPMENT_TEAM={team}"));
    }
    run_cmd(xcodebuild.arg("build"))?;
    let products = derived_data
        .join("Build/Products")
        .join(format!("{configuration}-{sdk}"));
    let app_bundle = find_app(&products, &scheme)?;

    match target {
        Target::Simulator(sim) => {
            run_on_simulator(app, &sim, &app_bundle, bundle_id, release, hot, attach)
        }
        Target::Device(device) => {
            run_on_device(app, &device, &app_bundle, bundle_id, release, attach)
        }
    }
}

fn run_on_simulator(
    app: &AppCrate,
    sim: &Simulator,
    app_bundle: &Path,
    bundle_id: &str,
    release: bool,
    hot: bool,
    attach: bool,
) -> Result<()> {
    sim.boot()?;
    sim.install(app_bundle)?;

    // Hot reload: the simulator shares the Mac's network, so the app connects directly.
    let server = if attach && hot && !release {
        let server = DevServer::bind()?;
        server.accept_in_background()?;
        Some(server)
    } else {
        None
    };
    let address = server.as_ref().map(|s| format!("127.0.0.1:{}", s.port()));
    let env: Vec<(&str, &str)> = address
        .as_deref()
        .map(|a| vec![(DEV_SERVER_ENV, a)])
        .unwrap_or_default();
    if !attach {
        sim.launch(bundle_id, &env)?;
        println!("→ launched {bundle_id} on {}", sim.name);
        return Ok(());
    }
    // Launching attached to the app's console streams its stdout / stderr.
    let logs = Logs::spawn(
        &mut sim.launch_command(bundle_id, &env, true),
        "the app exited",
    )?;
    println!("→ launched {bundle_id} on {}", sim.name);

    let target_dir = app.target_dir()?;
    let hot_reload = match &server {
        Some(server) => HotReload::On {
            server,
            build: Box::new(|| hot_library(app, &target_dir)),
        },
        None if release => HotReload::Off("release build"),
        None => HotReload::Off("--no-hot"),
    };
    hot::session(app, hot_reload, Some(logs))
}

fn run_on_device(
    app: &AppCrate,
    device: &Device,
    app_bundle: &Path,
    bundle_id: &str,
    release: bool,
    attach: bool,
) -> Result<()> {
    device.install(app_bundle)?;
    if !attach {
        run_cmd(&mut device.launch_command(bundle_id, false))?;
        println!("→ launched {bundle_id} on {}", device.name);
        return Ok(());
    }
    let logs = Logs::spawn(
        &mut device.launch_command(bundle_id, true),
        "the app exited",
    )?;
    println!(
        "→ launching {bundle_id} on {} (unlock the device if it's locked)",
        device.name
    );
    let why = if release {
        "release build"
    } else {
        "not supported on physical iOS devices yet"
    };
    hot::session(app, HotReload::Off(why), Some(logs))
}

/// `wanted` (a connected device, else a simulator, by name or id) if given; otherwise the
/// booted simulator, else the newest iPhone simulator.
fn pick_target(wanted: Option<&str>) -> Result<Target> {
    let devices = Device::list();
    let connected: Vec<&Device> = devices.iter().filter(|d| d.connected).collect();
    let sims = Simulator::list()?;
    let Some(wanted) = wanted else {
        let sim = Simulator::pick(&sims, None)?;
        if !connected.is_empty() {
            let names: Vec<&str> = connected.iter().map(|d| d.name.as_str()).collect();
            println!(
                "   connected iOS devices: {} (run on one with --device <name>)",
                names.join(", ")
            );
        }
        return Ok(Target::Simulator(sim));
    };
    let device = Device::find(&devices, wanted);
    if let Some(device) = device.filter(|d| d.connected) {
        return Ok(Target::Device(device.clone()));
    }
    match Simulator::pick(&sims, Some(wanted)) {
        Ok(sim) => Ok(Target::Simulator(sim)),
        Err(_) if device.is_some() => bail!(
            "iOS device `{wanted}` is not connected: plug it in (or join the same Wi-Fi), \
             unlock it and trust this Mac"
        ),
        Err(e) if connected.is_empty() => Err(e),
        Err(e) => {
            let names: Vec<&str> = connected.iter().map(|d| d.name.as_str()).collect();
            Err(e.context(format!("connected iOS devices: {}", names.join(", "))))
        }
    }
}

/// Environment variable the app's dev client reads the server address from
/// (`RuxDevClient.environmentKey`).
const DEV_SERVER_ENV: &str = "RUX_DEV_SERVER";

/// Rebuilds just the app library (debug, simulator) for hot reload; returns its path.
fn hot_library(app: &AppCrate, target_dir: &Path) -> Result<PathBuf> {
    const TARGET: &str = "aarch64-apple-ios-sim";
    let mut cmd = Command::new("cargo");
    cmd.current_dir(&app.dir)
        .args(["build", "-p", &app.package, "--lib", "--target", TARGET]);
    cmd.args(app.cargo_args(Platform::Ios));
    run_quiet(&mut cmd)?;
    Ok(target_dir
        .join(TARGET)
        .join("debug")
        .join(format!("lib{}.dylib", app.lib_name)))
}

fn find_xcodeproj(project: &Path) -> Result<PathBuf> {
    let found = with_extension(project, "xcodeproj")?;
    match found.as_slice() {
        [one] => Ok(one.clone()),
        [] => bail!("no .xcodeproj in {}", project.display()),
        _ => bail!("several .xcodeproj in {}, expected one", project.display()),
    }
}

/// `<scheme>.app` in the build products, else the only `.app` there.
fn find_app(products: &Path, scheme: &str) -> Result<PathBuf> {
    let preferred = products.join(format!("{scheme}.app"));
    if preferred.is_dir() {
        return Ok(preferred);
    }
    match with_extension(products, "app")?.as_slice() {
        [one] => Ok(one.clone()),
        _ => bail!("can't find the built .app in {}", products.display()),
    }
}

fn with_extension(dir: &Path, ext: &str) -> Result<Vec<PathBuf>> {
    let entries = std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))?;
    let mut found = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().is_some_and(|e| e == ext) {
            found.push(path);
        }
    }
    Ok(found)
}
