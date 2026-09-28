//! Setting up an Amazon S3 bucket for Keepr, so nobody has to find their way around IAM: a
//! private bucket, a user that may only list, read, write and delete in it, and that user's
//! access key, saved in the Keychain.
//!
//! Two ways in. With the AWS CLI installed, `aws login` signs in through the browser with the
//! person's own console login, and Keepr runs the setup with those short-lived credentials, kept
//! in a temporary folder of its own (never ~/.aws) and deleted afterwards. Without the CLI, Keepr
//! hands over the same script to paste into AWS CloudShell, which prints one line to paste back.

use crate::{config, keychain, places};
use serde::{Deserialize, Serialize};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// What setup made. The secret key goes straight to the Keychain, not back to the page.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Made {
    pub region: String,
    pub bucket: String,
    pub access_key: String,
    #[serde(default, skip_serializing)]
    secret: String,
}

const CANDIDATES: [&str; 3] = ["/opt/homebrew/bin/aws", "/usr/local/bin/aws", "/usr/bin/aws"];

/// The AWS CLI, if it's installed and new enough to have `aws login` (2.32 and later).
pub fn cli() -> Option<String> {
    let aws = CANDIDATES.iter().find(|p| std::path::Path::new(p).exists())?;
    let out = Command::new(aws).arg("--version").output().ok()?;
    let v = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    let ver = v.split_whitespace().next()?.strip_prefix("aws-cli/")?;
    let mut n = ver.split('.').map(|x| x.parse::<u32>().unwrap_or(0));
    let (major, minor) = (n.next()?, n.next().unwrap_or(0));
    (major > 2 || (major == 2 && minor >= 32)).then(|| aws.to_string())
}

/// A bucket name that's very likely free: bucket names are shared by every AWS account.
pub fn suggest_bucket() -> String {
    format!("keepr-backup-{}", &config::new_id()[..8])
}

fn check(region: &str, bucket: &str) -> Result<(), String> {
    if region.is_empty() || !region.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') {
        return Err("Choose a region, such as eu-west-1.".into());
    }
    let ok = (3..=63).contains(&bucket.len()) && bucket.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '.') && !bucket.starts_with('-') && !bucket.ends_with('-');
    if !ok {
        return Err("A bucket name is 3 to 63 lower-case letters, digits, dots and hyphens.".into());
    }
    Ok(())
}

/// What the key is for: keeping backups in a bucket Keepr makes, or backing up a bucket that's
/// already there, which Keepr may then only list and read.
#[derive(Deserialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Mode {
    Destination,
    Source,
}

/// The setup, as a shell script for the AWS CLI. Run in a subshell so a failure pasted into
/// CloudShell doesn't close CloudShell.
pub fn script(region: &str, bucket: &str, mode: Mode) -> Result<String, String> {
    check(region, bucket)?;
    // IAM user names stop at 64 characters.
    let user = match mode {
        Mode::Destination if bucket.starts_with("keepr") => bucket[..bucket.len().min(64)].to_string(),
        Mode::Destination => format!("keepr-{}", &bucket[..bucket.len().min(57)]),
        Mode::Source => format!("keepr-read-{}", &bucket[..bucket.len().min(52)]),
    };
    let bucket_part = match mode {
        Mode::Destination => r#"if aws s3api head-bucket --bucket "${B}" --region "${R}" >/dev/null 2>&1; then
  echo "The bucket ${B} is already there; using it."
else
  echo "Making the bucket ${B} in ${R}..."
  if [ "${R}" = us-east-1 ]; then aws s3api create-bucket --bucket "${B}" --region "${R}" >/dev/null
  else aws s3api create-bucket --bucket "${B}" --region "${R}" --create-bucket-configuration LocationConstraint="${R}" >/dev/null; fi
fi
aws s3api put-public-access-block --bucket "${B}" --region "${R}" --public-access-block-configuration BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
A='"s3:GetObject","s3:PutObject","s3:DeleteObject"'
WHAT="allowed only into that bucket""#,
        // The bucket is wherever it is; its region is asked rather than assumed.
        Mode::Source => r#"L=$(aws s3api get-bucket-location --bucket "${B}" --query LocationConstraint --output text)
case "${L}" in None|null|"") R=us-east-1 ;; EU) R=eu-west-1 ;; *) R="${L}" ;; esac
echo "The bucket ${B} is in ${R}."
A='"s3:GetObject"'
WHAT="allowed only to list and read that bucket""#,
    };
    Ok(format!(
        r#"(
set -euo pipefail
export AWS_PAGER=""
R='{region}'; B='{bucket}'; U='{user}'
# ${{R}} not $R throughout: macOS's bash 3.2 reads a following non-ASCII byte as part of the name.
{bucket_part}
if aws iam get-user --user-name "${{U}}" >/dev/null 2>&1; then
  echo "The user ${{U}} is already there; giving it a new key."
else
  echo "Making the user ${{U}}, ${{WHAT}}..."
  aws iam create-user --user-name "${{U}}" --tags Key=created-by,Value=Keepr >/dev/null
fi
aws iam put-user-policy --user-name "${{U}}" --policy-name keepr-bucket-only --policy-document '{{"Version":"2012-10-17","Statement":[{{"Effect":"Allow","Action":"s3:ListBucket","Resource":"arn:aws:s3:::'"${{B}}"'"}},{{"Effect":"Allow","Action":['"${{A}}"'],"Resource":"arn:aws:s3:::'"${{B}}"'/*"}}]}}'
# A user may have two keys: make room by removing the oldest Keepr made before.
OLD=$(aws iam list-access-keys --user-name "${{U}}" --query 'AccessKeyMetadata[].AccessKeyId' --output text)
set -- $OLD
if [ $# -ge 2 ]; then aws iam delete-access-key --user-name "${{U}}" --access-key-id "$1"; fi
K=$(aws iam create-access-key --user-name "${{U}}" --query 'AccessKey.[AccessKeyId,SecretAccessKey]' --output text)
set -- $K
echo
echo "Done. Copy the next line into Keepr:"
echo "keepr-setup {{\"region\":\"${{R}}\",\"bucket\":\"${{B}}\",\"accessKey\":\"$1\",\"secret\":\"$2\"}}"
)
"#
    ))
}

