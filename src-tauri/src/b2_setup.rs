// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Setting up Backblaze B2 for Keepr through B2's own API, from the account's master
//! application key, which is used for this and then forgotten (never saved).
//!
//! A destination gets a new private bucket, encrypted at rest, whose lifecycle rule keeps only
//! each file's latest version: B2 keeps every version by default, and a delete only hides a
//! file, so without the rule Keepr's tidy-ups would free nothing and old data would be billed
//! for ever. A source gets nothing new in the bucket. Either way Keepr then makes a key that
//! works only in that one bucket, with only what it needs, and keeps that key in the Keychain.

use crate::aws_setup::{Made, Mode};
use base64::Engine;
use serde_json::{json, Value};

struct Account {
    id: String,
    api: String,
    token: String,
    /// "https://s3.eu-central-003.backblazeb2.com"
    s3: String,
    agent: ureq::Agent,
}

/// B2's error ({"code": …, "message": …}) in words.
fn explain(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(_, r) => {
            let v: Value = r.into_json().unwrap_or_default();
            let code = v["code"].as_str().unwrap_or_default();
            let msg = v["message"].as_str().unwrap_or_default();
            match code {
                "bad_auth_token" | "unauthorized" if msg.contains("key") || msg.contains("capab") => {
                    format!("That key isn't allowed to do this ({msg}). Use the account's master application key.")
                }
                "bad_auth_token" | "unauthorized" => "Backblaze didn't accept that key ID and application key.".into(),
                "duplicate_bucket_name" => "Another Backblaze account already has a bucket with that name. Choose another.".into(),
                "too_many_buckets" => "This account has as many buckets as Backblaze allows.".into(),
                _ if !msg.is_empty() => format!("Backblaze said: {msg}"),
                _ => format!("Backblaze answered {code}."),
            }
        }
        e => format!("Couldn't reach Backblaze: {e}"),
    }
}

fn authorize(key_id: &str, key: &str) -> Result<Account, String> {
    let agent = crate::cloud_setup::http_agent()?;
    let basic = base64::engine::general_purpose::STANDARD.encode(format!("{}:{}", key_id.trim(), key.trim()));
    let v: Value = agent
        .get("https://api.backblazeb2.com/b2api/v2/b2_authorize_account")
        .set("Authorization", &format!("Basic {basic}"))
        .call()
        .map_err(explain)?
        .into_json()
        .map_err(|e| e.to_string())?;
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    Ok(Account { id: s("accountId"), api: s("apiUrl"), token: s("authorizationToken"), s3: s("s3ApiUrl"), agent })
}

impl Account {
    fn call(&self, op: &str, body: Value) -> Result<Value, String> {
        self.agent
            .post(&format!("{}/b2api/v2/{op}", self.api))
            .set("Authorization", &self.token)
            .send_json(body)
            .map_err(explain)?
            .into_json()
            .map_err(|e| e.to_string())
    }

    /// Each bucket in the account, as B2 describes it.
    fn buckets(&self) -> Result<Vec<Value>, String> {
        let v = self.call("b2_list_buckets", json!({ "accountId": self.id }))?;
        Ok(v["buckets"].as_array().cloned().unwrap_or_default())
    }

    /// "eu-central-003", from the S3 address.
    fn region(&self) -> String {
        self.s3.split("://").last().unwrap_or_default().trim_start_matches("s3.").split('.').next().unwrap_or_default().to_string()
    }
}

/// Keep only each file's latest version; hidden (deleted) ones go a day later.
fn keep_latest() -> Value {
    json!([{ "fileNamePrefix": "", "daysFromHidingToDeleting": 1, "daysFromUploadingToHiding": null }])
}

/// Whether Keepr made this bucket: it tags the ones it makes, in their bucket info.
fn made_by_keepr(b: &Value) -> bool {
    b["bucketInfo"]["created-by"].as_str() == Some("Keepr")
}

fn check_name(bucket: &str) -> Result<(), String> {
    let ok = (6..=50).contains(&bucket.len())
        && bucket.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !bucket.to_lowercase().starts_with("b2-");
    if ok {
        Ok(())
    } else {
        Err("A B2 bucket name is 6 to 50 letters, digits and hyphens, and can't start with b2-.".into())
    }
}

