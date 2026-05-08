//! File-tree builders.
//!
//! [`FileTree`] is a more general builder than [`TempProject`]: it
//! materializes a tree under any caller-chosen root, supports
//! Rust-workspace shortcuts, and can create symlinks where the
//! platform supports them.
//!
//! [`TempProject`]: crate::TempProject

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A staged file or directory entry.
enum Entry {
    File(PathBuf, Vec<u8>),
    Dir(PathBuf),
    #[cfg(unix)]
    Symlink {
        link: PathBuf,
        target: PathBuf,
    },
}

/// Builder for a tree of files and directories under a chosen root.
///
/// # Example
///
/// ```
/// use dev_fixtures::tree::FileTree;
/// let dir = tempfile::tempdir().unwrap();
/// FileTree::new(dir.path())
///     .file("README.md", "hello")
///     .dir("src")
///     .file("src/lib.rs", "pub fn x() {}")
///     .build()
///     .unwrap();
/// assert!(dir.path().join("src/lib.rs").exists());
/// ```
pub struct FileTree {
    root: PathBuf,
    entries: Vec<Entry>,
}

impl FileTree {
    /// Build a tree rooted at `root`. The directory MUST exist on
    /// build; the builder does not create it.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            entries: Vec::new(),
        }
    }

    /// Stage a UTF-8 text file at `relative_path`.
    pub fn file(mut self, relative_path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        self.entries.push(Entry::File(
            relative_path.into(),
            contents.into().into_bytes(),
        ));
        self
    }

    /// Stage a binary file at `relative_path`.
    pub fn bytes(
        mut self,
        relative_path: impl Into<PathBuf>,
        contents: impl Into<Vec<u8>>,
    ) -> Self {
        self.entries
            .push(Entry::File(relative_path.into(), contents.into()));
        self
    }

    /// Stage an empty directory at `relative_path`.
    pub fn dir(mut self, relative_path: impl Into<PathBuf>) -> Self {
        self.entries.push(Entry::Dir(relative_path.into()));
        self
    }

    /// Stage a symlink. Available on Unix only; a no-op on Windows.
    ///
    /// `link` is the relative path of the symlink itself; `target` is
    /// the path the symlink points to.
    #[cfg_attr(not(unix), allow(unused_mut, unused_variables))]
    pub fn symlink(mut self, link: impl Into<PathBuf>, target: impl Into<PathBuf>) -> Self {
        #[cfg(unix)]
        {
            self.entries.push(Entry::Symlink {
                link: link.into(),
                target: target.into(),
            });
        }
        // On Windows, silently no-op (symlinks need admin privilege).
        self
    }

    /// Materialize the tree on disk.
    pub fn build(self) -> io::Result<()> {
        for e in &self.entries {
            match e {
                Entry::File(rel, bytes) => {
                    let target = self.root.join(rel);
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&target, bytes)?;
                }
                Entry::Dir(rel) => {
                    fs::create_dir_all(self.root.join(rel))?;
                }
                #[cfg(unix)]
                Entry::Symlink { link, target } => {
                    let link_path = self.root.join(link);
                    if let Some(parent) = link_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    std::os::unix::fs::symlink(target, &link_path)?;
                }
            }
        }
        Ok(())
    }
}

/// Convenience: build a minimal Rust crate layout under `root`.
///
/// Creates `Cargo.toml` and `src/lib.rs`. Returns the relative paths
/// of the files written.
///
/// # Example
///
/// ```
/// use dev_fixtures::tree::rust_crate;
/// let dir = tempfile::tempdir().unwrap();
/// rust_crate(dir.path(), "sample", "0.1.0").unwrap();
/// assert!(dir.path().join("Cargo.toml").exists());
/// assert!(dir.path().join("src/lib.rs").exists());
/// ```
pub fn rust_crate(root: &Path, name: &str, version: &str) -> io::Result<()> {
    let cargo = format!(
        "[package]\nname = \"{}\"\nversion = \"{}\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        name, version
    );
    FileTree::new(root)
        .file("Cargo.toml", cargo)
        .file("src/lib.rs", "//! Sample crate.\n")
        .build()
}

/// Convenience: build a multi-crate Rust workspace under `root`.
///
/// Creates a top-level `Cargo.toml` with `[workspace]` and a member
/// crate per name in `members`. Each member gets its own
/// `Cargo.toml` and `src/lib.rs`.
///
/// # Example
///
/// ```
/// use dev_fixtures::tree::rust_workspace;
/// let dir = tempfile::tempdir().unwrap();
/// rust_workspace(dir.path(), &["a", "b"]).unwrap();
/// assert!(dir.path().join("a/Cargo.toml").exists());
/// assert!(dir.path().join("b/Cargo.toml").exists());
/// ```
pub fn rust_workspace(root: &Path, members: &[&str]) -> io::Result<()> {
    let members_lines: String = members
        .iter()
        .map(|m| format!("    \"{}\",\n", m))
        .collect();
    let workspace_toml = format!(
        "[workspace]\nresolver = \"2\"\nmembers = [\n{}]\n",
        members_lines
    );
    FileTree::new(root)
        .file("Cargo.toml", workspace_toml)
        .build()?;
    for m in members {
        rust_crate(&root.join(m), m, "0.0.0")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_basic_tree() {
        let dir = tempfile::tempdir().unwrap();
        FileTree::new(dir.path())
            .file("a.txt", "hello")
            .dir("empty")
            .file("nested/b.txt", "world")
            .build()
            .unwrap();
        assert!(dir.path().join("a.txt").exists());
        assert!(dir.path().join("empty").is_dir());
        assert_eq!(
            fs::read_to_string(dir.path().join("nested/b.txt")).unwrap(),
            "world"
        );
    }

    #[test]
    fn rust_crate_layout() {
        let dir = tempfile::tempdir().unwrap();
        rust_crate(dir.path(), "sample", "0.1.0").unwrap();
        let cargo = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"sample\""));
        assert!(cargo.contains("version = \"0.1.0\""));
        assert!(dir.path().join("src/lib.rs").exists());
    }

    #[test]
    fn rust_workspace_layout() {
        let dir = tempfile::tempdir().unwrap();
        rust_workspace(dir.path(), &["alpha", "beta"]).unwrap();
        let ws = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(ws.contains("\"alpha\""));
        assert!(ws.contains("\"beta\""));
        assert!(dir.path().join("alpha/Cargo.toml").exists());
        assert!(dir.path().join("beta/src/lib.rs").exists());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_unix() {
        let dir = tempfile::tempdir().unwrap();
        FileTree::new(dir.path())
            .file("real.txt", "data")
            .symlink("link.txt", "real.txt")
            .build()
            .unwrap();
        assert!(dir.path().join("link.txt").exists());
    }

    #[cfg(windows)]
    #[test]
    fn symlink_no_op_on_windows() {
        let dir = tempfile::tempdir().unwrap();
        // Should succeed without creating anything.
        FileTree::new(dir.path())
            .symlink("link.txt", "real.txt")
            .build()
            .unwrap();
        assert!(!dir.path().join("link.txt").exists());
    }
}
