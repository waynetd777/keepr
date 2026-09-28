//! A directory as a snapshot keeps it: its entries, sorted by name. A tree is itself a blob, named
//! by its contents, so a folder that didn't change between backups is the same tree and costs
//! nothing to keep again.

use crate::Id;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum NodeKind {
    #[serde(rename = "f")]
    File,
    #[serde(rename = "d")]
    Dir,
    #[serde(rename = "l")]
    Symlink,
}

fn is_zero(v: &u64) -> bool {
    *v == 0
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Node {
    pub name: String,
    #[serde(rename = "k")]
    pub kind: NodeKind,
    /// A file's size; for a folder, the total size of the files below it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub size: u64,
    /// For a folder, how many files are below it.
    #[serde(rename = "n", default, skip_serializing_if = "is_zero")]
    pub files: u64,
    /// Modified time, nanoseconds since 1970.
    #[serde(rename = "m")]
    pub mtime: i64,
    /// Status-change time; with the inode, what tells a file apart from one swapped in with the
    /// same size and modified time.
    #[serde(rename = "c", default)]
    pub ctime: i64,
    #[serde(rename = "i", default, skip_serializing_if = "is_zero")]
    pub inode: u64,
    pub mode: u32,
    /// A file's chunks, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<Id>,
    /// A directory's tree.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtree: Option<Id>,
    /// Where a symlink points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

impl Node {
    /// What a file's contents are, as one id: two versions are the same when this is.
    pub fn content_key(&self) -> String {
        match self.kind {
            NodeKind::File => {
                let mut h = blake3::Hasher::new();
                for c in &self.content {
                    h.update(&c.0);
                }
                format!("{}:{}", self.size, h.finalize().to_hex())
            }
            NodeKind::Dir => self.subtree.map(|t| t.hex()).unwrap_or_default(),
            NodeKind::Symlink => self.target.clone().unwrap_or_default(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Tree {
    pub nodes: Vec<Node>,
}

impl Tree {
    pub fn get(&self, name: &str) -> Option<&Node> {
        self.nodes.binary_search_by(|n| n.name.as_str().cmp(name)).ok().map(|i| &self.nodes[i])
    }

    pub fn sort(&mut self) {
        self.nodes.sort_by(|a, b| a.name.cmp(&b.name));
    }
}
