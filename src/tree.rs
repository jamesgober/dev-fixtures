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
use std::path::{Component, Path, PathBuf};

/// Join `rel` onto `root`, refusing anything that would land outside
/// `root`.
///
/// `rel` must be relative and made of plain names (and `.`): no `..`,
/// no leading `/` or `\`, no drive or UNC prefix. In addition, if an
/// existing entry along the way (including the final one) is a symlink,
/// it must resolve to a location inside `root`; otherwise a write would
/// follow the link out of the tree. Errors use
/// [`io::ErrorKind::InvalidInput`] and name the offending path.
pub(crate) fn join_inside(root: &Path, rel: &Path) -> io::Result<PathBuf> {
    for c in rel.components() {
        match c {
            Component::Normal(_) | Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "fixture path `{}` must be relative and stay inside the root \
                         (no `..`, no absolute path, no drive prefix)",
                        rel.display()
                    ),
                ));
            }
        }
    }

    let mut current = root.to_path_buf();
    let mut canonical_root: Option<PathBuf> = None;
    for c in rel.components() {
        current.push(c);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                let canonical_root = match &canonical_root {
                    Some(r) => r.clone(),
                    None => {
                        let r = fs::canonicalize(root)?;
                        canonical_root = Some(r.clone());
                        r
                    }
                };
                let inside = fs::canonicalize(&current)
                    .map(|t| t.starts_with(&canonical_root))
                    .unwrap_or(false);
                if !inside {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "fixture path `{}` goes through symlink `{}`, which is dangling \
                             or points outside the root",
                            rel.display(),
                            current.display()
                        ),
                    ));
                }
            }
            Ok(_) => {}
            // Nothing exists here yet, so nothing below it can be a link.
            Err(_) => break,
        }
    }
    Ok(root.join(rel))
}

