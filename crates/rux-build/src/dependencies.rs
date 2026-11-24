use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    #[serde(default)]
    dependencies: Dependencies,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Dependencies {
    android: BTreeMap<String, String>,
    ios: BTreeMap<String, SwiftPackage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SwiftPackage {
    url: String,
    version: String,
    product: String,
}

impl Dependencies {
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(input) => toml::from_str::<Config>(&input)
                .map(|config| config.dependencies)
                .with_context(|| format!("parsing {}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn android_catalog(&self) -> Result<String> {
        let mut versions = String::from("[versions]\nagp = \"9.5.0\"\n");
        let mut libraries = String::from("\n[libraries]\n");
        let mut aliases = BTreeSet::new();
        for (alias, coordinate) in &self.android {
            let normalized = alias.replace(['_', '.'], "-");
            let segments: Vec<_> = normalized.split('-').collect();
            ensure!(
                alias.len() >= 2
                    && alias.starts_with(|c: char| c.is_ascii_lowercase())
                    && segments.iter().all(|segment| {
                        !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_alphanumeric())
                    })
                    && !matches!(segments[0], "bundles" | "plugins" | "versions")
                    && !matches!(normalized.as_str(), "class" | "extensions" | "convention"),
                "invalid Gradle catalog library alias `{alias}` in [dependencies.android]"
            );
            ensure!(
                aliases.insert(normalized),
                "duplicate Gradle catalog library alias `{alias}` in [dependencies.android]"
            );

            let coordinate_parts: Vec<_> = coordinate.split(':').collect();
            let [group, artifact, version] = coordinate_parts.as_slice() else {
                anyhow::bail!("Android dependency `{alias}` must use group:artifact:version");
            };
            ensure!(
                !coordinate.contains('@')
                    && coordinate_parts
                        .iter()
                        .all(|part| !part.is_empty() && !part.contains(char::is_whitespace)),
                "Android dependency `{alias}` must use non-empty group:artifact:version"
            );
            let version_key = quote(&format!("dep-{alias}"));
            writeln!(versions, "{version_key} = {}", quote(version)).unwrap();
            writeln!(
                libraries,
                "{} = {{ module = {}, version.ref = {version_key} }}",
                quote(alias),
                quote(&format!("{group}:{artifact}")),
            )
            .unwrap();
        }

        Ok(versions
            + &libraries
            + "\n[plugins]\nandroid-application = { id = \"com.android.application\", version.ref = \"agp\" }\nandroid-library = { id = \"com.android.library\", version.ref = \"agp\" }\n")
    }

    pub fn ios_packages(&self) -> Result<(String, String)> {
        if self.ios.is_empty() {
            return Ok((String::new(), String::new()));
        }

        let mut packages = String::from("\npackages:\n");
        let mut dependencies = String::new();
        for (alias, package) in &self.ios {
            ensure!(
                !alias.trim().is_empty() && !alias.eq_ignore_ascii_case("RuxRuntime"),
                "invalid Swift package alias `{alias}` in [dependencies.ios]"
            );
            ensure!(
                [&package.url, &package.version, &package.product]
                    .iter()
                    .all(|value| !value.trim().is_empty()),
                "iOS dependency `{alias}` requires a url, version and product"
            );
            let alias = quote(alias);
            writeln!(
                packages,
                "  {alias}:\n    url: {}\n    exactVersion: {}",
                quote(&package.url),
                quote(&package.version),
            )
            .unwrap();
            writeln!(
                dependencies,
                "      - package: {alias}\n        product: {}",
                quote(&package.product),
            )
            .unwrap();
        }
        Ok((packages, dependencies))
    }
}

fn quote(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_native_dependencies_for_each_platform() {
        let config: Config = toml::from_str(
            r#"
            [dependencies.android]
            okhttp = "com.squareup.okhttp3:okhttp:4.12.0"

            [dependencies.ios]
            Client = { url = "https://example.com/client.git", version = "1.2.3", product = "ClientKit" }
            "#,
        )
        .unwrap();

        let catalog: toml::Value = config
            .dependencies
            .android_catalog()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(catalog["versions"]["dep-okhttp"].as_str(), Some("4.12.0"));
        assert_eq!(
            catalog["libraries"]["okhttp"]["module"].as_str(),
            Some("com.squareup.okhttp3:okhttp")
        );
        assert_eq!(
            catalog["libraries"]["okhttp"]["version"]["ref"].as_str(),
            Some("dep-okhttp")
        );
        let (packages, targets) = config.dependencies.ios_packages().unwrap();
        assert!(packages.contains("exactVersion: \"1.2.3\""));
        assert!(targets.contains("product: \"ClientKit\""));
    }
}
