//! Rust names → Kotlin / Swift conventions.

/// `user_name` / `UserName` → `userName`.
pub fn camel(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut upper_next = false;
    for (i, c) in name.chars().enumerate() {
        if c == '_' {
            upper_next = i > 0;
        } else if out.is_empty() {
            out.extend(c.to_lowercase());
        } else if upper_next {
            out.extend(c.to_uppercase());
            upper_next = false;
        } else {
            out.push(c);
        }
    }
    out
}

/// `SmartWatch` → `SMART_WATCH` (Kotlin enum entries).
pub fn upper_snake(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_uppercase() && i > 0 {
            out.push('_');
        }
        out.extend(c.to_uppercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{camel, upper_snake};

    #[test]
    fn converts_names() {
        assert_eq!(camel("user_name"), "userName");
        assert_eq!(camel("info"), "info");
        assert_eq!(camel("SmartWatch"), "smartWatch");
        assert_eq!(camel("os_version_2"), "osVersion2");
        assert_eq!(upper_snake("SmartWatch"), "SMART_WATCH");
        assert_eq!(upper_snake("Full"), "FULL");
    }
}
