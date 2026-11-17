//! Validation of application ids (Android `applicationId`, iOS bundle identifier).

use anyhow::{Result, bail};

/// Android rules: at least two `.`-separated segments, each starting with a letter and
/// containing only letters, digits and `_`.
pub fn validate_android_app_id(id: &str) -> Result<()> {
    let segments: Vec<&str> = id.split('.').collect();
    let ok = segments.len() >= 2
        && segments.iter().all(|s| {
            s.starts_with(|c: char| c.is_ascii_alphabetic())
                && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
    if !ok {
        bail!(
            "`{id}`: an Android application id needs at least two `.`-separated segments, each \
             starting with a letter and containing only letters, digits and `_`"
        );
    }
    Ok(())
}

/// iOS rules: only letters, digits, `-` and `.`, with no empty segments.
pub fn validate_ios_bundle_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id
            .split('.')
            .all(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'));
    if !ok {
        bail!(
            "`{id}`: an iOS bundle id may only contain letters, digits, `-` and `.` \
             (no `_`, no empty segments)"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_android_app_id, validate_ios_bundle_id};

    #[test]
    fn validates_app_ids() {
        for ok in [
            "com.example.app",
            "dev.rux.examples.counter",
            "com.my_co.app2",
        ] {
            assert!(validate_android_app_id(ok).is_ok(), "android {ok}");
        }
        for bad in [
            "app",
            "com.1app",
            "com.my-co.app",
            "com..app",
            ".com.app",
            "",
        ] {
            assert!(validate_android_app_id(bad).is_err(), "android {bad}");
        }
        for ok in ["com.example.app", "com.my-co.app", "com.example.1app"] {
            assert!(validate_ios_bundle_id(ok).is_ok(), "ios {ok}");
        }
        for bad in ["com.my_co.app", "com..app", "com.app.", ""] {
            assert!(validate_ios_bundle_id(bad).is_err(), "ios {bad}");
        }
    }
}
