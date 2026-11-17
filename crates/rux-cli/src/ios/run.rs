//! `rux run ios`: builds the xcframework + app, installs it on a simulator and launches it.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use super::simctl::Simulator;
use crate::app::AppCrate;
use crate::hot::{self, DevServer};
use crate::util::{on_path, run as run_cmd, run_quiet};

pub fn run(app: &AppCrate, release: bool, device: Option<&str>, hot: bool) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("running on iOS requires macOS + Xcode");
    }
    // Fail on config / tool / device problems before the (slow) builds.
    let bundle_id = app.meta.ios_bundle_id()?;
    let project =
        super::project_dir(app).context("missing `ios-project` in [package.metadata.rux]")?;
    // The .xcodeproj is generated (and gitignored) when the project uses XcodeGen.
    let xcodegen = if project.join("project.yml").is_file() {
        Some(on_path("xcodegen").context(
            "`xcodegen` not found (the iOS project uses project.yml): brew install xcodegen",
        )?)
    } else {
        None
    };
    let sim = Simulator::pick(&Simulator::list()?, device)?;
    println!("→ simulator {} ({})", sim.name, sim.udid);

    super::build(app, release)?;

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
    let derived_data = app.target_dir()?.join("ios").join("DerivedData");
    run_cmd(
        Command::new("xcodebuild")
            .arg("-project")
            .arg(&xcodeproj)
            .args(["-scheme", &scheme, "-configuration", configuration])
            .args(["-sdk", "iphonesimulator"])
            .args(["-destination", &format!("id={}", sim.udid)])
            .arg("-derivedDataPath")
            .arg(&derived_data)
            .arg("build"),
    )?;
    let products = derived_data
        .join("Build/Products")
        .join(format!("{configuration}-iphonesimulator"));
    let app_bundle = find_app(&products, &scheme)?;

    sim.boot()?;
    sim.install(&app_bundle)?;

    // Hot reload: the simulator shares the Mac's network, so the app connects directly.
    let server = if hot && !release {
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
    sim.launch(bundle_id, &env)?;
    println!("→ launched {bundle_id} on {}", sim.name);

    let Some(server) = server else {
        return Ok(());
    };
    let target_dir = app.target_dir()?;
    hot::session(app, &server, || hot_library(app, &target_dir))
}

/// Environment variable the app's dev client reads the server address from
/// (`RuxDevClient.environmentKey`).
const DEV_SERVER_ENV: &str = "RUX_DEV_SERVER";

/// Rebuilds just the app library (debug, simulator) for hot reload; returns its path.
fn hot_library(app: &AppCrate, target_dir: &Path) -> Result<PathBuf> {
    const TARGET: &str = "aarch64-apple-ios-sim";
    run_quiet(Command::new("cargo").current_dir(&app.dir).args([
        "build",
        "-p",
        &app.package,
        "--lib",
        "--target",
        TARGET,
    ]))?;
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
