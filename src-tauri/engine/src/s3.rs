// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! A repository in an S3 bucket: Amazon S3, or anything that speaks its API (Cloudflare R2,
//! Wasabi, MinIO, a NAS's S3 server). Plain blocking HTTPS with AWS Signature Version 4, signed
//! over each body's SHA-256 so the server refuses a body that changed on the way.
//!
//! An object is written whole or not at all, which is what `Backend::write` asks for. Keys are
//! the prefix plus the repository path ("keepr/Documents 1c9b7f/packs/ab/abcd.pack").

use crate::backend::Backend;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    /// "https://s3.eu-west-1.amazonaws.com", "https://<account>.r2.cloudflarestorage.com",
    /// "http://nas.local:9000". No path.
    pub endpoint: String,
    /// "eu-west-1"; R2 takes "auto".
    pub region: String,
    pub bucket: String,
    /// Inside the bucket, without slashes at either end; empty for its top.
    pub prefix: String,
    pub access_key: String,
    pub secret_key: String,
}

pub struct S3 {
    cfg: Config,
    /// "https", and the host (with a port, if any) requests go to.
    scheme: String,
    host: String,
    /// The bucket goes in the path (MinIO, most NASes, dotted bucket names) rather than the host.
    path_style: bool,
    agent: ureq::Agent,
}

const TRIES: u32 = 4;

impl S3 {
    pub fn new(cfg: Config) -> io::Result<S3> {
        let ep = cfg.endpoint.trim().trim_end_matches('/');
        let ep = if ep.contains("://") { ep.to_string() } else { format!("https://{ep}") };
        let (scheme, rest) = ep.split_once("://").unwrap();
        let host = rest.split('/').next().unwrap_or_default().to_lowercase();
        if host.is_empty() {
            return Err(bad("The endpoint needs a server name, such as s3.eu-west-1.amazonaws.com."));
        }
        if cfg.bucket.trim().is_empty() {
            return Err(bad("Enter the bucket's name."));
        }
        // Amazon wants the bucket in the host name; a dotted name would break its certificate.
        let path_style = !host.ends_with(".amazonaws.com") || cfg.bucket.contains('.');
        let tls = native_tls::TlsConnector::new().map_err(io::Error::other)?;
        let agent = ureq::AgentBuilder::new()
            .tls_connector(std::sync::Arc::new(tls))
            .timeout_connect(Duration::from_secs(20))
            .timeout_read(Duration::from_secs(120))
            .timeout_write(Duration::from_secs(120))
            .build();
        let cfg = Config {
            prefix: cfg.prefix.trim_matches('/').to_string(),
            bucket: cfg.bucket.trim().to_string(),
            region: if cfg.region.trim().is_empty() { "us-east-1".into() } else { cfg.region.trim().to_string() },
            ..cfg
        };
        Ok(S3 { cfg, scheme: scheme.to_string(), host, path_style, agent })
    }

    /// The same bucket, one folder further in.
    pub fn within(&self, sub: &str) -> S3 {
        let sub = sub.trim_matches('/');
        let prefix = match (self.cfg.prefix.is_empty(), sub.is_empty()) {
            (_, true) => self.cfg.prefix.clone(),
            (true, false) => sub.to_string(),
            (false, false) => format!("{}/{sub}", self.cfg.prefix),
        };
        S3 {
            cfg: Config { prefix, ..self.cfg.clone() },
            scheme: self.scheme.clone(),
            host: self.host.clone(),
            path_style: self.path_style,
            agent: self.agent.clone(),
        }
    }

