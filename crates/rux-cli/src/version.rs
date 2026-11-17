//! The app version, `x.y.z+n` in the app's Cargo.toml.

use anyhow::{Result, bail};

/// App version from `version = "x.y.z+n"` in the app's Cargo.toml, like Flutter's pubspec:
/// `x.y.z` is the version name (Android `versionName`, iOS `CFBundleShortVersionString`) and
/// `n` is the build number (Android `versionCode`, iOS `CFBundleVersion`). `+n` defaults to 1.
#[derive(Debug, PartialEq, Eq)]
pub struct AppVersion {
    pub name: String,
    pub code: u32,
}

impl AppVersion {
    /// Google Play's upper bound for `versionCode`.
    const MAX_CODE: u32 = 2_100_000_000;

    pub fn parse(raw: &str) -> Result<Self> {
        let (name, code) = match raw.split_once('+') {
            Some((name, code)) => (name, Some(code)),
            None => (raw, None),
        };
        let parts: Vec<&str> = name.split('.').collect();
        if parts.len() != 3
            || parts
                .iter()
                .any(|p| p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()))
        {
            bail!(
                "`{raw}`: expected `x.y.z+n` (e.g. `1.2.3+45`); the version name must be three \
                 numbers (iOS does not allow pre-release suffixes like `-beta`)"
            );
        }
        let code = match code {
            None => 1,
            Some(c) => match c.parse::<u32>() {
                Ok(n) if (1..=Self::MAX_CODE).contains(&n) => n,
                _ => bail!(
                    "`{raw}`: the build number after `+` must be an integer between 1 and {}",
                    Self::MAX_CODE
                ),
            },
        };
        Ok(Self {
            name: name.to_owned(),
            code,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::AppVersion;

    fn v(name: &str, code: u32) -> AppVersion {
        AppVersion {
            name: name.into(),
            code,
        }
    }

    #[test]
    fn parses_name_and_build_number() {
        assert_eq!(AppVersion::parse("1.2.3+45").unwrap(), v("1.2.3", 45));
        assert_eq!(AppVersion::parse("0.1.0").unwrap(), v("0.1.0", 1));
    }

    #[test]
    fn rejects_invalid_versions() {
        for bad in [
            "1.2",
            "1.2.3.4",
            "1.2.x",
            "1.2.3-beta+1",
            "1.2.3+",
            "1.2.3+0",
            "1.2.3+abc",
            "1.2.3+2100000001",
        ] {
            assert!(AppVersion::parse(bad).is_err(), "{bad} should be rejected");
        }
    }
}
