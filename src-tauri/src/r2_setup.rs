// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! Setting up Cloudflare R2 for Keepr through Cloudflare's API, from a short-lived setup token
//! the person makes in the dashboard (R2 edit, and API tokens edit). Keepr uses it to make (or,
//! for a source, find) the bucket, then makes a new token scoped to that one bucket and turns it
//! into S3 keys (R2's access key is the token's id, its secret the SHA-256 of the token), and
//! finally deletes the setup token, which is never saved.

use crate::aws_setup::{Made, Mode};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::time::Duration;

const API: &str = "https://api.cloudflare.com/client/v4";

struct Cf {
    token: String,
    agent: ureq::Agent,
}

/// Cloudflare's errors ({"errors": [{"message": …}]}) in words.
fn explain(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, r) => {
            let v: Value = r.into_json().unwrap_or_default();
            let msg = v["errors"].as_array().and_then(|a| a.first()).and_then(|e| e["message"].as_str()).unwrap_or_default().to_string();
            match code {
                401 => "Cloudflare didn't accept that token.".into(),
                403 => format!(
                    "That token isn't allowed to do this{}. It needs R2 Storage: Edit and API Tokens: Edit.",
                    if msg.is_empty() { String::new() } else { format!(" ({msg})") }
                ),
                _ if msg.contains("already exists") => "You already have a bucket with that name. Choose another.".into(),
                _ if !msg.is_empty() => format!("Cloudflare said: {msg}"),
                _ => format!("Cloudflare answered {code}."),
            }
        }
        e => format!("Couldn't reach Cloudflare: {e}"),
    }
}

impl Cf {
    fn new(token: &str) -> Result<Cf, String> {
        let tls = native_tls::TlsConnector::new().map_err(|e| e.to_string())?;
        Ok(Cf {
            token: token.trim().to_string(),
            agent: ureq::AgentBuilder::new().tls_connector(std::sync::Arc::new(tls)).timeout(Duration::from_secs(60)).build(),
        })
    }

    fn call(&self, method: &str, path: &str, body: Option<Value>) -> Result<Value, String> {
        let req = self.agent.request(method, &format!("{API}{path}")).set("Authorization", &format!("Bearer {}", self.token));
        let r = match body {
            Some(b) => req.send_json(b),
            None => req.call(),
        };
        let v: Value = r.map_err(explain)?.into_json().map_err(|e| e.to_string())?;
        Ok(v["result"].clone())
    }

    /// The accounts this token can see: (id, name).
    fn accounts(&self) -> Result<Vec<(String, String)>, String> {
        let v = self.call("GET", "/accounts", None)?;
        Ok(v.as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|a| (a["id"].as_str().unwrap_or_default().to_string(), a["name"].as_str().unwrap_or_default().to_string()))
            .collect())
    }

    fn account(&self, want: &str) -> Result<String, String> {
        let all = self.accounts()?;
        match (want.trim(), all.len()) {
            (w, _) if !w.is_empty() => all
                .iter()
                .find(|(id, name)| id == w || name == w)
                .map(|a| a.0.clone())
                .ok_or_else(|| "That token can't see that account.".to_string()),
            (_, 1) => Ok(all[0].0.clone()),
            (_, 0) => Err("That token can't see any Cloudflare account. Give it R2 Storage: Edit on your account.".into()),
            _ => Err(format!(
                "That token can see several accounts ({}). Enter the account ID too.",
                all.iter().map(|a| a.1.as_str()).collect::<Vec<_>>().join(", ")
            )),
        }
    }

    fn buckets(&self, account: &str) -> Result<Vec<String>, String> {
        let v = self.call("GET", &format!("/accounts/{account}/r2/buckets"), None)?;
        Ok(v["buckets"].as_array().cloned().unwrap_or_default().iter().filter_map(|b| b["name"].as_str().map(str::to_string)).collect())
    }

    fn permission_group(&self, name: &str) -> Result<String, String> {
        let v = self.call("GET", "/user/tokens/permission_groups", None)?;
        v.as_array()
            .cloned()
            .unwrap_or_default()
            .iter()
            .find(|g| g["name"].as_str() == Some(name))
            .and_then(|g| g["id"].as_str().map(str::to_string))
            .ok_or_else(|| format!("Cloudflare has no \"{name}\" permission any more; set the bucket up by hand."))
    }
}

fn check_name(bucket: &str) -> Result<(), String> {
    let ok = (3..=63).contains(&bucket.len())
        && bucket.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !bucket.starts_with('-')
        && !bucket.ends_with('-');
    if ok {
        Ok(())
    } else {
        Err("An R2 bucket name is 3 to 63 lower-case letters, digits and hyphens.".into())
    }
}

/// R2's S3 secret for a token: the SHA-256 of its value, in hex.
fn s3_secret(token_value: &str) -> String {
    hex::encode(Sha256::digest(token_value.as_bytes()))
}

/// The account's buckets, for choosing one to back up.
pub fn list(token: &str, account: &str) -> Result<Vec<String>, String> {
    let cf = Cf::new(token)?;
    let acc = cf.account(account)?;
    let mut b = cf.buckets(&acc)?;
    b.sort();
    Ok(b)
}

/// Makes (or, for a source, finds) the bucket and S3 keys for it alone, then deletes the setup token.
pub fn setup(token: &str, account: &str, bucket: &str, mode: Mode) -> Result<Made, String> {
    let bucket = bucket.trim();
    check_name(bucket)?;
    let cf = Cf::new(token)?;
    let acc = cf.account(account)?;
    let exists = cf.buckets(&acc)?.iter().any(|b| b == bucket);
    match (mode, exists) {
        (Mode::Source, false) => return Err(format!("There's no bucket called {bucket} in this account.")),
        (Mode::Destination, false) => {
            cf.call("POST", &format!("/accounts/{acc}/r2/buckets"), Some(json!({ "name": bucket })))?;
        }
        _ => {}
    }
    let perms: Vec<String> = match mode {
        Mode::Destination => vec![cf.permission_group("Workers R2 Storage Bucket Item Write")?],
        Mode::Source => vec![cf.permission_group("Workers R2 Storage Bucket Item Read")?],
    };
    let resource = format!("com.cloudflare.edge.r2.bucket.{acc}_default_{bucket}");
    let name = format!("Keepr {} {bucket}", if mode == Mode::Source { "(read)" } else { "backups" });
    let t = cf.call(
        "POST",
        "/user/tokens",
        Some(json!({
            "name": name,
            "policies": [{ "effect": "allow", "resources": { resource: "*" }, "permission_groups": perms.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>() }],
        })),
    )?;
    let (id, value) = (t["id"].as_str().unwrap_or_default(), t["value"].as_str().unwrap_or_default());
    if id.is_empty() || value.is_empty() {
        return Err("Cloudflare didn't return the new token.".into());
    }
    let made =
        Made::new("auto".into(), bucket.to_string(), id.to_string(), s3_secret(value), format!("https://{acc}.r2.cloudflarestorage.com"));
    let res = crate::aws_setup::finish(made);
    // The setup token has done its job; it could do far more than Keepr needs, so it goes.
    if res.is_ok() {
        if let Ok(v) = cf.call("GET", "/user/tokens/verify", None) {
            if let Some(me) = v["id"].as_str() {
                let _ = cf.call("DELETE", &format!("/user/tokens/{me}"), None);
            }
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_and_secrets() {
        assert!(check_name("keepr-backup-1a2b").is_ok());
        assert!(check_name("Keepr").is_err());
        // R2's documented derivation: the secret is the token value's SHA-256.
        assert_eq!(s3_secret("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