    fn key(&self, rel: &str) -> io::Result<String> {
        if rel.starts_with('/') || rel.split('/').any(|c| c == "..") {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("bad repository path {rel:?}")));
        }
        Ok(match (self.cfg.prefix.is_empty(), rel.is_empty()) {
            (true, _) => rel.to_string(),
            (false, true) => self.cfg.prefix.clone(),
            (false, false) => format!("{}/{rel}", self.cfg.prefix),
        })
    }

    /// Checks the bucket is there and these keys may use it. Amazon says which region a bucket
    /// is really in; the answer names it.
    pub fn check_bucket(&self) -> io::Result<()> {
        match self.send("HEAD", "", &[], &[], None) {
            Ok(_) => Ok(()),
            Err(Fail::Status(301, _, Some(region), _)) | Err(Fail::Status(400, _, Some(region), _)) if region != self.cfg.region => {
                Err(bad(&format!("That bucket is in the {region} region. Choose {region} and try again.")))
            }
            Err(Fail::Status(404, ..)) => {
                Err(bad(&format!("There's no bucket called {} there. Make it first, in the service's own console.", self.cfg.bucket)))
            }
            Err(Fail::Status(403, ..)) => Err(bad(
                "The keys were refused for this bucket. Check the access key, the secret, and that the key may read and write the bucket.",
            )),
            Err(e) => Err(e.into_io(&self.cfg)),
        }
    }

    /// One signed request, tried again after a pause if the network or the server stumbled.
    fn send(
        &self,
        method: &str,
        key: &str,
        query: &[(&str, &str)],
        body: &[u8],
        range: Option<(u64, u64)>,
    ) -> Result<ureq::Response, Fail> {
        let mut last = None;
        for attempt in 0..TRIES {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(500 << attempt));
            }
            match self.send_once(method, key, query, body, range) {
                Ok(r) => return Ok(r),
                Err(e) if e.retry() => last = Some(e),
                Err(e) => return Err(e),
            }
        }
        Err(last.unwrap())
    }

    fn send_once(
        &self,
        method: &str,
        key: &str,
        query: &[(&str, &str)],
        body: &[u8],
        range: Option<(u64, u64)>,
    ) -> Result<ureq::Response, Fail> {
        let (host, path) = if self.path_style {
            (self.host.clone(), format!("/{}/{}", uri_encode(&self.cfg.bucket, true), uri_encode(key, false)))
        } else {
            (format!("{}.{}", self.cfg.bucket, self.host), format!("/{}", uri_encode(key, false)))
        };
        let path = if key.is_empty() { path.trim_end_matches('/').to_string().max("/".into()) } else { path };
        let mut q: Vec<(String, String)> = query.iter().map(|(k, v)| (uri_encode(k, true), uri_encode(v, true))).collect();
        q.sort();
        let qs = q.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("&");
        let now = chrono::Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let day = now.format("%Y%m%d").to_string();
        let payload = hex::encode(Sha256::digest(body));
        let canonical = format!("{method}\n{path}\n{qs}\nhost:{host}\nx-amz-content-sha256:{payload}\nx-amz-date:{amz_date}\n\nhost;x-amz-content-sha256;x-amz-date\n{payload}");
        let scope = format!("{day}/{}/s3/aws4_request", self.cfg.region);
        let to_sign = format!("AWS4-HMAC-SHA256\n{amz_date}\n{scope}\n{}", hex::encode(Sha256::digest(canonical.as_bytes())));
        let mut k = hmac(format!("AWS4{}", self.cfg.secret_key).as_bytes(), day.as_bytes());
        for part in [self.cfg.region.as_bytes(), b"s3", b"aws4_request"] {
            k = hmac(&k, part);
        }
        let sig = hex::encode(hmac(&k, to_sign.as_bytes()));
        let auth = format!(
            "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={sig}",
            self.cfg.access_key
        );
        let url = format!("{}://{host}{path}{}{qs}", self.scheme, if qs.is_empty() { "" } else { "?" });
        let mut req =
            self.agent.request(method, &url).set("x-amz-date", &amz_date).set("x-amz-content-sha256", &payload).set("authorization", &auth);
        if let Some((from, len)) = range {
            req = req.set("range", &format!("bytes={from}-{}", from + len - 1));
        }
        let res = if method == "PUT" { req.send_bytes(body) } else { req.call() };
        match res {
            Ok(r) => Ok(r),
            Err(ureq::Error::Status(code, r)) => {
                let region = r.header("x-amz-bucket-region").map(str::to_string);
                let text = r.into_string().unwrap_or_default();
                Err(Fail::Status(code, tag(&text, "Code").unwrap_or_default(), region, tag(&text, "Message").unwrap_or_default()))
            }
            Err(e) => Err(Fail::Network(e.to_string())),
        }
    }

    /// Every object under `prefix` (a full key, ending '/' unless empty): its key, size,
    /// modified time and ETag.
    fn list_objects(&self, prefix: &str) -> io::Result<Vec<(String, u64, i64, String)>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        loop {
            let mut q = vec![("list-type", "2"), ("prefix", prefix)];
            if let Some(t) = &token {
                q.push(("continuation-token", t.as_str()));
            }
            let r = self.send("GET", "", &q, &[], None).map_err(|e| e.into_io(&self.cfg))?;
            let text = String::from_utf8_lossy(&S3::body(r, 64 << 20)?).to_string();
            for c in text.split("<Contents>").skip(1) {
                let Some(k) = tag(c, "Key") else { continue };
                let size = tag(c, "Size").and_then(|s| s.parse().ok()).unwrap_or(0);
                let mtime = tag(c, "LastModified")
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok())
                    .and_then(|t| t.timestamp_nanos_opt())
                    .unwrap_or(0);
                let etag = tag(c, "ETag").unwrap_or_default().trim_matches('"').to_string();
                out.push((k, size, mtime, etag));
            }
            token = (tag(&text, "IsTruncated").as_deref() == Some("true")).then(|| tag(&text, "NextContinuationToken")).flatten();
            if token.is_none() {
                return Ok(out);
            }
        }
    }

    /// A GET and its body, tried again whole: a connection that drops part-way through the body
    /// fails after the headers have come, which `send` alone doesn't try again. `range` is
    /// (offset, length).
    fn get(&self, path: &str, key: &str, range: Option<(u64, u64)>) -> io::Result<Vec<u8>> {
        let mut last = None;
        for attempt in 0..TRIES {
            if attempt > 0 {
                std::thread::sleep(Duration::from_millis(500 << attempt));
            }
            let r = match self.send_once("GET", key, &[], &[], range) {
                Ok(r) => r,
                Err(e) if e.retry() => {
                    last = Some(e.into_io(&self.cfg));
                    continue;
                }
                Err(e) => return Err(e.into_io(&self.cfg)),
            };
            let want: Option<u64> = match range {
                Some((_, len)) => Some(len),
                None => r.header("content-length").and_then(|l| l.parse().ok()),
            };
            match S3::body(r, range.map_or(u64::MAX, |(_, len)| len)) {
                Ok(out) if want.is_none_or(|w| w == out.len() as u64) => return Ok(out),
                Ok(_) => last = Some(io::Error::new(io::ErrorKind::UnexpectedEof, format!("{path} came back short"))),
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap())
    }

    fn body(r: ureq::Response, limit: u64) -> io::Result<Vec<u8>> {
        let mut out = Vec::with_capacity(r.header("content-length").and_then(|l| l.parse().ok()).unwrap_or(0).min(limit as usize));
        r.into_reader().take(limit).read_to_end(&mut out)?;
        Ok(out)
    }
}

