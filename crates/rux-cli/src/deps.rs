//! The app's resolved dependencies (`cargo metadata`): plugins with native APIs, and the `rux`
//! crate, which ships the Android / iOS runtimes.
//!
//! The runtimes are taken from the `rux` crate cargo resolved for the app, wherever it comes
//! from (crates.io, git, a path), so they always match the Rust side.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::app::AppCrate;

pub struct Deps {
    /// `cargo metadata` packages in the app's dependency closure (the app included).
    packages: Vec<Value>,
}

impl Deps {
    pub fn load(app: &AppCrate) -> Result<Self> {
        let out = Command::new("cargo")
            .args(["metadata", "--format-version=1"])
            .current_dir(&app.dir)
            .output()
            .context("running cargo metadata")?;
        if !out.status.success() {
            bail!(
                "cargo metadata failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        let meta: Value = serde_json::from_slice(&out.stdout).context("parsing cargo metadata")?;
        let packages = meta["packages"].as_array().context("no packages")?;

        let manifest = std::fs::canonicalize(app.dir.join("Cargo.toml"))?;
        let app_id = packages
            .iter()
            .find(|p| {
                p["manifest_path"]
                    .as_str()
                    .and_then(|m| std::fs::canonicalize(m).ok())
                    .is_some_and(|m| m == manifest)
            })
            .and_then(|p| p["id"].as_str())
            .context("app package not found in cargo metadata")?;

        let nodes: BTreeMap<&str, Vec<&str>> = meta["resolve"]["nodes"]
            .as_array()
            .context("no dependency graph")?
            .iter()
            .filter_map(|n| {
                let deps = n["deps"]
                    .as_array()?
                    .iter()
                    .filter_map(|d| d["pkg"].as_str())
                    .collect();
                Some((n["id"].as_str()?, deps))
            })
            .collect();
        let mut seen = BTreeSet::from([app_id]);
        let mut queue = VecDeque::from([app_id]);
        while let Some(id) = queue.pop_front() {
            for &dep in nodes.get(id).into_iter().flatten() {
                if seen.insert(dep) {
                    queue.push_back(dep);
                }
            }
        }

        let packages = packages
            .iter()
            .filter(|p| p["id"].as_str().is_some_and(|id| seen.contains(id)))
            .cloned()
            .collect();
        Ok(Self { packages })
    }

    pub fn packages(&self) -> &[Value] {
        &self.packages
    }

    /// The Android runtime library (Gradle project) shipped by the `rux` crate.
    pub fn android_runtime(&self) -> Result<PathBuf> {
        self.runtime("platforms/android/rux-runtime", "build.gradle.kts")
    }

    /// The iOS runtime (Swift package) shipped by the `rux` crate.
    pub fn ios_runtime(&self) -> Result<PathBuf> {
        self.runtime("platforms/ios", "Package.swift")
    }

    fn runtime(&self, rel: &str, marker: &str) -> Result<PathBuf> {
        let rux = self
            .packages
            .iter()
            .find(|p| p["name"] == "rux")
            .context("the app doesn't depend on the `rux` crate")?;
        let dir = package_dir(rux)?.join(rel);
        if !dir.join(marker).is_file() {
            bail!(
                "the `rux` crate at {} has no {rel}/{marker}",
                package_dir(rux)?.display()
            );
        }
        Ok(dir)
    }
}

/// Directory of a `cargo metadata` package.
pub fn package_dir(pkg: &Value) -> Result<PathBuf> {
    Path::new(pkg["manifest_path"].as_str().unwrap_or_default())
        .parent()
        .map(Path::to_path_buf)
        .context("package directory")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_runtimes_in_the_resolved_rux_crate() {
        let root = std::fs::canonicalize(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).unwrap();
        let app = AppCrate::load(&root.join("examples/counter")).unwrap();
        let deps = Deps::load(&app).unwrap();
        let canon = |p: PathBuf| std::fs::canonicalize(p).unwrap();
        assert_eq!(
            canon(deps.android_runtime().unwrap()),
            root.join("platforms/android/rux-runtime")
        );
        assert_eq!(
            canon(deps.ios_runtime().unwrap()),
            root.join("platforms/ios")
        );
        assert!(
            deps.packages()
                .iter()
                .any(|p| p["name"] == "rux-device-info")
        );
    }
}
