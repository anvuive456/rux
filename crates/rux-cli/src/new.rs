//! `rux new`: creates an app from the template in `templates/app` (same layout as
//! `examples/counter`: Rust code in `src/`, headless tests in `tests/`, native shells in
//! `android/` and `ios/`).
//!
//! The app depends on the rux SDK of this CLI's version, installed and linked into the app by
//! `rux setup` (see `sdk`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::app::AppCrate;
use crate::app_id::{validate_android_app_id, validate_ios_bundle_id};
use crate::sdk;

/// `rust-toolchain.toml` of standalone apps: the Rust version rux needs.
const TOOLCHAIN: &str = include_str!("../templates/app/rust-toolchain.toml.tmpl");

enum Content {
    /// Rendered: `{{key}}` placeholders are replaced.
    Text(&'static str),
    /// Copied as is.
    Binary(&'static [u8]),
}

struct TemplateFile {
    /// Destination, relative to the app directory; may contain placeholders.
    path: &'static str,
    content: Content,
    executable: bool,
}

macro_rules! template {
    ($path:literal, $src:literal) => {
        template!($path, $src, text, false)
    };
    ($path:literal, $src:literal, text, $exec:literal) => {
        TemplateFile {
            path: $path,
            content: Content::Text(include_str!(concat!("../templates/app/", $src))),
            executable: $exec,
        }
    };
    ($path:literal, $src:literal, binary) => {
        TemplateFile {
            path: $path,
            content: Content::Binary(include_bytes!(concat!("../templates/app/", $src))),
            executable: false,
        }
    };
}

const TEMPLATE: &[TemplateFile] = &[
    template!("Cargo.toml", "Cargo.toml.tmpl"),
    template!(".gitignore", "gitignore.tmpl"),
    template!("src/lib.rs", "src/lib.rs"),
    template!("src/state/mod.rs", "src/state/mod.rs"),
    template!("src/state/counter.rs", "src/state/counter.rs"),
    template!("src/views/mod.rs", "src/views/mod.rs"),
    template!("src/views/home.rs", "src/views/home.rs"),
    template!("tests/home_test.rs", "tests/home_test.rs"),
    template!("android/settings.gradle.kts", "android/settings.gradle.kts"),
    template!("android/build.gradle.kts", "android/build.gradle.kts"),
    template!("android/gradle.properties", "android/gradle.properties"),
    template!("android/gradlew", "android/gradlew", text, true),
    template!("android/gradlew.bat", "android/gradlew.bat"),
    template!(
        "android/gradle/wrapper/gradle-wrapper.properties",
        "android/gradle/wrapper/gradle-wrapper.properties"
    ),
    template!(
        "android/gradle/wrapper/gradle-wrapper.jar",
        "android/gradle/wrapper/gradle-wrapper.jar",
        binary
    ),
    template!(
        "android/app/build.gradle.kts",
        "android/app/build.gradle.kts"
    ),
    template!(
        "android/app/src/main/AndroidManifest.xml",
        "android/app/src/main/AndroidManifest.xml"
    ),
    template!(
        "android/app/src/main/java/{{android_package_dir}}/MainActivity.kt",
        "android/MainActivity.kt.tmpl"
    ),
    template!("ios/project.yml", "ios/project.yml"),
    template!(
        "ios/Sources/AppDelegate.swift",
        "ios/Sources/AppDelegate.swift"
    ),
];

pub fn run(dir: &Path, name: Option<&str>, app_id: Option<&str>) -> Result<()> {
    run_in(&sdk::home()?, dir, name, app_id)
}

/// `run` with the SDK installed under `home`.
fn run_in(home: &Path, dir: &Path, name: Option<&str>, app_id: Option<&str>) -> Result<()> {
    let dir = std::path::absolute(dir).with_context(|| format!("resolving {}", dir.display()))?;
    let existed = dir.exists();
    if existed && std::fs::read_dir(&dir)?.next().is_some() {
        bail!("{} already exists and is not empty", dir.display());
    }

    let name = match name {
        Some(n) => n.to_owned(),
        None => dir
            .file_name()
            .and_then(|n| n.to_str())
            .context("cannot derive the package name from the directory: pass --name")?
            .to_owned(),
    };
    validate_package_name(&name)?;
    let names = Names::new(&name);
    let app_id = match app_id {
        Some(id) => id.to_owned(),
        None => names.default_app_id(),
    };
    validate_app_id(&app_id).with_context(|| {
        if app_id == names.default_app_id() {
            "the default app id derived from the name is invalid: pass --app-id".to_owned()
        } else {
            "invalid --app-id".to_owned()
        }
    })?;

    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let result = generate(home, &dir, &names, &app_id);
    if result.is_err() && !existed {
        let _ = std::fs::remove_dir_all(&dir);
    }
    result
}

fn generate(home: &Path, dir: &Path, names: &Names, app_id: &str) -> Result<()> {
    // Real paths: the relative paths written into the project must survive symlinks
    // (e.g. /tmp → /private/tmp on macOS).
    let dir = real_path(dir)?;
    let workspace = enclosing_workspace(&dir)?;
    let target_dir = workspace.as_ref().map_or(&dir, |w| &w.root).join("target");
    let ios_dir = dir.join("ios");
    // Inside rux's own workspace (e.g. `examples/`), the app uses its `rux`, not the SDK.
    let uses_sdk = !workspace.as_ref().is_some_and(|w| w.has_rux_dep);

    let vars = [
        ("name", names.package.clone()),
        ("lib_name", names.lib.clone()),
        ("project_name", names.project.clone()),
        ("app_struct", names.app_struct.clone()),
        ("display_name", names.display.clone()),
        ("app_id", app_id.to_owned()),
        ("android_package", app_id.to_owned()),
        ("android_package_dir", app_id.replace('.', "/")),
        (
            "rux_dep",
            if uses_sdk {
                format!(
                    "{{ version = \"={}\", path = \"{}\" }}",
                    sdk::VERSION,
                    sdk::APP_LINK
                )
            } else {
                "{ workspace = true }".to_owned()
            },
        ),
        ("rust_version", env!("CARGO_PKG_RUST_VERSION").to_owned()),
        (
            "xcframework",
            escape_quoted(&relative(
                &ios_dir,
                &target_dir.join(format!("ios/{}.xcframework", names.lib)),
            )),
        ),
        (
            "workspace",
            if workspace.is_some() {
                String::new()
            } else {
                "\n# A standalone workspace (the app isn't part of an enclosing one). The linked rux SDK\n\
                 # has its own workspace: keep it out of this one.\n\
                 [workspace]\nexclude = [\".rux\"]\n"
                    .to_owned()
            },
        ),
    ];

    for file in TEMPLATE {
        let path = dir.join(render(file.path, &vars));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let bytes = match file.content {
            Content::Text(text) => render(text, &vars).into_bytes(),
            Content::Binary(bytes) => bytes.to_vec(),
        };
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        if file.executable {
            make_executable(&path)?;
        }
    }
    // rustup also looks for this file in parent directories: keep an enclosing one.
    let has_toolchain = dir
        .ancestors()
        .skip(1)
        .any(|d| d.join("rust-toolchain.toml").is_file() || d.join("rust-toolchain").is_file());
    if !has_toolchain {
        let path = dir.join("rust-toolchain.toml");
        std::fs::write(&path, render(TOOLCHAIN, &vars))
            .with_context(|| format!("writing {}", path.display()))?;
    }

    // Catches template mistakes (bad placeholders, invalid ids) before the user does.
    let app = AppCrate::load(&dir).context("checking the generated Cargo.toml")?;
    app.meta.android_app_id()?;
    app.meta.ios_bundle_id()?;

    if uses_sdk {
        let (sdk_dir, _) = sdk::install(home)?;
        sdk::link(&dir, &sdk_dir)?;
    }

    println!("✓ Created {} in {}", names.display, dir.display());
    println!("  app id: {app_id}");
    if let Some(ws) = workspace.as_ref().filter(|_| uses_sdk) {
        println!(
            "\n⚠ {} is inside the Cargo workspace {}: add \"{}/.rux\" to `exclude` in its \
             [workspace] (the linked rux SDK has its own workspace).",
            dir.display(),
            ws.root.display(),
            relative(&ws.root, &dir),
        );
    }
    if let Some(ws) = &workspace {
        if ws.has_member {
            println!("  part of the Cargo workspace {}", ws.root.display());
        } else {
            println!(
                "\n⚠ {} is inside the Cargo workspace {} but not one of its members:\n  add \
                 \"{}\" to `members` in its Cargo.toml.",
                dir.display(),
                ws.root.display(),
                relative(&ws.root, &dir),
            );
        }
    }
    println!(
        "\nNext:\n  cd {}\n  cargo test          # headless UI tests\n  rux run android     # \
         or: rux run ios",
        dir.display()
    );
    Ok(())
}

/// The names derived from the package name.
struct Names {
    /// Cargo package name (`my-app`).
    package: String,
    /// Library / native library name (`my_app` → `libmy_app.so`).
    lib: String,
    /// Gradle / Xcode project name (`MyApp`).
    project: String,
    /// Root app struct (`MyApp`, `CounterApp`).
    app_struct: String,
    /// Name shown under the app icon (`My App`).
    display: String,
}

impl Names {
    fn new(package: &str) -> Self {
        let words: Vec<String> = package
            .split(['-', '_'])
            .filter(|w| !w.is_empty())
            .map(|w| {
                let mut c = w.chars();
                c.next()
                    .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                    .unwrap_or_default()
            })
            .collect();
        let project = words.concat();
        // Not just `App`: it would clash with the `App` trait from the prelude.
        let app_struct = if project.ends_with("App") && project != "App" {
            project.clone()
        } else {
            format!("{project}App")
        };
        Self {
            package: package.to_owned(),
            lib: package.replace('-', "_"),
            project,
            app_struct,
            display: words.join(" "),
        }
    }

    fn default_app_id(&self) -> String {
        format!(
            "com.example.{}",
            self.lib.replace('_', "").to_ascii_lowercase()
        )
    }
}

/// Cargo's rules plus what the template needs: the lib name is used as a Rust path
/// (`use my_app::…`) and as a native library name.
fn validate_package_name(name: &str) -> Result<()> {
    if !name.starts_with(|c: char| c.is_ascii_alphabetic())
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        bail!(
            "invalid package name `{name}`: use letters, digits, `-` and `_`, starting with a \
             letter (pass --name to choose another name than the directory's)"
        );
    }
    let lib = name.replace('-', "_");
    if RUST_KEYWORDS.contains(&lib.as_str())
        || ["rux", "std", "core", "alloc", "proc_macro", "test"].contains(&lib.as_str())
    {
        bail!("invalid package name `{name}`: `{lib}` is reserved (pass --name)");
    }
    Ok(())
}

/// The app id is used as is for Android (`applicationId` and the Kotlin package of
/// `MainActivity`) and iOS (bundle id).
fn validate_app_id(id: &str) -> Result<()> {
    validate_android_app_id(id)?;
    validate_ios_bundle_id(id)?;
    if let Some(s) = id.split('.').find(|s| JVM_KEYWORDS.contains(s)) {
        bail!("`{id}`: `{s}` is a Kotlin / Java keyword, so it can't be part of a package name");
    }
    Ok(())
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut",
    "pub", "ref", "return", "self", "static", "struct", "super", "trait", "true", "try", "type",
    "unsafe", "use", "where", "while", "abstract", "become", "box", "do", "final", "macro",
    "override", "priv", "typeof", "unsized", "virtual", "yield",
];

/// Kotlin hard keywords + Java reserved words (AGP generates Java for the `R` class).
const JVM_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "assert",
    "boolean",
    "break",
    "byte",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extends",
    "false",
    "final",
    "finally",
    "float",
    "for",
    "fun",
    "goto",
    "if",
    "implements",
    "import",
    "in",
    "instanceof",
    "int",
    "interface",
    "is",
    "long",
    "native",
    "new",
    "null",
    "object",
    "package",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "true",
    "try",
    "typealias",
    "typeof",
    "val",
    "var",
    "void",
    "volatile",
    "when",
    "while",
];