impl Backend for S3 {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let key = self.key(path)?;
        self.get(path, &key, None)
    }

    fn read_at(&self, path: &str, offset: u64, len: u64) -> io::Result<Vec<u8>> {
        if len == 0 {
            return Ok(vec![]);
        }
        let key = self.key(path)?;
        self.get(path, &key, Some((offset, len)))
    }

    fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let key = self.key(path)?;
        self.send("PUT", &key, &[], data, None).map(|_| ()).map_err(|e| e.into_io(&self.cfg))
    }

    fn list(&self, dir: &str) -> io::Result<Vec<String>> {
        let base = self.key(dir)?;
        let prefix = if base.is_empty() { String::new() } else { format!("{base}/") };
        Ok(self
            .list_objects(&prefix)?
            .into_iter()
            .filter_map(|(k, ..)| k.strip_prefix(&prefix).filter(|r| !r.is_empty() && !r.ends_with('/')).map(str::to_string))
            .collect())
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        let key = self.key(path)?;
        match self.send("DELETE", &key, &[], &[], None) {
            Ok(_) | Err(Fail::Status(404, ..)) => Ok(()),
            Err(e) => Err(e.into_io(&self.cfg)),
        }
    }

    fn exists(&self, path: &str) -> bool {
        self.key(path).is_ok_and(|k| self.send("HEAD", &k, &[], &[], None).is_ok())
    }

    fn size(&self, path: &str) -> io::Result<u64> {
        let key = self.key(path)?;
        let r = self.send("HEAD", &key, &[], &[], None).map_err(|e| e.into_io(&self.cfg))?;
        r.header("content-length").and_then(|l| l.parse().ok()).ok_or_else(|| io::Error::other("the server didn't say how big it is"))
    }

    fn describe(&self) -> String {
        if self.cfg.prefix.is_empty() {
            format!("s3://{}", self.cfg.bucket)
        } else {
            format!("s3://{}/{}", self.cfg.bucket, self.cfg.prefix)
        }
    }
}

