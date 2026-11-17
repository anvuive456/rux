//! Fingerprint of the app's type definitions.
//!
//! Hot reload carries state over by reusing the previous build's memory, which is only sound
//! if the types stored in signals keep their layout. The runtime checks each value's type name
//! and size, but can't see a field being changed for another of the same size — so whenever a
//! `struct` / `enum` / `union` / `type` definition (or Cargo.toml) changes, `rux run` does a
//! hot restart instead.

use std::collections::BTreeMap;
use std::path::Path;

use quote::ToTokens;
use syn::Item;

/// `None` if a file doesn't parse (treated as "changed").
pub type Fingerprint = Option<BTreeMap<String, String>>;

/// Type definitions in `<app>/src/**/*.rs`, plus `<app>/Cargo.toml`.
pub fn fingerprint(app_dir: &Path) -> Fingerprint {
    let mut defs = BTreeMap::new();
    collect_dir(&app_dir.join("src"), &mut defs)?;
    if let Ok(manifest) = std::fs::read_to_string(app_dir.join("Cargo.toml")) {
        defs.insert("Cargo.toml".into(), manifest);
    }
    Some(defs)
}

fn collect_dir(dir: &Path, defs: &mut BTreeMap<String, String>) -> Option<()> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Some(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_dir(&path, defs)?;
        } else if path.extension().is_some_and(|e| e == "rs") {
            let source = std::fs::read_to_string(&path).ok()?;
            let file = syn::parse_file(&source).ok()?;
            collect_items(&path.to_string_lossy(), &file.items, defs);
        }
    }
    Some(())
}

fn collect_items(scope: &str, items: &[Item], defs: &mut BTreeMap<String, String>) {
    for item in items {
        let name = match item {
            Item::Struct(i) => &i.ident,
            Item::Enum(i) => &i.ident,
            Item::Union(i) => &i.ident,
            Item::Type(i) => &i.ident,
            Item::Mod(m) => {
                if let Some((_, items)) = &m.content {
                    collect_items(&format!("{scope}::{}", m.ident), items, defs);
                }
                continue;
            }
            _ => continue,
        };
        defs.insert(
            format!("{scope}::{name}"),
            item.to_token_stream().to_string(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::fingerprint;

    fn app(files: &[(&str, &str)]) -> std::path::PathBuf {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("rux-types-test-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (path, source) in files {
            let path = dir.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        dir
    }

    #[test]
    fn ignores_code_changes_but_not_type_changes() {
        let base = app(&[(
            "src/lib.rs",
            "struct S { a: i32 }\nfn f() -> i32 { 1 }\nmod m { enum E { A } }",
        )]);
        let code = app(&[(
            "src/lib.rs",
            "struct S { a: i32 }\nfn f() -> i32 { 2 } // edited\nmod m { enum E { A } }",
        )]);
        let field = app(&[(
            "src/lib.rs",
            "struct S { a: u32 }\nfn f() -> i32 { 1 }\nmod m { enum E { A } }",
        )]);
        let nested = app(&[(
            "src/lib.rs",
            "struct S { a: i32 }\nfn f() -> i32 { 1 }\nmod m { enum E { A, B } }",
        )]);
        let strip = |fp: super::Fingerprint, dir: &std::path::Path| {
            fp.map(|m| {
                m.into_iter()
                    .map(|(k, v)| (k.replace(&*dir.to_string_lossy(), ""), v))
                    .collect::<Vec<_>>()
            })
        };
        let fp = |dir: &std::path::Path| strip(fingerprint(dir), dir);
        assert_eq!(fp(&base), fp(&code));
        assert_ne!(fp(&base), fp(&field));
        assert_ne!(fp(&base), fp(&nested));
        let broken = app(&[("src/lib.rs", "struct {")]);
        assert_eq!(fingerprint(&broken), None);
        for d in [base, code, field, nested, broken] {
            std::fs::remove_dir_all(d).unwrap();
        }
    }
}
