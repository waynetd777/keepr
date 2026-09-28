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
    let ok = (3..=63).contains(&bucket.len()) && bucket.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-') && !bucket.starts_with('-') && !bucket.ends_with('-');
    if !ok {
        return Err("A bucket name is 3 to 63 lower-case letters, digits and hyphens.".into());
    }
    Ok(())
}

/// The setup, as a shell script for the AWS CLI. Run in a subshell so a failure pasted into
/// CloudShell doesn't close CloudShell.
pub fn script(region: &str, bucket: &str) -> Result<String, String> {
    check(region, bucket)?;
    // IAM user names stop at 64 characters.
    let user = if bucket.starts_with("keepr") { bucket[..bucket.len().min(64)].to_string() } else { format!("keepr-{}", &bucket[..bucket.len().min(57)]) };
    Ok(format!(
        r#"(
set -euo pipefail
export AWS_PAGER=""
R='{region}'; B='{bucket}'; U='{user}'
# ${{R}} not $R throughout: macOS's bash 3.2 reads a following non-ASCII byte as part of the name.
if aws s3api head-bucket --bucket "${{B}}" --region "${{R}}" >/dev/null 2>&1; then
  echo "The bucket ${{B}} is already there; using it."
else
  echo "Making the bucket ${{B}} in ${{R}}..."
  if [ "${{R}}" = us-east-1 ]; then aws s3api create-bucket --bucket "${{B}}" --region "${{R}}" >/dev/null
  else aws s3api create-bucket --bucket "${{B}}" --region "${{R}}" --create-bucket-configuration LocationConstraint="${{R}}" >/dev/null; fi
fi
aws s3api put-public-access-block --bucket "${{B}}" --region "${{R}}" --public-access-block-configuration BlockPublicAcls=true,IgnorePublicAcls=true,BlockPublicPolicy=true,RestrictPublicBuckets=true
if aws iam get-user --user-name "${{U}}" >/dev/null 2>&1; then
  echo "The user ${{U}} is already there; giving it a new key."
else
  echo "Making the user ${{U}}, allowed only into that bucket..."
  aws iam create-user --user-name "${{U}}" --tags Key=created-by,Value=Keepr >/dev/null
fi
aws iam put-user-policy --user-name "${{U}}" --policy-name keepr-bucket-only --policy-document '{{"Version":"2012-10-17","Statement":[{{"Effect":"Allow","Action":"s3:ListBucket","Resource":"arn:aws:s3:::'"${{B}}"'"}},{{"Effect":"Allow","Action":["s3:GetObject","s3:PutObject","s3:DeleteObject"],"Resource":"arn:aws:s3:::'"${{B}}"'/*"}}]}}'
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

/// Signs in through the browser and runs the setup. Blocks until it's done or has failed.
pub fn with_cli(region: &str, bucket: &str) -> Result<Made, String> {
    let aws = cli().ok_or("The AWS CLI (version 2.32 or later) isn't installed. Use CloudShell instead.")?;
    let script = script(region, bucket)?;
    let dir = config::data_dir().join(format!("aws-setup-{}", config::new_id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let run = || -> Result<Made, String> {
        let env = |c: &mut Command| {
            c.env("AWS_CONFIG_FILE", dir.join("config"))
                .env("AWS_SHARED_CREDENTIALS_FILE", dir.join("credentials"))
                .env("AWS_LOGIN_CACHE_DIRECTORY", dir.join("login"))
                .env("AWS_PROFILE", "keepr-setup")
                .env("AWS_REGION", region)
                .env("AWS_PAGER", "")
                .env_remove("AWS_ACCESS_KEY_ID")
                .env_remove("AWS_SECRET_ACCESS_KEY")
                .env_remove("AWS_SESSION_TOKEN")
                .stdin(Stdio::null());
        };
        std::fs::write(dir.join("config"), format!("[profile keepr-setup]\nregion = {region}\n")).map_err(|e| e.to_string())?;
        let mut login = Command::new(&aws);
        login.args(["login", "--profile", "keepr-setup", "--region", region]);
        env(&mut login);
        let out = run_for(login, Duration::from_secs(600))?;
        if !out.0 {
            return Err(format!("Signing in to AWS didn't finish. {}", last_line(&out.1)));
        }
        let mut sh = Command::new("/bin/bash");
        sh.arg("-c").arg(&script);
        // The script calls `aws` by name.
        let path = format!("{}:/usr/bin:/bin", std::path::Path::new(&aws).parent().unwrap().display());
        sh.env("PATH", path);
        env(&mut sh);
        let out = run_for(sh, Duration::from_secs(300))?;
        if !out.0 {
            return Err(explain(&out.1));
        }
        parse(&out.1)
    };
    let res = run();
    let mut logout = Command::new(&aws);
    logout.args(["logout", "--profile", "keepr-setup"]).env("AWS_CONFIG_FILE", dir.join("config")).env("AWS_LOGIN_CACHE_DIRECTORY", dir.join("login")).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    let _ = logout.status();
    let _ = std::fs::remove_dir_all(&dir);
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
        let s = script("eu-west-1", "keepr-backup-1a2b3c4d").unwrap();
        assert!(s.contains("R='eu-west-1'; B='keepr-backup-1a2b3c4d'; U='keepr-backup-1a2b3c4d'"));
        assert!(s.contains(r#""Resource":"arn:aws:s3:::'"${B}"'/*""#));
        assert!(script("eu-west-1", "Bad_Name").is_err());
        assert!(script("eu-west-1; rm", "keepr").is_err());
        let m = parse("Done.\nkeepr-setup {\"region\":\"eu-west-1\",\"bucket\":\"keepr-x1\",\"accessKey\":\"AKIA1\",\"secret\":\"s/+x\"}\n$ ").unwrap();
        assert_eq!((m.bucket.as_str(), m.access_key.as_str(), m.secret.as_str()), ("keepr-x1", "AKIA1", "s/+x"));
        assert!(serde_json::to_string(&m).unwrap().find("s/+x").is_none());
        assert!(parse("nothing here").is_err());
    }
}