/// A bucket as a source to back up. Only reads: listing and downloading.
impl crate::backup::Remote for S3 {
    fn objects(&self) -> io::Result<Vec<crate::backup::RemoteObject>> {
        let prefix = if self.cfg.prefix.is_empty() { String::new() } else { format!("{}/", self.cfg.prefix) };
        Ok(self
            .list_objects(&prefix)?
            .into_iter()
            .filter_map(|(k, size, mtime, tag)| {
                Some(crate::backup::RemoteObject { key: k.strip_prefix(&prefix)?.to_string(), size, mtime, tag })
            })
            .collect())
    }

    fn open(&self, key: &str) -> io::Result<Box<dyn io::Read + Send>> {
        // The key as the bucket has it: no checks on its shape, since Keepr didn't make it.
        let full = if self.cfg.prefix.is_empty() { key.to_string() } else { format!("{}/{key}", self.cfg.prefix) };
        let r = self.send("GET", &full, &[], &[], None).map_err(|e| e.into_io(&self.cfg))?;
        Ok(Box::new(r.into_reader()))
    }
}

enum Fail {
    /// Status, S3's error code ("NoSuchKey"), the bucket's real region if the server said, and its message.
    Status(u16, String, Option<String>, String),
    Network(String),
}

impl Fail {
    fn retry(&self) -> bool {
        match self {
            Fail::Network(_) => true,
            Fail::Status(code, s3, ..) => *code >= 500 || *code == 429 || s3 == "RequestTimeout" || s3 == "SlowDown",
        }
    }

    fn into_io(self, cfg: &Config) -> io::Error {
        match self {
            Fail::Network(e) => io::Error::other(format!("Couldn't reach {}: {e}", cfg.endpoint)),
            Fail::Status(404, s3, ..) if s3 != "NoSuchBucket" => io::Error::new(io::ErrorKind::NotFound, "not in the bucket"),
            Fail::Status(code, s3, region, msg) => {
                let why = match s3.as_str() {
                    "NoSuchBucket" => format!("There's no bucket called {}.", cfg.bucket),
                    "InvalidAccessKeyId" => "The access key isn't known to this service.".into(),
                    "SignatureDoesNotMatch" => "The secret key doesn't match the access key.".into(),
                    "AccessDenied" => "These keys may not do that in this bucket.".into(),
                    "RequestTimeTooSkewed" => {
                        "This Mac's clock is too far out for the service. Set the date and time automatically in System Settings.".into()
                    }
                    "PermanentRedirect" | "AuthorizationHeaderMalformed" if region.is_some() => {
                        format!("The bucket is in the {} region.", region.unwrap())
                    }
                    _ if !msg.is_empty() => format!("{msg} ({s3})"),
                    _ => format!("The service answered {code}."),
                };
                io::Error::new(if code == 403 { io::ErrorKind::PermissionDenied } else { io::ErrorKind::Other }, why)
            }
        }
    }
}

