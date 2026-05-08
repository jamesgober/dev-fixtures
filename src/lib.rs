//! # dev-fixtures
//!
//! Repeatable test environments, sample data, and controlled inputs for
//! Rust. Part of the `dev-*` verification suite.
//!
//! ## Why
//!
//! Tests are only useful if they are repeatable. AI agents in particular
//! need fixtures that:
//!
//! - Build the same way every time
//! - Clean themselves up
//! - Provide both happy-path and adversarial inputs
//!
//! `dev-fixtures` provides primitives for building those environments.
//!
//! ## Quick example
//!
//! ```no_run
//! use dev_fixtures::TempProject;
//!
//! let project = TempProject::new()
//!     .with_file("Cargo.toml", "[package]\nname = \"sample\"\n")
//!     .with_file("src/lib.rs", "pub fn answer() -> u32 { 42 }")
//!     .build()
//!     .unwrap();
//!
//! // project.path() points at a temp directory.
//! // It is deleted automatically when `project` is dropped.
//! ```

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// A temporary project directory that auto-cleans on drop.
///
/// Holds an internal `tempfile::TempDir`. The temp directory is deleted
/// when this value is dropped.
pub struct TempProject {
    _dir: tempfile::TempDir,
    files: Vec<(PathBuf, Vec<u8>)>,
}

impl TempProject {
    /// Begin building a temp project.
    pub fn new() -> TempProjectBuilder {
        TempProjectBuilder::default()
    }

    /// Path to the root of the temp project.
    pub fn path(&self) -> &Path {
        self._dir.path()
    }

    /// Files declared at build time. Useful for diagnostics.
    pub fn declared_files(&self) -> impl Iterator<Item = (&Path, &[u8])> {
        self.files.iter().map(|(p, b)| (p.as_path(), b.as_slice()))
    }
}

/// Builder for [`TempProject`].
#[derive(Default)]
pub struct TempProjectBuilder {
    files: Vec<(PathBuf, Vec<u8>)>,
}

impl TempProjectBuilder {
    /// Stage a UTF-8 text file at `relative_path` inside the temp project.
    pub fn with_file(mut self, relative_path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        self.files
            .push((relative_path.into(), contents.into().into_bytes()));
        self
    }

    /// Stage a binary file at `relative_path` inside the temp project.
    pub fn with_bytes(
        mut self,
        relative_path: impl Into<PathBuf>,
        contents: impl Into<Vec<u8>>,
    ) -> Self {
        self.files.push((relative_path.into(), contents.into()));
        self
    }

    /// Build the temp project on disk.
    pub fn build(self) -> io::Result<TempProject> {
        let dir = tempfile::tempdir()?;
        for (rel, bytes) in &self.files {
            let target = dir.path().join(rel);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, bytes)?;
        }
        Ok(TempProject {
            _dir: dir,
            files: self.files,
        })
    }
}

/// A trait for any fixture that can be set up and torn down.
///
/// Implementors should ensure that `tear_down` is idempotent and that
/// `set_up` followed by `tear_down` always returns the system to a clean
/// state.
pub trait Fixture {
    /// Output produced when the fixture is set up.
    type Output;

    /// Set the fixture up. Returns the output (e.g. a path, a handle).
    fn set_up(&mut self) -> io::Result<Self::Output>;

    /// Tear the fixture down. MUST be idempotent.
    fn tear_down(&mut self) -> io::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_project_builds_and_writes_files() {
        let project = TempProject::new()
            .with_file("a.txt", "hello")
            .with_file("nested/b.txt", "world")
            .build()
            .unwrap();

        let a = project.path().join("a.txt");
        let b = project.path().join("nested").join("b.txt");
        assert!(a.exists());
        assert!(b.exists());
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "hello");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "world");
    }

    #[test]
    fn temp_project_cleans_up_on_drop() {
        let path = {
            let project = TempProject::new()
                .with_file("x.txt", "ephemeral")
                .build()
                .unwrap();
            project.path().to_path_buf()
        };
        assert!(!path.exists());
    }
}
