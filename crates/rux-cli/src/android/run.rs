//! `rux run android`: builds the Rust library + APK, installs it with Gradle, launches it and
//! stays attached (logcat + hot reload in debug builds) unless detached.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use anyhow::{Result, bail};

use crate::hot::{self, DevServer, HotReload, Logs};

use super::adb::Adb;
use crate::app::AppCrate;
use crate::util::{on_path, output, run as run_cmd};

/// Intent extra the app's dev client reads the server address from (`RuxDevClient.EXTRA`).
const DEV_SERVER_EXTRA: &str = "rux.devServer";

pub fn run(
    app: &AppCrate,
    release: bool,
    abis: &[String],
    device: Option<&str>,
    hot: bool,
    attach: bool,
) -> Result<()> {
    // Fail on config / device problems before the (slow) builds.
    let app_id = app.meta.android_app_id()?;
    let adb = Adb::connect(device)?;
    let device_abi = adb.shell(&["getprop", "ro.product.cpu.abi"])?;
    println!("→ device {} ({device_abi})", adb.serial());

    // Only build the device's ABI unless told otherwise: faster, and right for emulators.
    let abis = if abis.is_empty() {
        vec![device_abi.clone()]
    } else {
        abis.to_vec()
    };
    super::build(app, release, &abis)?;

    let project = super::project_dir(app)?;
    let task = if release {
        "installRelease"
    } else {
        "installDebug"
    };
    let mut cmd = gradle(&project)?;
    cmd.current_dir(&project)
        .arg(task)
        .env("ANDROID_SERIAL", adb.serial());
    run_cmd(&mut cmd)?;

    // Hot reload: the app connects to our dev server through `adb reverse`.
    let server = if attach && hot && !release {
        let server = DevServer::bind()?;
        let port = format!("tcp:{}", server.port());
        output(adb.command().args(["reverse", &port, &port]))?;
        server.accept_in_background()?;
        Some(server)
    } else {
        None
    };
    let address = server.as_ref().map(|s| format!("127.0.0.1:{}", s.port()));

    let activity = launcher_activity(&adb, app_id)?;
    // -S: restart the app if it's running, so it gets this launch's intent.
    let mut start = vec!["am", "start", "-S", "-n", &activity];
    if let Some(address) = &address {
        start.extend(["--es", DEV_SERVER_EXTRA, address]);
    }
    let out = adb.shell(&start)?;
    if out.contains("Error") {
        bail!("launching {activity} failed:\n{out}");
    }
    println!("→ launched {activity}");

    if !attach {
        return Ok(());
    }
    let logs = app_logs(&adb, app_id);
    let target_dir = app.target_dir()?;
    let hot_reload = match &server {
        Some(server) => HotReload::On {
            server,
            build: Box::new(|| super::build::hot_library(app, &device_abi, &target_dir)),
        },
        None if release => HotReload::Off("release build"),
        None => HotReload::Off("--no-hot"),
    };
    let result = hot::session(app, hot_reload, logs);
    if let Some(server) = &server {
        let port = format!("tcp:{}", server.port());
        let _ = output(adb.command().args(["reverse", "--remove", &port]));
    }
    result
}

/// Streams the app process's logcat output (including Rust's stdout / stderr, which
/// `rux-android` sends to logcat).
fn app_logs(adb: &Adb, app_id: &str) -> Option<Logs> {
    // The process may take a moment to start after `am start`.
    let deadline = Instant::now() + Duration::from_secs(10);
    let pid = loop {
        match adb.shell(&["pidof", "-s", app_id]) {
            Ok(pid) if !pid.is_empty() => break pid,
            _ if Instant::now() > deadline => {
                println!("   ⚠ {app_id} is not running, so no logs");
                return None;
            }
            _ => std::thread::sleep(Duration::from_millis(200)),
        }
    };
    let pid = format!("--pid={pid}");
    match Logs::spawn(
        adb.command().args(["logcat", "-v", "tag", &pid]),
        "logcat stopped",
    ) {
        Ok(logs) => Some(logs),
        Err(e) => {
            println!("   ⚠ can't stream the app's logs: {e:#}");
            None
        }
    }
}

/// The project's Gradle wrapper (which reuses the Gradle in `~/.gradle/wrapper/dists` or
/// downloads it once) if present, else `gradle` from `PATH`. Runs on Android Studio's bundled
/// JDK when no JDK is configured (see `java`).
fn gradle(project: &Path) -> Result<Command> {
    let wrapper = project.join(if cfg!(windows) {
        "gradlew.bat"
    } else {
        "gradlew"
    });
    let mut cmd = if wrapper.is_file() {
        Command::new(wrapper)
    } else if let Some(gradle) = on_path("gradle") {
        Command::new(gradle)
    } else {
        bail!(
            "no Gradle wrapper (gradlew) in {} and `gradle` is not on PATH",
            project.display()
        );
    };
    super::java::configure(&mut cmd);
    Ok(cmd)
}

/// `<package>/<activity>` of the installed app's launcher activity.
fn launcher_activity(adb: &Adb, app_id: &str) -> Result<String> {
    let out = adb.shell(&[
        "cmd",
        "package",
        "resolve-activity",
        "--brief",
        "-c",
        "android.intent.category.LAUNCHER",
        app_id,
    ])?;
    match out.lines().last().map(str::trim) {
        Some(component) if component.contains('/') => Ok(component.to_owned()),
        _ => bail!("no launcher activity found for {app_id}:\n{out}"),
    }
}