fn bad(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, m.to_string())
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut m = Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes any key");
    m.update(data);
    m.finalize().into_bytes().to_vec()
}

/// SigV4's URI encoding: everything but unreserved characters, and '/' too unless it separates a key's parts.
fn uri_encode(s: &str, slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b'/' if !slash => out.push('/'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The text of the first <name>…</name> in some XML, unescaped.
fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{name}>"))? + start;
    Some(xml[start..end].replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_like_amazon() {
        // The signing key from Amazon's SigV4 documentation example.
        let mut k = hmac(b"AWS4wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY", b"20120215");
        for part in [&b"us-east-1"[..], b"iam", b"aws4_request"] {
            k = hmac(&k, part);
        }
        assert_eq!(hex::encode(k), "f4780e2d9f65fa895f9c67b32ce1baf0b0d8a43505a000a1a9e090d414db404d");
        assert_eq!(uri_encode("Documents 1c/packs/a+b.pack", false), "Documents%201c/packs/a%2Bb.pack");
        assert_eq!(uri_encode("a/b", true), "a%2Fb");
        assert_eq!(tag("<X><Key>a&amp;b</Key></X>", "Key").as_deref(), Some("a&b"));
    }

    #[test]
    fn keys_sit_under_the_prefix() {
        let s = S3::new(Config {
            endpoint: "s3.eu-west-1.amazonaws.com".into(),
            region: "eu-west-1".into(),
            bucket: "b".into(),
            prefix: "/keepr/".into(),
            access_key: "a".into(),
            secret_key: "s".into(),
        })
        .unwrap();
        assert!(!s.path_style);
        let p = s.within("Docs 1c9b7f");
        assert_eq!(p.key("packs/ab/x.pack").unwrap(), "keepr/Docs 1c9b7f/packs/ab/x.pack");
        assert_eq!(p.describe(), "s3://b/keepr/Docs 1c9b7f");
        assert!(p.key("../x").is_err());
        assert!(
            S3::new(Config {
                endpoint: "http://nas.local:9000".into(),
                region: "".into(),
                bucket: "b".into(),
                prefix: "".into(),
                access_key: "a".into(),
                secret_key: "s".into()
            })
            .unwrap()
            .path_style
        );
    }

    /// Against a real bucket: KEEPR_S3_TEST="endpoint region bucket access secret" cargo test -- --ignored s3
    #[test]
    #[ignore]
    fn round_trip_on_a_real_bucket() {
        let v = std::env::var("KEEPR_S3_TEST").expect("KEEPR_S3_TEST");
        let p: Vec<&str> = v.split_whitespace().collect();
        let s = S3::new(Config {
            endpoint: p[0].into(),
            region: p[1].into(),
            bucket: p[2].into(),
            prefix: format!("keepr-test-{}", crate::Id::random().short()),
            access_key: p[3].into(),
            secret_key: p[4].into(),
        })
        .unwrap();
        s.check_bucket().unwrap();
        s.write("packs/ab/one.pack", b"123456").unwrap();
        s.write("index/x.idx", b"i").unwrap();
        assert_eq!(s.read("packs/ab/one.pack").unwrap(), b"123456");
        assert_eq!(s.read_at("packs/ab/one.pack", 2, 3).unwrap(), b"345");
        assert_eq!(s.size("packs/ab/one.pack").unwrap(), 6);
        assert_eq!(s.list("packs").unwrap(), vec!["ab/one.pack"]);
        assert!(s.read("nope").unwrap_err().kind() == io::ErrorKind::NotFound);
        s.remove("index/x.idx").unwrap();
        s.remove("packs/ab/one.pack").unwrap();
        assert!(!s.exists("index/x.idx"));
    }
}