/// The line the script ends with, from whatever was pasted around it.
pub fn parse(text: &str) -> Result<Made, String> {
    let line = text.lines().rev().find_map(|l| l.trim().strip_prefix("keepr-setup ")).ok_or("Paste the line that starts with keepr-setup.")?;
    let m: Made = serde_json::from_str(line.trim()).map_err(|_| "That line is incomplete. Copy all of it.")?;
    check(&m.region, &m.bucket)?;
    if m.access_key.is_empty() || m.secret.is_empty() {
        return Err("That line has no access key in it.".into());
    }
    Ok(m)
}

/// Saves the secret and waits for the new key to work: IAM takes a few seconds to tell S3.
pub fn finish(m: Made) -> Result<Made, String> {
    keychain::set(&keychain::s3_account(&m.access_key), &m.secret)?;
    let place = config::S3 { endpoint: endpoint(&m.region), region: m.region.clone(), bucket: m.bucket.clone(), prefix: String::new(), access_key: m.access_key.clone(), name: None };
    let b = places::s3_backend(&place, Some(m.secret.clone()), "")?;
    let start = Instant::now();
    loop {
        match b.check_bucket() {
            Ok(()) => return Ok(m),
            Err(e) if start.elapsed() > Duration::from_secs(90) => return Err(format!("The bucket and key were made, but the key doesn't work yet: {e}")),
            Err(_) => std::thread::sleep(Duration::from_secs(3)),
        }
    }
}

pub fn endpoint(region: &str) -> String {
    format!("https://s3.{region}.amazonaws.com")
}

/// A browser sign-in, kept for a few minutes so choosing a bucket doesn't mean signing in twice.
struct Session {
    aws: String,
    dir: std::path::PathBuf,
    region: String,
    at: Instant,
}

static SESSION: std::sync::Mutex<Option<Session>> = std::sync::Mutex::new(None);

impl Session {
    fn command(&self, program: &str) -> Command {
        let mut c = Command::new(program);
        c.env("AWS_CONFIG_FILE", self.dir.join("config"))
            .env("AWS_SHARED_CREDENTIALS_FILE", self.dir.join("credentials"))
            .env("AWS_LOGIN_CACHE_DIRECTORY", self.dir.join("login"))
            .env("AWS_PROFILE", "keepr-setup")
            .env("AWS_REGION", &self.region)
            .env("AWS_PAGER", "")
            // The script calls `aws` by name.
            .env("PATH", format!("{}:/usr/bin:/bin", std::path::Path::new(&self.aws).parent().unwrap().display()))
            .env_remove("AWS_ACCESS_KEY_ID")
            .env_remove("AWS_SECRET_ACCESS_KEY")
            .env_remove("AWS_SESSION_TOKEN")
            .stdin(Stdio::null());
        c
    }