/// Escape `s` for use inside a TOML basic (double-quoted) string.
fn toml_basic_string_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

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
/// let dir = mod_tempdir::TempDir::new().unwrap();
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
    ///
    /// Entries are written in the order they were staged. Every staged
    /// path must be relative and stay inside the root: a path with `..`,
    /// an absolute path, a drive prefix, or a path that would be written
    /// through a symlink pointing outside the root fails with
    /// [`io::ErrorKind::InvalidInput`] before anything is written for that
    /// entry. A symlink's `target` is not checked, since links pointing
    /// anywhere are a legitimate fixture.
    pub fn build(self) -> io::Result<()> {
        for e in &self.entries {
            match e {
                Entry::File(rel, bytes) => {
                    let target = join_inside(&self.root, rel)?;
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&target, bytes)?;
                }
                Entry::Dir(rel) => {
                    fs::create_dir_all(join_inside(&self.root, rel)?)?;
                }
                #[cfg(unix)]
                Entry::Symlink { link, target } => {
                    let link_path = join_inside(&self.root, link)?;
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
/// let dir = mod_tempdir::TempDir::new().unwrap();
/// rust_crate(dir.path(), "sample", "0.1.0").unwrap();
/// assert!(dir.path().join("Cargo.toml").exists());
/// assert!(dir.path().join("src/lib.rs").exists());
/// ```
///
/// `name` and `version` are written as TOML strings with `"`, `\` and
/// control characters escaped, so the manifest always parses as TOML even
/// for odd values (Cargo may still reject an invalid package name).
pub fn rust_crate(root: &Path, name: &str, version: &str) -> io::Result<()> {
    let cargo = format!(
        "[package]\nname = \"{}\"\nversion = \"{}\"\nedition = \"2021\"\n\n[lib]\npath = \"src/lib.rs\"\n",
        toml_basic_string_escape(name),
        toml_basic_string_escape(version)
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
/// let dir = mod_tempdir::TempDir::new().unwrap();
/// rust_workspace(dir.path(), &["a", "b"]).unwrap();
/// assert!(dir.path().join("a/Cargo.toml").exists());
/// assert!(dir.path().join("b/Cargo.toml").exists());
/// ```
///
/// Each member name is used as a directory under `root`, so it follows
/// the same rule as [`FileTree`] paths: relative, no `..`, no absolute
/// path. An invalid member fails with [`io::ErrorKind::InvalidInput`]
/// before anything is written.
pub fn rust_workspace(root: &Path, members: &[&str]) -> io::Result<()> {
    for m in members {
        join_inside(root, Path::new(m))?;
    }
    let members_lines: String = members
        .iter()
        .map(|m| format!("    \"{}\",\n", toml_basic_string_escape(m)))
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
        let dir = mod_tempdir::TempDir::new().unwrap();
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
        let dir = mod_tempdir::TempDir::new().unwrap();
        rust_crate(dir.path(), "sample", "0.1.0").unwrap();
        let cargo = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(cargo.contains("name = \"sample\""));
        assert!(cargo.contains("version = \"0.1.0\""));
        assert!(dir.path().join("src/lib.rs").exists());
    }

    #[test]
    fn rust_workspace_layout() {
        let dir = mod_tempdir::TempDir::new().unwrap();
        rust_workspace(dir.path(), &["alpha", "beta"]).unwrap();
        let ws = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(ws.contains("\"alpha\""));
        assert!(ws.contains("\"beta\""));
        assert!(dir.path().join("alpha/Cargo.toml").exists());
        assert!(dir.path().join("beta/src/lib.rs").exists());
    }

    fn escaping_paths() -> Vec<PathBuf> {
        let mut v = vec![
            PathBuf::from("../escape.txt"),
            PathBuf::from("a/../../escape.txt"),
            PathBuf::from("./../escape.txt"),
        ];
        // An absolute path in the platform's own form.
        let abs = std::env::temp_dir().join("dev_fixtures_abs_escape.txt");
        v.push(abs);
        #[cfg(windows)]
        {
            v.push(PathBuf::from(r"C:escape.txt"));
            v.push(PathBuf::from(r"\escape.txt"));
            v.push(PathBuf::from(r"..\escape.txt"));
        }
        v
    }

    #[test]
    fn file_tree_rejects_paths_that_escape_root() {
        let outer = mod_tempdir::TempDir::new().unwrap();
        let root = outer.path().join("root");
        fs::create_dir(&root).unwrap();
        for p in escaping_paths() {
            let err = FileTree::new(&root)
                .file(p.clone(), "x")
                .build()
                .unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{}", p.display());
            assert!(err.to_string().contains("must be relative"), "{err}");
            let err = FileTree::new(&root).dir(p.clone()).build().unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::InvalidInput, "{}", p.display());
        }
        assert!(!outer.path().join("escape.txt").exists());
        assert!(!std::env::temp_dir()
            .join("dev_fixtures_abs_escape.txt")
            .exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }

    #[test]
    fn file_tree_allows_inner_dot_components() {
        let dir = mod_tempdir::TempDir::new().unwrap();
        FileTree::new(dir.path())
            .file("./a/./b.txt", "ok")
            .build()
            .unwrap();
        assert!(dir.path().join("a").join("b.txt").is_file());
    }

    #[test]
    fn rust_workspace_rejects_escaping_member() {
        let outer = mod_tempdir::TempDir::new().unwrap();
        let root = outer.path().join("ws");
        fs::create_dir(&root).unwrap();
        let err = rust_workspace(&root, &["ok", "../evil"]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(!outer.path().join("evil").exists());
        // Validation happens before any write.
        assert!(!root.join("Cargo.toml").exists());
    }

    #[test]
    fn rust_crate_escapes_toml_strings() {
        let dir = mod_tempdir::TempDir::new().unwrap();
        rust_crate(dir.path(), "we\"ird\\name", "1.0.0\n[evil]").unwrap();
        let cargo = fs::read_to_string(dir.path().join("Cargo.toml")).unwrap();
        assert!(cargo.contains(r#"name = "we\"ird\\name""#), "{cargo}");
        assert!(cargo.contains(r#"version = "1.0.0\n[evil]""#), "{cargo}");
        // No injected table header line.
        assert!(!cargo.lines().any(|l| l == "[evil]"));
    }

    #[test]
    fn toml_escape_handles_control_chars() {
        assert_eq!(
            toml_basic_string_escape("a\tb\u{1}c\u{7f}"),
            "a\\tb\\u0001c\\u007F"
        );
        assert_eq!(toml_basic_string_escape("plain-1.0"), "plain-1.0");
    }

    #[cfg(unix)]
    #[test]
    fn file_tree_refuses_to_write_through_outside_symlink() {
        let outer = mod_tempdir::TempDir::new().unwrap();
        let root = outer.path().join("root");
        let elsewhere = outer.path().join("elsewhere");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&elsewhere).unwrap();

        // Directory link pointing outside.
        let err = FileTree::new(&root)
            .symlink("out", &elsewhere)
            .file("out/pwned.txt", "x")
            .build()
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(!elsewhere.join("pwned.txt").exists());

        // Dangling file link: writing would create the target.
        let err = FileTree::new(&root)
            .symlink("dangling", elsewhere.join("created.txt"))
            .file("dangling", "x")
            .build()
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
        assert!(!elsewhere.join("created.txt").exists());

        // A link that stays inside the root is fine to write through.
        FileTree::new(&root)
            .dir("real")
            .symlink("inner", root.join("real"))
            .file("inner/ok.txt", "x")
            .build()
            .unwrap();
        assert!(root.join("real").join("ok.txt").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn symlink_unix() {
        let dir = mod_tempdir::TempDir::new().unwrap();
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
        let dir = mod_tempdir::TempDir::new().unwrap();
        // Should succeed without creating anything.
        FileTree::new(dir.path())
            .symlink("link.txt", "real.txt")
            .build()
            .unwrap();
        assert!(!dir.path().join("link.txt").exists());
    }
}
