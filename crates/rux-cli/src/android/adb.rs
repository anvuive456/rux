//! Locating `adb` and talking to one connected device.

use std::path::PathBuf;
use std::process::Command;

use anyhow::{Result, bail};

use super::sdk::sdk_dirs;
use crate::util::{on_path, output};

/// `adb` bound to a single device serial.
pub struct Adb {
    exe: PathBuf,
    serial: String,
}

impl Adb {
    /// Picks the device to use: `device` if given (must be connected), otherwise the only
    /// connected device.
    pub fn connect(device: Option<&str>) -> Result<Self> {
        let exe = find_adb()?;
        let listing = output(Command::new(&exe).arg("devices"))?;
        let serial = pick_device(&parse_devices(&listing), device)?;
        Ok(Self { exe, serial })
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// `adb -s <serial>`.
    pub fn command(&self) -> Command {
        let mut cmd = Command::new(&self.exe);
        cmd.args(["-s", &self.serial]);
        cmd
    }

    /// `adb -s <serial> shell <args>`, returning trimmed stdout.
    pub fn shell(&self, args: &[&str]) -> Result<String> {
        output(self.command().arg("shell").args(args))
    }
}

/// `adb` from `PATH`, else from the Android SDK's `platform-tools`.
fn find_adb() -> Result<PathBuf> {
    if let Some(adb) = on_path("adb") {
        return Ok(adb);
    }
    let exe = format!("adb{}", std::env::consts::EXE_SUFFIX);
    match sdk_dirs()
        .into_iter()
        .map(|sdk| sdk.join("platform-tools").join(&exe))
        .find(|adb| adb.is_file())
    {
        Some(adb) => Ok(adb),
        None => {
            bail!("`adb` not found: install the Android SDK platform-tools and set ANDROID_HOME")
        }
    }
}

/// Serials of the devices that are ready (`adb devices` state `device`).
fn parse_devices(listing: &str) -> Vec<String> {
    listing
        .lines()
        .skip(1) // "List of devices attached"
        .filter_map(|line| {
            let (serial, state) = line.split_once('\t')?;
            (state.trim() == "device").then(|| serial.trim().to_owned())
        })
        .collect()
}

fn pick_device(serials: &[String], wanted: Option<&str>) -> Result<String> {
    match (wanted, serials) {
        (_, []) => bail!(
            "no Android device connected: start an emulator or plug in a device (see `adb devices`)"
        ),
        (Some(w), _) if serials.iter().any(|s| s == w) => Ok(w.to_owned()),
        (Some(w), _) => bail!(
            "device `{w}` is not connected; connected: {}",
            serials.join(", ")
        ),
        (None, [only]) => Ok(only.clone()),
        (None, _) => bail!(
            "several Android devices connected, pick one with --device: {}",
            serials.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_devices, pick_device};

    #[test]
    fn parses_ready_devices_only() {
        let listing = "List of devices attached\n\
                       emulator-5554\tdevice\n\
                       R58M123\tunauthorized\n\
                       192.168.1.5:5555\tdevice\n";
        assert_eq!(
            parse_devices(listing),
            ["emulator-5554", "192.168.1.5:5555"]
        );
        assert!(parse_devices("List of devices attached\n").is_empty());
    }

    #[test]
    fn picks_device() {
        let one = vec!["a".to_owned()];
        let two = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(pick_device(&one, None).unwrap(), "a");
        assert_eq!(pick_device(&two, Some("b")).unwrap(), "b");
        assert!(pick_device(&two, None).is_err());
        assert!(pick_device(&two, Some("c")).is_err());
        assert!(pick_device(&[], None).is_err());
    }
}