struct Workspace {
    root: PathBuf,
    /// Whether the new app matches the workspace's `members`.
    has_member: bool,
    /// Whether it has `rux` in `[workspace.dependencies]` (rux's own workspace does).
    has_rux_dep: bool,
}

/// The Cargo workspace above `dir`, if any (Cargo would reject a nested `[workspace]` there).
fn enclosing_workspace(dir: &Path) -> Result<Option<Workspace>> {
    for root in dir.ancestors().skip(1) {
        let manifest = root.join("Cargo.toml");
        let Ok(raw) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        let table: toml::Table =
            toml::from_str(&raw).with_context(|| format!("parsing {}", manifest.display()))?;
        let Some(ws) = table.get("workspace").and_then(|w| w.as_table()) else {
            continue;
        };
        let patterns = |key: &str| -> Vec<String> {
            ws.get(key)
                .and_then(|m| m.as_array())
                .into_iter()
                .flatten()
                .filter_map(|m| m.as_str().map(str::to_owned))
                .collect()
        };
        let rel = relative(root, dir);
        let matches = |p: &String| glob_match(p.trim_end_matches('/'), &rel);
        let has_member =
            patterns("members").iter().any(matches) && !patterns("exclude").iter().any(matches);
        return Ok(Some(Workspace {
            root: root.to_path_buf(),
            has_member,
            has_rux_dep: ws.get("dependencies").and_then(|d| d.get("rux")).is_some(),
        }));
    }
    Ok(None)
}

