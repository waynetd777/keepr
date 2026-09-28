//! Where a repository's files live. Today that's a folder: on this Mac, an external drive or a
//! mounted SMB share, which all look the same once mounted. Cloud stores (S3, Google Drive,
//! OneDrive) will be other implementations of `Backend`.
//!
//! Paths are relative, '/'-separated, and never start with '/' or hold "..".

use std::fs;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

pub trait Backend: Send + Sync {
    fn read(&self, path: &str) -> io::Result<Vec<u8>>;
    fn read_at(&self, path: &str, offset: u64, len: u64) -> io::Result<Vec<u8>>;
    /// Replaces the file whole or not at all: a crash mid-write never leaves half a file.
    fn write(&self, path: &str, data: &[u8]) -> io::Result<()>;
    /// Files under `dir`, as paths relative to it ("ab/abcd.pack"), in no particular order.
    fn list(&self, dir: &str) -> io::Result<Vec<String>>;
    fn remove(&self, path: &str) -> io::Result<()>;
    fn exists(&self, path: &str) -> bool;
    fn size(&self, path: &str) -> io::Result<u64>;
    /// For people: where this is ("/Volumes/Backups/Keepr/…").
    fn describe(&self) -> String;
}

pub struct Folder {
    root: PathBuf,
}

impl Folder {
    pub fn new(root: impl Into<PathBuf>) -> Folder {
        Folder { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn at(&self, rel: &str) -> io::Result<PathBuf> {
        if rel.starts_with('/') || rel.split('/').any(|c| c == "..") {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("bad repository path {rel:?}")));
        }
        Ok(if rel.is_empty() { self.root.clone() } else { self.root.join(rel) })
    }
}

impl Backend for Folder {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        fs::read(self.at(path)?)
    }

    fn read_at(&self, path: &str, offset: u64, len: u64) -> io::Result<Vec<u8>> {
        let mut f = fs::File::open(self.at(path)?)?;
        f.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len as usize];
        f.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let p = self.at(path)?;
        let dir = p.parent().expect("a repository file has a folder");
        fs::create_dir_all(dir)?;
        let name = p.file_name().unwrap().to_string_lossy();
        let tmp = dir.join(format!(".{name}.{}.tmp", crate::Id::random().short()));
        let res = (|| {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(data)?;
            // On a share, a rename can reach the server before the data does; sync first.
            f.sync_all()?;
            drop(f);
            fs::rename(&tmp, &p)
        })();
        if res.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        res
    }

    fn list(&self, dir: &str) -> io::Result<Vec<String>> {
        let base = self.at(dir)?;
        let mut out = Vec::new();
        let mut stack = vec![(base.clone(), String::new())];
        while let Some((d, prefix)) = stack.pop() {
            let rd = match fs::read_dir(&d) {
                Ok(rd) => rd,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(e),
            };
            for e in rd {
                let e = e?;
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue; // temporary files mid-write, and Finder's .DS_Store
                }
                let rel = if prefix.is_empty() { name.clone() } else { format!("{prefix}/{name}") };
                if e.file_type()?.is_dir() {
                    stack.push((e.path(), rel));
                } else {
                    out.push(rel);
                }
            }
        }
        Ok(out)
    }

    fn remove(&self, path: &str) -> io::Result<()> {
        match fs::remove_file(self.at(path)?) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            r => r,
        }
    }

    fn exists(&self, path: &str) -> bool {
        self.at(path).is_ok_and(|p| p.exists())
    }

    fn size(&self, path: &str) -> io::Result<u64> {
        Ok(fs::metadata(self.at(path)?)?.len())
    }

    fn describe(&self) -> String {
        self.root.display().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_lists_and_refuses_escapes() {
        let t = tempfile::tempdir().unwrap();
        let f = Folder::new(t.path());
        f.write("packs/ab/one.pack", b"123456").unwrap();
        f.write("index/x.idx", b"i").unwrap();
        assert_eq!(f.read_at("packs/ab/one.pack", 2, 3).unwrap(), b"345");
        let mut l = f.list("packs").unwrap();
        l.sort();
        assert_eq!(l, vec!["ab/one.pack"]);
        assert!(f.list("nothing").unwrap().is_empty());
        assert!(f.read("../etc/passwd").is_err());
        assert!(f.write("/abs", b"").is_err());
        f.remove("index/x.idx").unwrap();
        f.remove("index/x.idx").unwrap();
        assert!(!f.exists("index/x.idx"));
    }
}
