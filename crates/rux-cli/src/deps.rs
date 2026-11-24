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

use crate::Platform;
use crate::app::AppCrate;

pub struct Deps {
    /// `cargo metadata` packages in the app's dependency closure (the app included).
    packages: Vec<Value>,
}

impl Deps {
    fn load(app: &AppCrate, platform: Platform, target: &str) -> Result<Self> {
        let mut command = Command::new("cargo");
        command
            .args([
                "metadata",
                "--format-version=1",
                "--filter-platform",
                target,
            ])
            .current_dir(&app.dir);
        command.args(app.cargo_args(platform));
        let out = command.output().context("running cargo metadata")?;
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

        let nodes: BTreeMap<&str, &Value> = meta["resolve"]["nodes"]
            .as_array()
            .context("no dependency graph")?
            .iter()
            .filter_map(|n| Some((n["id"].as_str()?, n)))
            .collect();
        let mut seen = BTreeSet::from([app_id]);
        let mut queue = VecDeque::from([app_id]);
        while let Some(id) = queue.pop_front() {
            let deps = nodes[id]["deps"]
                .as_array()
                .context("no dependency edges")?;
            for dep in deps.iter().filter_map(|dep| {
                let kinds = dep["dep_kinds"].as_array()?;
                if kinds.iter().any(|kind| kind["kind"].is_null()) {
                    dep["pkg"].as_str()
                } else {
                    None
                }
            }) {
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

    pub fn for_build(app: &AppCrate, platform: Platform, targets: &[&str]) -> Result<Self> {
        // All ABIs / variants of the platform share one native source project.
        let mut packages = BTreeMap::new();
        for target in targets {
            let deps = Self::load(app, platform, target)?;
            for package in deps.packages {
                packages.insert(package["id"].as_str().unwrap().to_owned(), package);
            }
        }
        Ok(Self {
            packages: packages.into_values().collect(),
        })
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
        let rux = self.rux()?;
        let dir = package_dir(rux)?.join(rel);
        if !dir.join(marker).is_file() {
            bail!(
                "the `rux` crate at {} has no {rel}/{marker}",
                package_dir(rux)?.display()
            );
        }
        Ok(dir)
    }

    fn rux(&self) -> Result<&Value> {
        self.packages
            .iter()
            .find(|p| p["name"] == "rux")
            .context("the app doesn't depend on the `rux` crate")
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
        let deps = Deps::for_build(&app, Platform::Ios, &["aarch64-apple-ios"]).unwrap();
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

    #[test]
    fn resolves_only_the_selected_backend() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let app = AppCrate::load(&root.join("examples/counter")).unwrap();
        for (platform, target, expected_backend) in [
            (Platform::Ios, "aarch64-apple-ios", "rux-ios"),
            (Platform::Android, "aarch64-linux-android", "rux-android"),
        ] {
            let deps = Deps::for_build(&app, platform, &[target]).unwrap();
            let backends: Vec<_> = deps
                .packages()
                .iter()
                .filter_map(|pkg| pkg["name"].as_str())
                .filter(|name| matches!(*name, "rux-ios" | "rux-android"))
                .collect();
            assert_eq!(backends, [expected_backend]);
        }
    }
}