/// Matches `/`-separated paths where a pattern segment may be `*` or contain `*` wildcards
/// (the forms used in `members`).
fn glob_match(pattern: &str, path: &str) -> bool {
    let (p, s): (Vec<_>, Vec<_>) = (pattern.split('/').collect(), path.split('/').collect());
    p.len() == s.len() && p.iter().zip(&s).all(|(p, s)| segment_match(p, s))
}

fn segment_match(p: &str, s: &str) -> bool {
    match p.split_once('*') {
        None => p == s,
        Some((head, tail)) => {
            s.starts_with(head) && (head.len()..=s.len()).any(|i| segment_match(tail, &s[i..]))
        }
    }
}

/// Replaces `{{key}}` placeholders.
fn render(text: &str, vars: &[(&str, String)]) -> String {
    vars.iter().fold(text.to_owned(), |t, (k, v)| {
        t.replace(&format!("{{{{{k}}}}}"), v)
    })
}

/// For values inside `"…"` in TOML, Kotlin and YAML.
fn escape_quoted(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// `to` relative to `from` (both absolute), with `/` separators; absolute if they share no
/// root (e.g. different drives on Windows).
fn relative(from: &Path, to: &Path) -> String {
    let (from, to): (Vec<_>, Vec<_>) = (from.components().collect(), to.components().collect());
    if from.first() != to.first() {
        return slash(to.iter().collect::<PathBuf>().as_path());
    }
    let common = from.iter().zip(&to).take_while(|(a, b)| a == b).count();
    let mut parts: Vec<String> = vec!["..".to_owned(); from.len() - common];
    parts.extend(
        to[common..]
            .iter()
            .map(|c| c.as_os_str().to_string_lossy().into_owned()),
    );
    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// `canonicalize`, without Windows' `\\?\` prefix (which Gradle / Cargo paths don't need).
fn real_path(path: &Path) -> Result<PathBuf> {
    let real =
        std::fs::canonicalize(path).with_context(|| format!("resolving {}", path.display()))?;
    if cfg!(windows)
        && let Some(rest) = real.to_str().and_then(|s| s.strip_prefix(r"\\?\"))
        && !rest.starts_with("UNC\\")
    {
        return Ok(PathBuf::from(rest));
    }
    Ok(real)
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    std::fs::set_permissions(path, perms).with_context(|| format!("chmod +x {}", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_names() {
        let n = Names::new("my-cool_app");
        assert_eq!(n.lib, "my_cool_app");
        assert_eq!(n.project, "MyCoolApp");
        assert_eq!(n.app_struct, "MyCoolApp");
        assert_eq!(n.display, "My Cool App");
        assert_eq!(n.default_app_id(), "com.example.mycoolapp");
        assert_eq!(Names::new("counter").app_struct, "CounterApp");
        assert_eq!(Names::new("app").app_struct, "AppApp");
    }

    #[test]
    fn validates_names_and_ids() {
        for ok in ["hello", "my-app", "app_2"] {
            assert!(validate_package_name(ok).is_ok(), "{ok}");
        }
        for bad in ["2app", "-app", "my app", "my.app", "", "fn", "rux", "test"] {
            assert!(validate_package_name(bad).is_err(), "{bad}");
        }
        assert!(validate_app_id("com.example.hello").is_ok());
        for bad in ["com.example.new", "com.my-co.app", "com.my_co.app", "app"] {
            assert!(validate_app_id(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn relative_paths() {
        let p = |s: &str| {
            PathBuf::from(if cfg!(windows) {
                format!("C:{s}")
            } else {
                s.into()
            })
        };
        assert_eq!(relative(&p("/a/b/app"), &p("/a/rux")), "../../rux");
        assert_eq!(
            relative(&p("/a/rux/examples/x/ios"), &p("/a/rux")),
            "../../.."
        );
        assert_eq!(relative(&p("/a"), &p("/a/b")), "b");
        assert_eq!(relative(&p("/a"), &p("/a")), ".");
    }

    #[test]
    fn matches_member_globs() {
        assert!(glob_match("examples/*", "examples/hello"));
        assert!(glob_match("crates/rux-*", "crates/rux-cli"));
        assert!(glob_match("apps/hello", "apps/hello"));
        assert!(!glob_match("examples/*", "examples/a/b"));
        assert!(!glob_match("crates/rux-*", "crates/other"));
    }

    #[test]
    fn renders_every_placeholder() {
        let vars: Vec<_> = [
            "name",
            "lib_name",
            "project_name",
            "app_struct",
            "display_name",
            "app_id",
            "android_package",
            "android_package_dir",
            "rux_dep",
            "rust_version",
            "xcframework",
            "workspace",
        ]
        .map(|k| (k, "x".to_owned()))
        .into();
        assert!(!render(TOOLCHAIN, &vars).contains("{{"));
        for file in TEMPLATE {
            assert!(!render(file.path, &vars).contains("{{"), "{}", file.path);
            if let Content::Text(t) = file.content {
                assert!(!render(t, &vars).contains("{{"), "{}", file.path);
            }
        }
    }

    /// The template's copies of `examples/counter` files must not drift from it (Gradle /
    /// AGP versions, wrapper, runtime glue).
    #[test]
    fn template_matches_counter_example() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for file in [
            "android/build.gradle.kts",
            "android/gradle.properties",
            "android/gradlew",
            "android/gradlew.bat",
            "android/gradle/wrapper/gradle-wrapper.properties",
            "android/gradle/wrapper/gradle-wrapper.jar",
            "ios/Sources/AppDelegate.swift",
            "src/state/counter.rs",
        ] {
            let read = |p: PathBuf| std::fs::read(&p).unwrap_or_else(|e| panic!("{p:?}: {e}"));
            assert!(
                read(root.join("templates/app").join(file))
                    == read(root.join("../../examples/counter").join(file)),
                "templates/app/{file} differs from examples/counter/{file}"
            );
        }

        let read = |p: &str| std::fs::read_to_string(root.join(p)).unwrap();
        let vars = [("project_name", "RuxCounter".to_owned())];
        assert_eq!(
            render(&read("templates/app/android/settings.gradle.kts"), &vars),
            read("../../examples/counter/android/settings.gradle.kts"),
        );

        // Same toolchain as rux itself (comments aside).
        let settings = |t: &str| -> Vec<String> {
            t.lines()
                .filter(|l| !l.trim_start().starts_with('#'))
                .map(str::to_owned)
                .collect()
        };
        let vars = [("rust_version", env!("CARGO_PKG_RUST_VERSION").to_owned())];
        assert_eq!(
            settings(&render(TOOLCHAIN, &vars)),
            settings(&read("../../rust-toolchain.toml")),
        );
    }

    #[test]
    fn creates_an_app() {
        let tmp = std::env::temp_dir().join(format!("rux-new-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let home = tmp.join("rux-home");
        let dir = tmp.join("hello-world");
        run_in(&home, &dir, None, None).unwrap();

        let app = AppCrate::load(&dir).unwrap();
        assert_eq!(app.lib_name, "hello_world");
        assert_eq!(app.meta.android_app_id().unwrap(), "com.example.helloworld");
        let activity = dir.join("android/app/src/main/java/com/example/helloworld/MainActivity.kt");
        assert!(activity.is_file());
        assert!(dir.join("rust-toolchain.toml").is_file());
        let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
        let dep = format!(
            "rux = {{ version = \"={}\", path = \"{}\" }}",
            sdk::VERSION,
            sdk::APP_LINK
        );
        assert!(manifest.contains(&dep), "{manifest}");
        assert!(manifest.contains("exclude = [\".rux\"]"));
        // The SDK is installed under `home` and linked into the app.
        assert!(dir.join(".rux/sdk/src/lib.rs").is_file());
        assert!(
            home.join("sdk")
                .join(sdk::VERSION)
                .join("src/lib.rs")
                .is_file()
        );
        assert!(
            run_in(&home, &dir, None, None).is_err(),
            "must not overwrite"
        );

        // A failed run removes the directory it created.
        let bad = tmp.join("bad");
        assert!(run_in(&home, &bad, None, Some("com.example.new")).is_err());
        assert!(!bad.exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
