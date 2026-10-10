// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What setting up Amazon S3, Backblaze B2 and Cloudflare R2 have in common: the HTTP client the
//! two API-driven setups talk through, and the S3 bucket-name rule that Amazon and R2 share.

use std::time::Duration;

/// An HTTPS client using the system's TLS, so a proxy or a company root certificate that macOS
/// trusts is trusted here too, with a time limit generous enough for a slow bucket creation.
pub fn http_agent() -> Result<ureq::Agent, String> {
    let tls = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
    Ok(ureq::AgentBuilder::new().tls_connector(std::sync::Arc::new(tls)).timeout(Duration::from_secs(60)).build())
}

/// Whether a name follows S3's rule: 3 to 63 lower-case letters, digits and hyphens, neither
/// starting nor ending with a hyphen. Amazon allows dots as well; R2 doesn't.
pub fn bucket_name_ok(name: &str, allow_dots: bool) -> bool {
    (3..=63).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || (allow_dots && c == '.'))
        && !name.starts_with('-')
        && !name.ends_with('-')
}

/// A list of bucket names, sorted for choosing from.
pub fn sorted(mut names: Vec<String>) -> Vec<String> {
    names.sort();
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bucket_names() {
        assert!(bucket_name_ok("keepr-backup-1a2b", false));
        assert!(bucket_name_ok("my.bucket", true) && !bucket_name_ok("my.bucket", false));
        assert!(!bucket_name_ok("Keepr", true) && !bucket_name_ok("-keepr", true) && !bucket_name_ok("ab", true));
    }
}
