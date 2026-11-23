//! Renders the native host projects for features enabled on the application crate.

mod dependencies;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml::Value;

macro_rules! files {
    ($platform:literal; $($file:literal),+ $(,)?) => {
        &[$(($file, include_str!(concat!("../templates/", $platform, "/", $file)))),+]
    };
}

const ANDROID: &[(&str, &str)] = files!("android";
    "settings.gradle.kts",
    "build.gradle.kts",
    "gradle.properties",
    "gradlew",
    "gradlew.bat",
    "gradle/wrapper/gradle-wrapper.properties",
    "app/build.gradle.kts",
    "app/src/main/AndroidManifest.xml",
);
const IOS: &[(&str, &str)] = files!("ios"; "project.yml", "Sources/AppDelegate.swift");

/// Generate the selected platform sources in the calling application's Cargo `OUT_DIR`.
pub fn generate() -> Result<()> {
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=rux.toml");
    let android = std::env::var_os("CARGO_FEATURE_ANDROID").is_some();
    let ios = std::env::var_os("CARGO_FEATURE_IOS").is_some();
    if !android && !ios {
        return Ok(());
    }

    let manifest_dir = cargo_path("CARGO_MANIFEST_DIR")?;
    let out = cargo_path("OUT_DIR")?;
    let dependencies = dependencies::Dependencies::load(&manifest_dir.join("rux.toml"))?;
    let manifest: Value = std::fs::read_to_string(manifest_dir.join("Cargo.toml"))?.parse()?;
    let package = manifest.get("package").context("missing [package]")?;
    let name = package
        .get("name")
        .and_then(Value::as_str)
        .context("missing package name")?;
    let lib = manifest
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| name.replace('-', "_"));
    let words: Vec<_> = name
        .split(['-', '_'])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            chars.next().unwrap().to_ascii_uppercase().to_string() + chars.as_str()
        })
        .collect();
    let meta = package
        .get("metadata")
        .and_then(|metadata| metadata.get("rux"));
    let id = |key| meta.and_then(|rux| rux.get(key)).and_then(Value::as_str);
    let android_package = id("app-id")
        .or_else(|| id("android-app-id"))
        .unwrap_or_default();
    if android {
        anyhow::ensure!(
            !android_package.is_empty()
                && android_package.split('.').all(|part| {
                    !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                }),
            "set a valid Android app-id in [package.metadata.rux]"
        );
    }
    let mut vars = vec![
        ("lib_name", lib.clone()),
        ("project_name", words.concat()),
        ("display_name", words.join(" ")),
        ("android_package", android_package.to_owned()),
        ("xcframework", format!("{lib}.xcframework")),
    ];

    if android {
        println!("cargo:rerun-if-env-changed=RUX_REPORT_ANDROID_OUT_DIR");
        if std::env::var_os("RUX_REPORT_ANDROID_OUT_DIR").is_some() {
            // cargo-ndk consumes Cargo's build-script JSON events. Cargo forwards this warning
            // to stderr, including when the build script's output is cached.
            println!(
                "cargo:warning=RUX_ANDROID_OUT_DIR[{}]={}",
                std::env::var("TARGET")?,
                out.display()
            );
        }
        let project = out.join("android");
        let catalog = dependencies.android_catalog()?;
        render_files(&project, ANDROID, &vars)?;
        write(
            &project.join("gradle/libs.versions.toml"),
            catalog.as_bytes(),
        )?;
        let activity = project
            .join("app/src/main/java")
            .join(android_package.replace('.', "/"))
            .join("MainActivity.kt");
        write(
            &activity,
            render(
                include_str!("../templates/android/MainActivity.kt.tmpl"),
                &vars,
            )
            .as_bytes(),
        )?;
        write(
            &project.join("gradle/wrapper/gradle-wrapper.jar"),
            include_bytes!("../templates/android/gradle/wrapper/gradle-wrapper.jar"),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let wrapper = project.join("gradlew");
            let mut permissions = std::fs::metadata(&wrapper)?.permissions();
            permissions.set_mode(permissions.mode() | 0o111);
            std::fs::set_permissions(wrapper, permissions)?;
        }
    }
    if ios {
        let (packages, target_dependencies) = dependencies.ios_packages()?;
        vars.extend([
            ("ios_packages", packages),
            ("ios_dependencies", target_dependencies),
        ]);
        render_files(&out.join("ios"), IOS, &vars)?;
    }
    Ok(())
}

fn cargo_path(name: &str) -> Result<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .with_context(|| format!("{name} must be set by Cargo"))
}

fn render_files(dir: &Path, files: &[(&str, &str)], vars: &[(&str, String)]) -> Result<()> {
    for (file, template) in files {
        write(&dir.join(file), render(template, vars).as_bytes())?;
    }
    Ok(())
}

fn render(template: &str, vars: &[(&str, String)]) -> String {
    vars.iter().fold(template.to_owned(), |text, (key, value)| {
        text.replace(&format!("{{{{{key}}}}}"), value)
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<()> {
    if std::fs::read(path).is_ok_and(|current| current == bytes) {
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().context("missing parent directory")?)?;
    std::fs::write(path, bytes).with_context(|| format!("writing {}", path.display()))
}
