//! Embeds the rux SDK (the `rux` crate, the crates it depends on and the Android / iOS runtimes)
//! into the CLI, so `rux setup` installs exactly the version of the CLI.
//!
//! Generates `$OUT_DIR/sdk.rs` with `FILES` (path in the SDK, contents) and `HASH` (of all of
//! them, to tell builds of the same version apart).

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

/// Build outputs and editor state that can sit in the source directories.
const SKIP_DIRS: &[&str] = &[
    "target",
    "build",
    ".gradle",
    ".build",
    ".swiftpm",
    ".idea",
    "DerivedData",
    "xcuserdata",
];
const SKIP_FILES: &[&str] = &[".DS_Store", "local.properties", "Thumbs.db"];

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = root.canonicalize().expect("rux workspace root");
    assert!(
        root.join("platforms").is_dir() && root.join("src/lib.rs").is_file(),
        "rux-cli must be built inside the rux repository (it embeds the rux SDK)"
    );

    // The facade crate (root), its sibling crates (but not this one), the runtimes, and the
    // licenses they're distributed under.
    let mut sources = vec![
        root.join("Cargo.toml"),
        root.join("LICENSE-APACHE"),
        root.join("LICENSE-MIT"),
        root.join("src"),
        root.join("platforms"),
    ];
    for entry in std::fs::read_dir(root.join("crates")).unwrap() {
        let dir = entry.unwrap().path();
        if dir.join("Cargo.toml").is_file() && !dir.ends_with("rux-cli") {
            sources.push(dir);
        }
    }

    let mut files = Vec::new();
    for source in &sources {
        println!("cargo:rerun-if-changed={}", source.display());
        collect(source, &mut files);
    }
    files.sort();

    let mut hasher = DefaultHasher::new();
    let mut out = String::from("pub const FILES: &[(&str, &[u8])] = &[\n");
    for file in &files {
        let rel = file
            .strip_prefix(&root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        rel.hash(&mut hasher);
        std::fs::read(file).unwrap().hash(&mut hasher);
        out += &format!(
            "    ({rel:?}, include_bytes!({:?})),\n",
            file.display().to_string()
        );
    }
    out += "];\n";
    out += &format!("pub const HASH: &str = \"{:016x}\";\n", hasher.finish());

    let dest = PathBuf::from(std::env::var_os("OUT_DIR").unwrap()).join("sdk.rs");
    std::fs::write(dest, out).unwrap();
}

fn collect(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        files.push(path.to_path_buf());
        return;
    }
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if path.is_dir() {
            if !SKIP_DIRS.contains(&name.as_ref()) {
                collect(&path, files);
            }
        } else if !SKIP_FILES.contains(&name.as_ref()) && !name.ends_with(".iml") {
            files.push(path);
        }
    }
}
