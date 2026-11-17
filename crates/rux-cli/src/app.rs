//! The app crate: its `Cargo.toml`, `[package.metadata.rux]` and cargo's target directory.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::app_id::{validate_android_app_id, validate_ios_bundle_id};
use crate::version::AppVersion;

#[derive(Deserialize)]
struct Manifest {
    package: Package,
    lib: Option<Lib>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    version: String,
    #[serde(default)]
    metadata: Metadata,
}

#[derive(Deserialize, Default)]
struct Metadata {
    #[serde(default)]
    rux: RuxMeta,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct RuxMeta {
    pub android_project: Option<PathBuf>,
    pub ios_project: Option<PathBuf>,
    /// Exact Android NDK version to build with (installed if missing). Default: the newest
    /// installed NDK, else `sdk::DEFAULT_NDK_VERSION`.
    pub android_ndk_version: Option<String>,
    /// Application id shared by both platforms (Android `applicationId`, iOS bundle id).
    app_id: Option<String>,
    /// Overrides `app-id` on Android.
    android_app_id: Option<String>,
    /// Overrides `app-id` on iOS.
    ios_bundle_id: Option<String>,
}

impl RuxMeta {
    /// Android `applicationId`: `android-app-id`, else `app-id`.
    pub fn android_app_id(&self) -> Result<&str> {
        let (key, id) = Self::pick("android-app-id", &self.android_app_id, &self.app_id)?;
        validate_android_app_id(id).with_context(|| format!("invalid `{key}`"))?;
        Ok(id)
    }

    /// iOS bundle identifier: `ios-bundle-id`, else `app-id`.
    pub fn ios_bundle_id(&self) -> Result<&str> {
        let (key, id) = Self::pick("ios-bundle-id", &self.ios_bundle_id, &self.app_id)?;
        validate_ios_bundle_id(id).with_context(|| format!("invalid `{key}`"))?;
        Ok(id)
    }

    fn pick<'a>(
        key: &'static str,
        specific: &'a Option<String>,
        shared: &'a Option<String>,
    ) -> Result<(&'static str, &'a str)> {
        match (specific, shared) {
            (Some(id), _) => Ok((key, id)),
            (None, Some(id)) => Ok(("app-id", id)),
            (None, None) => bail!(
                "missing `app-id` (or `{key}`) in [package.metadata.rux], \
                 e.g. app-id = \"com.example.myapp\""
            ),
        }
    }
}

#[derive(Deserialize)]
struct Lib {
    name: Option<String>,
}

pub struct AppCrate {
    pub dir: PathBuf,
    pub package: String,
    pub lib_name: String,
    pub version: AppVersion,
    pub meta: RuxMeta,
}

impl AppCrate {
    pub fn load(dir: &Path) -> Result<Self> {
        // Absolute, so paths stay valid when commands run with `current_dir(&self.dir)`.
        let dir =
            std::path::absolute(dir).with_context(|| format!("resolving {}", dir.display()))?;
        let path = dir.join("Cargo.toml");
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let m: Manifest =
            toml::from_str(&raw).with_context(|| format!("parsing {}", path.display()))?;
        let lib_name = m
            .lib
            .and_then(|l| l.name)
            .unwrap_or_else(|| m.package.name.replace('-', "_"));
        let version = AppVersion::parse(&m.package.version)
            .with_context(|| format!("invalid `version` in {}", path.display()))?;
        Ok(Self {
            dir,
            package: m.package.name,
            lib_name,
            version,
            meta: m.package.metadata.rux,
        })
    }

    pub fn target_dir(&self) -> Result<PathBuf> {
        #[derive(Deserialize)]
        struct Meta {
            target_directory: PathBuf,
        }
        let out = Command::new("cargo")
            .args(["metadata", "--format-version=1", "--no-deps"])
            .current_dir(&self.dir)
            .output()
            .context("running cargo metadata")?;
        if !out.status.success() {
            bail!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let meta: Meta = serde_json::from_slice(&out.stdout).context("parsing cargo metadata")?;
        Ok(meta.target_directory)
    }
}

#[cfg(test)]
mod tests {
    use super::RuxMeta;

    #[test]
    fn platform_ids_fall_back_to_app_id() {
        let meta = RuxMeta {
            app_id: Some("com.example.app".into()),
            ios_bundle_id: Some("com.example.app-ios".into()),
            ..Default::default()
        };
        assert_eq!(meta.android_app_id().unwrap(), "com.example.app");
        assert_eq!(meta.ios_bundle_id().unwrap(), "com.example.app-ios");
        assert!(RuxMeta::default().android_app_id().is_err());
    }
}
