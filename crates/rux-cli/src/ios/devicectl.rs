//! Physical iOS devices via `xcrun devicectl` (Xcode 15+).

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::util::run;

#[derive(Debug, Clone)]
pub struct Device {
    /// CoreDevice identifier, what `devicectl` takes.
    pub identifier: String,
    /// Hardware UDID, what `xcodebuild -destination id=…` takes.
    pub udid: String,
    pub name: String,
    /// Reachable (USB or network), so apps can be installed and launched.
    pub connected: bool,
}

impl Device {
    /// Lists the iOS devices paired with this Mac (none if `devicectl` is unavailable).
    pub fn list() -> Vec<Self> {
        let json = std::env::temp_dir().join(format!("rux-devicectl-{}.json", std::process::id()));
        let listed = Command::new("xcrun")
            .args(["devicectl", "list", "devices", "-q", "--json-output"])
            .arg(&json)
            .output()
            .is_ok_and(|o| o.status.success());
        let devices = listed
            .then(|| std::fs::read_to_string(&json).ok())
            .flatten()
            .and_then(|json| parse(&json).ok())
            .unwrap_or_default();
        let _ = std::fs::remove_file(&json);
        devices
    }

    /// The device whose identifier, UDID or name is `wanted`, preferring connected ones.
    pub fn find<'a>(devices: &'a [Self], wanted: &str) -> Option<&'a Self> {
        devices
            .iter()
            .filter(|d| {
                d.identifier.eq_ignore_ascii_case(wanted)
                    || d.udid.eq_ignore_ascii_case(wanted)
                    || d.name.eq_ignore_ascii_case(wanted)
            })
            .max_by_key(|d| d.connected)
    }

    pub fn install(&self, app: &Path) -> Result<()> {
        run(Command::new("xcrun")
            .args([
                "devicectl",
                "device",
                "install",
                "app",
                "--device",
                &self.identifier,
            ])
            .arg(app))
    }

    /// `devicectl` launching (or relaunching) the app; with `console`, it stays attached to the
    /// app's stdout / stderr until the app exits.
    pub fn launch_command(&self, bundle_id: &str, console: bool) -> Command {
        let mut cmd = Command::new("xcrun");
        cmd.args([
            "devicectl",
            "device",
            "process",
            "launch",
            "--terminate-existing",
        ])
        .args(["--device", &self.identifier]);
        if console {
            cmd.arg("--console");
        }
        cmd.arg(bundle_id);
        cmd
    }
}

fn parse(json: &str) -> Result<Vec<Device>> {
    #[derive(Deserialize)]
    struct Output {
        result: Listing,
    }
    #[derive(Deserialize)]
    struct Listing {
        devices: Vec<Entry>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Entry {
        identifier: String,
        connection_properties: Connection,
        device_properties: Props,
        hardware_properties: Hardware,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Connection {
        tunnel_state: Option<String>,
    }
    #[derive(Deserialize)]
    struct Props {
        name: String,
    }
    #[derive(Deserialize)]
    struct Hardware {
        platform: Option<String>,
        reality: Option<String>,
        udid: Option<String>,
    }

    let output: Output = serde_json::from_str(json).context("parsing `devicectl` output")?;
    Ok(output
        .result
        .devices
        .into_iter()
        .filter(|d| {
            d.hardware_properties.platform.as_deref() == Some("iOS")
                && d.hardware_properties.reality.as_deref() == Some("physical")
        })
        .map(|d| Device {
            udid: d
                .hardware_properties
                .udid
                .unwrap_or_else(|| d.identifier.clone()),
            identifier: d.identifier,
            name: d.device_properties.name,
            // `disconnected`: reachable, the tunnel is opened on demand.
            connected: d
                .connection_properties
                .tunnel_state
                .is_some_and(|s| s != "unavailable"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{Device, parse};

    const LISTING: &str = r#"{"result": {"devices": [
        {"identifier": "AAAA", "connectionProperties": {"tunnelState": "unavailable"},
         "deviceProperties": {"name": "iPhone"},
         "hardwareProperties": {"platform": "iOS", "reality": "physical", "udid": "0001-A"}},
        {"identifier": "BBBB", "connectionProperties": {"tunnelState": "connected"},
         "deviceProperties": {"name": "iPhone"},
         "hardwareProperties": {"platform": "iOS", "reality": "physical", "udid": "0001-B"}},
        {"identifier": "CCCC", "connectionProperties": {"tunnelState": "connected"},
         "deviceProperties": {"name": "Watch"},
         "hardwareProperties": {"platform": "watchOS", "reality": "physical", "udid": "0001-C"}}
    ]}}"#;

    #[test]
    fn lists_physical_ios_devices() {
        let devices = parse(LISTING).unwrap();
        assert_eq!(devices.len(), 2);
        assert!(!devices[0].connected);
        assert!(devices[1].connected);
        assert_eq!(devices[1].udid, "0001-B");
    }

    #[test]
    fn finds_by_name_identifier_or_udid_preferring_connected() {
        let devices = parse(LISTING).unwrap();
        let find = |w| Device::find(&devices, w).map(|d| d.identifier.as_str());
        assert_eq!(find("iphone"), Some("BBBB"));
        assert_eq!(find("AAAA"), Some("AAAA"));
        assert_eq!(find("0001-a"), Some("AAAA"));
        assert_eq!(find("Watch"), None);
    }
}