/// The account's buckets, for choosing one to back up.
pub fn list(key_id: &str, key: &str) -> Result<Vec<String>, String> {
    let a = authorize(key_id, key)?;
    Ok(crate::cloud_setup::sorted(a.buckets()?.iter().filter_map(|b| b["bucketName"].as_str().map(str::to_string)).collect()))
}

/// Makes (or, for a source, finds) the bucket and a key for it alone. The master key is
/// dropped when this returns.
pub fn setup(key_id: &str, key: &str, bucket: &str, mode: Mode) -> Result<Made, String> {
    let bucket = bucket.trim();
    check_name(bucket)?;
    let a = authorize(key_id, key)?;
    let existing = a.buckets()?.into_iter().find(|b| b["bucketName"].as_str() == Some(bucket));
    let id = |b: &Value| b["bucketId"].as_str().unwrap_or_default().to_string();
    let bucket_id = match (mode, existing) {
        (Mode::Source, Some(b)) => id(&b),
        (Mode::Source, None) => return Err(format!("There's no bucket called {bucket} in this account.")),
        (Mode::Destination, Some(b)) if made_by_keepr(&b) => {
            // Ours already (a second try): make sure it keeps only the latest versions.
            a.call("b2_update_bucket", json!({ "accountId": a.id, "bucketId": id(&b), "lifecycleRules": keep_latest() }))?;
            id(&b)
        }
        // Someone's own bucket. Keepr's rule would delete its old versions a day after they were
        // replaced, which may be exactly what the bucket is there to keep.
        (Mode::Destination, Some(_)) => {
            return Err(format!("This account already has a bucket called {bucket} that Keepr didn't make. Choose another name."))
        }
        (Mode::Destination, None) => {
            let v = a.call(
                "b2_create_bucket",
                json!({
                    "accountId": a.id,
                    "bucketName": bucket,
                    "bucketType": "allPrivate",
                    "lifecycleRules": keep_latest(),
                    "defaultServerSideEncryption": { "mode": "SSE-B2", "algorithm": "AES256" },
                    "bucketInfo": { "created-by": "Keepr" },
                }),
            )?;
            v["bucketId"].as_str().unwrap_or_default().to_string()
        }
    };
    let caps: &[&str] = match mode {
        Mode::Destination => &["listBuckets", "listFiles", "readFiles", "writeFiles", "deleteFiles"],
        Mode::Source => &["listBuckets", "listFiles", "readFiles"],
    };
    let name = format!("keepr-{}{}", if mode == Mode::Source { "read-" } else { "" }, bucket);
    let k = a.call(
        "b2_create_key",
        json!({ "accountId": a.id, "capabilities": caps, "keyName": &name[..name.len().min(100)], "bucketId": bucket_id }),
    )?;
    let (id, secret) = (k["applicationKeyId"].as_str().unwrap_or_default(), k["applicationKey"].as_str().unwrap_or_default());
    if id.is_empty() || secret.is_empty() {
        return Err("Backblaze didn't return the new key.".into());
    }
    let made = Made::new(a.region(), bucket.to_string(), id.to_string(), secret.to_string(), a.s3.clone());
    crate::aws_setup::finish(made)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_regions() {
        assert!(check_name("keepr-backup-1a2b3c4d").is_ok());
        assert!(check_name("b2-mine").is_err());
        assert!(check_name("short").is_err());
        assert!(check_name("has_underscore").is_err());
        let a = Account {
            id: String::new(),
            api: String::new(),
            token: String::new(),
            s3: "https://s3.eu-central-003.backblazeb2.com".into(),
            agent: ureq::agent(),
        };
        assert_eq!(a.region(), "eu-central-003");
        assert_eq!(keep_latest()[0]["daysFromHidingToDeleting"], 1);
        assert!(made_by_keepr(&json!({ "bucketInfo": { "created-by": "Keepr" } })));
        assert!(!made_by_keepr(&json!({ "bucketInfo": {} })) && !made_by_keepr(&json!({})));
    }
}
