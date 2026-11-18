//! The JDK that Gradle and the SDK's `sdkmanager` run on, so users don't have to install one or
//! set `JAVA_HOME`: fall back to the JDK bundled with Android Studio.
//!
//! Order: `JAVA_HOME`, then `java` on PATH, then Android Studio's JDK (`jbr`, formerly `jre`).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::on_path;

/// Oldest JDK the Android Gradle Plugin (9.x) runs on.
pub const MIN_VERSION: u32 = 17;

pub const HINT: &str = "install Android Studio (it bundles a JDK), or set JAVA_HOME to a JDK";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    JavaHome,
    Path,
    AndroidStudio,
}

#[derive(Debug)]
pub struct Java {
    pub source: Source,
    /// The JDK directory (`JAVA_HOME`); unknown for `java` found on PATH.
    pub home: Option<PathBuf>,
    pub exe: PathBuf,
}

impl Java {
    /// Where it comes from, for messages.
    pub fn describe(&self) -> String {
        match (self.source, &self.home) {
            (Source::JavaHome, Some(h)) => format!("JAVA_HOME: {}", h.display()),
            (Source::AndroidStudio, Some(h)) => format!("Android Studio's JDK: {}", h.display()),
            _ => format!("on PATH: {}", self.exe.display()),
        }
    }

    /// Major version (`21` for "21.0.3", `8` for "1.8.0_391").
    pub fn version(&self) -> Option<u32> {
        let out = Command::new(&self.exe).arg("-version").output().ok()?;
        // `java -version` prints to stderr: `openjdk version "21.0.3" 2024-04-16`.
        parse_version(&String::from_utf8_lossy(&out.stderr))
    }
}

/// The JDK to use, if any.
pub fn find() -> Option<Java> {
    if let Some(home) = std::env::var_os("JAVA_HOME").filter(|h| !h.is_empty()) {
        // Set but wrong is reported as such (Gradle would fail on it anyway).
        let home = PathBuf::from(home);
        return Some(Java {
            source: Source::JavaHome,
            exe: java_exe(&home),
            home: Some(home),
        });
    }
    if let Some(exe) = on_path("java") {
        return Some(Java {
            source: Source::Path,
            home: None,
            exe,
        });
    }
    android_studio_jdk().map(|home| Java {
        source: Source::AndroidStudio,
        exe: java_exe(&home),
        home: Some(home),
    })
}

/// Lets `cmd` (Gradle, sdkmanager) find a JDK: sets `JAVA_HOME` to Android Studio's JDK when
/// neither `JAVA_HOME` nor `java` on PATH is there.
pub fn configure(cmd: &mut Command) {
    if let Some(Java {
        source: Source::AndroidStudio,
        home: Some(home),
        ..
    }) = find()
    {
        cmd.env("JAVA_HOME", home);
    }
}

fn java_exe(home: &Path) -> PathBuf {
    home.join("bin")
        .join(format!("java{}", std::env::consts::EXE_SUFFIX))
}

/// The JDK bundled with Android Studio, in its default install locations.
pub fn android_studio_jdk() -> Option<PathBuf> {
    android_studio_dirs()
        .into_iter()
        .flat_map(|studio| {
            ["jbr", "jre"].map(|jdk| {
                if cfg!(target_os = "macos") {
                    studio.join("Contents").join(jdk).join("Contents/Home")
                } else {
                    studio.join(jdk)
                }
            })
        })
        .find(|home| java_exe(home).is_file())
}

/// Default Android Studio install directories (`.app` bundles on macOS).
fn android_studio_dirs() -> Vec<PathBuf> {
    let env = |v: &str| {
        std::env::var_os(v)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    };
    let mut dirs = Vec::new();
    if cfg!(target_os = "macos") {
        let app = "Android Studio.app";
        dirs.push(Path::new("/Applications").join(app));
        dirs.extend(env("HOME").map(|h| h.join("Applications").join(app)));
    } else if cfg!(windows) {
        dirs.extend(env("ProgramFiles").map(|p| p.join(r"Android\Android Studio")));
        // JetBrains Toolbox / per-user installs.
        dirs.extend(env("LOCALAPPDATA").map(|p| p.join(r"Programs\Android Studio")));
    } else {
        dirs.extend(["/opt/android-studio", "/usr/local/android-studio"].map(PathBuf::from));
        dirs.extend(env("HOME").map(|h| h.join("android-studio")));
        dirs.push(PathBuf::from("/snap/android-studio/current/android-studio"));
    }
    dirs
}

fn parse_version(text: &str) -> Option<u32> {
    let quoted = text.split('"').nth(1)?;
    let mut parts = quoted.split(['.', '_', '-', '+']);
    let first: u32 = parts.next()?.parse().ok()?;
    if first == 1 {
        parts.next()?.parse().ok() // 1.8.0_391 → 8
    } else {
        Some(first)
    }
}

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn parses_java_versions() {
        let v = |s| parse_version(s);
        assert_eq!(
            v("openjdk version \"21.0.3\" 2024-04-16\nOpenJDK Runtime"),
            Some(21)
        );
        assert_eq!(v("java version \"1.8.0_391\""), Some(8));
        assert_eq!(v("openjdk version \"17\" 2021-09-14"), Some(17));
        assert_eq!(v("openjdk version \"25-ea\" 2025-09-16"), Some(25));
        assert_eq!(v("no version here"), None);
    }
}
