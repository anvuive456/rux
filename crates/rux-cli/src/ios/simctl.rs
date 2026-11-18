//! iOS simulators via `xcrun simctl`.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::util::{output, run};

#[derive(Debug, Clone)]
pub struct Simulator {
    pub udid: String,
    pub name: String,
    booted: bool,
    /// iOS runtime version, e.g. `[18, 2]`.
    runtime: Vec<u32>,
}

impl Simulator {
    /// Lists the available iOS simulators.
    pub fn list() -> Result<Vec<Self>> {
        let json =
            output(Command::new("xcrun").args(["simctl", "list", "devices", "available", "-j"]))?;
        parse(&json)
    }

    /// Picks `wanted` (UDID or name) if given; otherwise the booted simulator, else the iPhone
    /// with the newest iOS runtime.
    pub fn pick(sims: &[Self], wanted: Option<&str>) -> Result<Self> {
        let found = match wanted {
            Some(w) => sims
                .iter()
                .filter(|s| s.udid == w || s.name.eq_ignore_ascii_case(w))
                .max_by_key(|s| (s.booted, s.runtime.clone())),
            None => sims
                .iter()
                .max_by_key(|s| (s.booted, s.name.starts_with("iPhone"), s.runtime.clone())),
        };
        match (found, wanted) {
            (Some(sim), _) => Ok(sim.clone()),
            (None, _) if sims.is_empty() => bail!(
                "no iOS simulator available: install an iOS runtime in Xcode > Settings > Components"
            ),
            (None, w) => {
                let names: Vec<&str> = sims.iter().map(|s| s.name.as_str()).collect();
                bail!(
                    "no simulator named `{}`; available: {}",
                    w.unwrap_or_default(),
                    names.join(", ")
                )
            }
        }
    }

    /// Boots the simulator (no-op if already booted), waits for it and shows Simulator.app.
    pub fn boot(&self) -> Result<()> {
        run(Command::new("xcrun").args(["simctl", "bootstatus", &self.udid, "-b"]))?;
        run(Command::new("open").args(["-a", "Simulator"]))
    }

    pub fn install(&self, app: &Path) -> Result<()> {
        run(Command::new("xcrun")
            .args(["simctl", "install", &self.udid])
            .arg(app))
    }

    /// `simctl` launching (or relaunching) the app; `env` is passed to the app's process. With
    /// `console`, it stays attached to the app's stdout / stderr until the app exits.
    pub fn launch_command(&self, bundle_id: &str, env: &[(&str, &str)], console: bool) -> Command {
        let mut cmd = Command::new("xcrun");
        cmd.args(["simctl", "launch", "--terminate-running-process"]);
        if console {
            // A pty, so the app's output is line-buffered (shows up right away).
            cmd.arg("--console-pty");
        }
        cmd.args([&self.udid, bundle_id]);
        for (key, value) in env {
            // simctl forwards SIMCTL_CHILD_* variables to the launched app.
            cmd.env(format!("SIMCTL_CHILD_{key}"), value);
        }
        cmd
    }

    /// Launches (or relaunches) the app, without attaching to it.
    pub fn launch(&self, bundle_id: &str, env: &[(&str, &str)]) -> Result<()> {
        run(&mut self.launch_command(bundle_id, env, false))
    }
}

fn parse(json: &str) -> Result<Vec<Simulator>> {
    #[derive(Deserialize)]
    struct Listing {
        devices: BTreeMap<String, Vec<Device>>,
    }
    #[derive(Deserialize)]
    struct Device {
        udid: String,
        name: String,
        state: String,
    }

    let listing: Listing = serde_json::from_str(json).context("parsing `simctl list` output")?;
    let mut sims = Vec::new();
    for (runtime, devices) in listing.devices {
        // e.g. com.apple.CoreSimulator.SimRuntime.iOS-18-2 (skips watchOS, tvOS, visionOS).
        let Some(version) = runtime
            .rsplit('.')
            .next()
            .and_then(|r| r.strip_prefix("iOS-"))
        else {
            continue;
        };
        let runtime: Vec<u32> = version.split('-').filter_map(|n| n.parse().ok()).collect();
        sims.extend(devices.into_iter().map(|d| Simulator {
            booted: d.state == "Booted",
            udid: d.udid,
            name: d.name,
            runtime: runtime.clone(),
        }));
    }
    Ok(sims)
}

#[cfg(test)]
mod tests {
    use super::{Simulator, parse};

    const LISTING: &str = r#"{"devices": {
        "com.apple.CoreSimulator.SimRuntime.iOS-17-5": [
            {"udid": "OLD", "name": "iPhone 15", "state": "Shutdown"}
        ],
        "com.apple.CoreSimulator.SimRuntime.iOS-18-2": [
            {"udid": "IPAD", "name": "iPad Air", "state": "Shutdown"},
            {"udid": "NEW", "name": "iPhone 16", "state": "Shutdown"}
        ],
        "com.apple.CoreSimulator.SimRuntime.watchOS-11-2": [
            {"udid": "WATCH", "name": "Apple Watch", "state": "Booted"}
        ]
    }}"#;

    fn pick(json: &str, wanted: Option<&str>) -> String {
        Simulator::pick(&parse(json).unwrap(), wanted).unwrap().udid
    }

    #[test]
    fn defaults_to_newest_iphone_and_skips_other_platforms() {
        assert_eq!(parse(LISTING).unwrap().len(), 3);
        assert_eq!(pick(LISTING, None), "NEW");
    }

    #[test]
    fn prefers_booted_simulator() {
        let json = LISTING.replace(
            r#""udid": "OLD", "name": "iPhone 15", "state": "Shutdown""#,
            r#""udid": "OLD", "name": "iPhone 15", "state": "Booted""#,
        );
        assert_eq!(pick(&json, None), "OLD");
    }

    #[test]
    fn picks_by_name_or_udid() {
        assert_eq!(pick(LISTING, Some("ipad air")), "IPAD");
        assert_eq!(pick(LISTING, Some("OLD")), "OLD");
        assert!(Simulator::pick(&parse(LISTING).unwrap(), Some("Pixel")).is_err());
        assert!(Simulator::pick(&[], None).is_err());
    }
}
