//! The development team device builds are signed with.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Result, bail};

use crate::app::AppCrate;
use crate::util::output;

/// `ios-development-team` from Cargo.toml, else the only team among this Mac's valid Apple
/// development certificates (like Flutter).
pub fn development_team(app: &AppCrate) -> Result<String> {
    if let Some(team) = &app.meta.ios_development_team {
        return Ok(team.clone());
    }
    let teams = keychain_teams()?;
    let mut iter = teams.iter();
    match (iter.next(), iter.next()) {
        (Some((id, name)), None) => {
            println!("→ signing with development team {id} ({name})");
            Ok(id.clone())
        }
        (None, _) => bail!(
            "no valid Apple development certificate in the keychain: sign in with your Apple ID \
             in Xcode > Settings > Accounts, or set `ios-development-team` in \
             [package.metadata.rux]"
        ),
        _ => {
            let choices: Vec<String> = teams
                .iter()
                .map(|(id, name)| format!("  ios-development-team = \"{id}\"   # {name}"))
                .collect();
            bail!(
                "several development teams can sign this app; pick one in \
                 [package.metadata.rux]:\n{}",
                choices.join("\n")
            )
        }
    }
}

/// Team id → team name, from the valid code signing identities' certificates.
fn keychain_teams() -> Result<BTreeMap<String, String>> {
    let identities =
        output(Command::new("security").args(["find-identity", "-v", "-p", "codesigning"]))?;
    let mut teams = BTreeMap::new();
    for (hash, name) in parse_identities(&identities) {
        if !(name.starts_with("Apple Development:") || name.starts_with("iPhone Developer:")) {
            continue;
        }
        let certs = output(Command::new("security").args([
            "find-certificate",
            "-a",
            "-Z",
            "-p",
            "-c",
            &name,
        ]))?;
        let Some(pem) = pem_with_hash(&certs, &hash) else {
            continue;
        };
        if let Some((id, team)) = subject(&pem).as_deref().and_then(parse_team) {
            teams.insert(id, team);
        }
    }
    Ok(teams)
}

/// `(SHA-1, name)` of the valid identities in `security find-identity -v` output.
fn parse_identities(listing: &str) -> Vec<(String, String)> {
    listing
        .lines()
        .filter(|line| !line.contains("CSSMERR_")) // revoked, expired, …
        .filter_map(|line| {
            let (_, rest) = line.split_once(") ")?;
            let (hash, name) = rest.split_once(' ')?;
            Some((hash.to_owned(), name.trim().trim_matches('"').to_owned()))
        })
        .collect()
}

/// The PEM certificate whose SHA-1 is `hash` in `security find-certificate -Z -p` output.
fn pem_with_hash(certs: &str, hash: &str) -> Option<String> {
    let start = certs.find(&format!("SHA-1 hash: {hash}"))?;
    let rest = &certs[start..];
    let begin = rest.find("-----BEGIN CERTIFICATE-----")?;
    let end = rest.find("-----END CERTIFICATE-----")? + "-----END CERTIFICATE-----".len();
    Some(rest[begin..end].to_owned())
}

/// The certificate's subject (RFC 2253), via the system's `openssl`.
fn subject(pem: &str) -> Option<String> {
    let openssl = if Path::new("/usr/bin/openssl").is_file() {
        "/usr/bin/openssl"
    } else {
        "openssl"
    };
    let mut child = Command::new(openssl)
        .args(["x509", "-noout", "-subject", "-nameopt", "RFC2253"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    child.stdin.take()?.write_all(pem.as_bytes()).ok()?;
    let out = child.wait_with_output().ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `(team id, team name)` from a subject like `subject= C=US,O=An Tr\E1\BA\A7n,OU=TEAM,CN=…`
/// (`OU` is the team id, `O` the team name).
fn parse_team(subject: &str) -> Option<(String, String)> {
    let subject = subject.trim().trim_start_matches("subject=").trim();
    let mut fields = BTreeMap::new();
    let mut field = Vec::new();
    let mut bytes = subject.bytes().peekable();
    let mut push = |field: &mut Vec<u8>| {
        let text = String::from_utf8_lossy(field).into_owned();
        if let Some((key, value)) = text.split_once('=') {
            fields.insert(key.trim().to_owned(), value.to_owned());
        }
        field.clear();
    };
    while let Some(b) = bytes.next() {
        match b {
            b'\\' => {
                // `\XX` is a hex-encoded byte (UTF-8), `\c` an escaped character.
                let hex: Vec<u8> = bytes.clone().take(2).collect();
                match std::str::from_utf8(&hex)
                    .ok()
                    .and_then(|h| u8::from_str_radix(h, 16).ok())
                {
                    Some(byte) if hex.len() == 2 => {
                        field.push(byte);
                        bytes.nth(1);
                    }
                    _ => field.extend(bytes.next()),
                }
            }
            b',' => push(&mut field),
            b => field.push(b),
        }
    }
    push(&mut field);
    let id = fields.get("OU")?.clone();
    let name = fields.get("O").cloned().unwrap_or_else(|| id.clone());
    Some((id, name))
}

#[cfg(test)]
mod tests {
    use super::{parse_identities, parse_team, pem_with_hash};

    #[test]
    fn parses_valid_identities() {
        let listing = r#"  1) AAA "Apple Development: Old (X1)" (CSSMERR_TP_CERT_REVOKED)
  2) BBB "Apple Development: me@example.com (X2)"
     2 valid identities found"#;
        assert_eq!(
            parse_identities(listing),
            [(
                "BBB".to_owned(),
                "Apple Development: me@example.com (X2)".to_owned()
            )]
        );
    }

    #[test]
    fn parses_team_from_subject() {
        let subject =
            r"subject= C=US,O=An Tr\E1\BA\A7n,OU=4QJF4U79R2,CN=Apple Development: a (B),UID=X";
        assert_eq!(
            parse_team(subject),
            Some(("4QJF4U79R2".to_owned(), "An Trần".to_owned()))
        );
        assert_eq!(
            parse_team(r"subject=O=Acme\, Inc.,OU=TEAM1"),
            Some(("TEAM1".to_owned(), "Acme, Inc.".to_owned()))
        );
        assert_eq!(parse_team("subject=CN=x"), None);
    }

    #[test]
    fn finds_pem_by_hash() {
        let certs = "SHA-1 hash: AAA\n-----BEGIN CERTIFICATE-----\nA\n-----END CERTIFICATE-----\n\
                     SHA-1 hash: BBB\n-----BEGIN CERTIFICATE-----\nB\n-----END CERTIFICATE-----\n";
        assert_eq!(
            pem_with_hash(certs, "BBB").unwrap(),
            "-----BEGIN CERTIFICATE-----\nB\n-----END CERTIFICATE-----"
        );
        assert!(pem_with_hash(certs, "CCC").is_none());
    }
}