    fn start(region: &str) -> Result<Session, String> {
        let aws = cli().ok_or("The AWS CLI (version 2.32 or later) isn't installed. Use CloudShell instead.")?;
        let dir = config::data_dir().join(format!("aws-setup-{}", config::new_id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        std::fs::write(dir.join("config"), format!("[profile keepr-setup]\nregion = {region}\n")).map_err(|e| e.to_string())?;
        let s = Session { aws: aws.clone(), dir, region: region.to_string(), at: Instant::now() };
        let mut login = s.command(&aws);
        login.args(["login", "--profile", "keepr-setup", "--region", region]);
        match run_for(login, Duration::from_secs(600)) {
            Ok((true, _)) => Ok(s),
            Ok((false, out)) => {
                s.end();
                Err(format!("Signing in to AWS didn't finish. {}", last_line(&out)))
            }
            Err(e) => {
                s.end();
                Err(e)
            }
        }
    }

    fn end(self) {
        let mut logout = self.command(&self.aws);
        logout.args(["logout", "--profile", "keepr-setup"]).stdout(Stdio::null()).stderr(Stdio::null());
        let _ = logout.status();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The sign-in from a moment ago, else a new one.
fn session(region: &str) -> Result<Session, String> {
    if let Some(s) = SESSION.lock().unwrap().take() {
        if s.at.elapsed() < Duration::from_secs(15 * 60) {
            return Ok(s);
        }
        s.end();
    }
    Session::start(region)
}

/// Forgets the sign-in, if one is kept.
pub fn end_session() {
    if let Some(s) = SESSION.lock().unwrap().take() {
        s.end();
    }
}

/// Signs in through the browser and lists the account's buckets. The sign-in is kept for the
/// setup that follows.
pub fn buckets(region: &str) -> Result<Vec<String>, String> {
    check(region, "keepr")?;
    let s = session(region)?;
    let mut c = s.command("/bin/bash");
    c.arg("-c").arg("aws s3api list-buckets --query 'Buckets[].Name' --output text");
    let out = run_for(c, Duration::from_secs(60));
    let res = match out {
        Ok((true, text)) => Ok(text.split_whitespace().map(str::to_string).collect()),
        Ok((false, text)) => Err(explain(&text)),
        Err(e) => Err(e),
    };
    *SESSION.lock().unwrap() = Some(s);
    res
}

/// Signs in through the browser (or uses the sign-in from a moment ago) and runs the setup.
/// Blocks until it's done or has failed.
pub fn with_cli(region: &str, bucket: &str, mode: Mode) -> Result<Made, String> {
    let script = script(region, bucket, mode)?;
    let s = session(region)?;
    let mut sh = s.command("/bin/bash");
    sh.arg("-c").arg(&script);
    let res = match run_for(sh, Duration::from_secs(300)) {
        Ok((true, out)) => parse(&out),
        Ok((false, out)) => Err(explain(&out)),
        Err(e) => Err(e),
    };
    s.end();
    finish(res?)
}

/// Runs a command with a time limit; returns whether it succeeded and all it printed.
fn run_for(mut c: Command, limit: Duration) -> Result<(bool, String), String> {
    let mut child = c.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| e.to_string())?;
    let (mut o, mut e) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
    let ot = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut o, &mut s);
        s
    });
    let et = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::io::Read::read_to_string(&mut e, &mut s);
        s
    });
    let start = Instant::now();
    let ok = loop {
        if let Some(st) = child.try_wait().map_err(|e| e.to_string())? {
            break st.success();
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Gave up waiting for AWS.".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    Ok((ok, ot.join().unwrap_or_default() + &et.join().unwrap_or_default()))
}

fn last_line(s: &str) -> String {
    s.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or_default().trim().to_string()
}

/// The AWS CLI's error, in words.
fn explain(out: &str) -> String {
    if out.contains("BucketAlreadyExists") || out.contains("BucketAlreadyOwnedByYou") {
        "That bucket name is taken. Choose another.".into()
    } else if out.contains("EntityAlreadyExists") {
        "A Keepr user for that bucket already exists in your account. Choose another bucket name, or delete the old user in IAM.".into()
    } else if out.contains("AccessDenied") || out.contains("not authorized") {
        "Your AWS login may not make buckets and users. Sign in as an administrator, or ask whoever runs the account.".into()
    } else {
        let tail: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty() && !l.contains("SecretAccessKey")).collect();
        format!("Setting up didn't finish: {}", tail[tail.len().saturating_sub(3)..].join(" · "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn script_and_its_answer() {
        let s = script("eu-west-1", "keepr-backup-1a2b3c4d", Mode::Destination).unwrap();
        assert!(s.contains("R='eu-west-1'; B='keepr-backup-1a2b3c4d'; U='keepr-backup-1a2b3c4d'"));
        assert!(s.contains(r#""Resource":"arn:aws:s3:::'"${B}"'/*""#));
        let r = script("eu-west-1", "photos", Mode::Source).unwrap();
        assert!(r.contains("U='keepr-read-photos'") && r.contains(r#"A='"s3:GetObject"'"#) && !r.contains("create-bucket"));
        assert!(script("eu-west-1", "Bad_Name", Mode::Destination).is_err());
        assert!(script("eu-west-1; rm", "keepr", Mode::Source).is_err());
        let m = parse("Done.\nkeepr-setup {\"region\":\"eu-west-1\",\"bucket\":\"keepr-x1\",\"accessKey\":\"AKIA1\",\"secret\":\"s/+x\"}\n$ ").unwrap();
        assert_eq!((m.bucket.as_str(), m.access_key.as_str(), m.secret.as_str()), ("keepr-x1", "AKIA1", "s/+x"));
        assert!(serde_json::to_string(&m).unwrap().find("s/+x").is_none());
        assert!(parse("nothing here").is_err());
    }
}
