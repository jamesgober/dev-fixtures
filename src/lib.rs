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
//!
//! ## Modules
//!
//! - [`tree`] — `FileTree` builder with workspace and symlink helpers.
//! - [`adversarial`] — generators for oversized, malformed, and unusual inputs.
//! - [`golden`] — snapshot-based verification with `dev-report` integration.
//! - [`mock`] — deterministic mock data (CSV, JSON, bytes).

#![cfg_attr(docsrs, feature(doc_cfg))]
#![warn(missing_docs)]
#![warn(rust_2018_idioms)]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use dev_report::{CheckResult, Evidence, Producer, Report, Severity};

pub mod adversarial;
pub mod golden;
pub mod mock;
pub mod tree;

/// A temporary project directory that auto-cleans on drop.
///
/// Holds an internal `tempfile::TempDir`. The temp directory is deleted
/// when this value is dropped.
///
/// # Example
///
/// ```
/// use dev_fixtures::TempProject;
///
/// let p = TempProject::new()
///     .with_file("README.md", "hello")
///     .build()
///     .unwrap();
/// assert!(p.path().join("README.md").exists());
/// ```
pub struct TempProject {
    _dir: tempfile::TempDir,
    files: Vec<(PathBuf, Vec<u8>)>,
}

impl TempProject {
    /// Begin building a temp project.
    ///
    /// Returns a [`TempProjectBuilder`] (not `Self`); call `.build()`
    /// on it to materialize the directory.
    #[allow(clippy::new_ret_no_self)]
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
///
/// # Example
///
/// ```
/// use dev_fixtures::TempProject;
///
/// let _ = TempProject::new()
///     .with_file("a.txt", "hello")
///     .with_bytes("b.bin", vec![1, 2, 3])
///     .build()
///     .unwrap();
/// ```
#[derive(Default)]
pub struct TempProjectBuilder {
    files: Vec<(PathBuf, Vec<u8>)>,
}

impl TempProjectBuilder {
    /// Stage a UTF-8 text file at `relative_path` inside the temp project.
    pub fn with_file(
        mut self,
        relative_path: impl Into<PathBuf>,
        contents: impl Into<String>,
    ) -> Self {
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
///
/// # Example
///
/// ```
/// use dev_fixtures::Fixture;
/// use std::io;
///
/// struct MyFixture;
/// impl Fixture for MyFixture {
///     type Output = u32;
///     fn set_up(&mut self) -> io::Result<u32> { Ok(42) }
///     fn tear_down(&mut self) -> io::Result<()> { Ok(()) }
/// }
/// ```
pub trait Fixture {
    /// Output produced when the fixture is set up.
    type Output;

    /// Set the fixture up. Returns the output (e.g. a path, a handle).
    fn set_up(&mut self) -> io::Result<Self::Output>;

    /// Tear the fixture down. MUST be idempotent.
    fn tear_down(&mut self) -> io::Result<()>;

    /// Run set_up and emit a [`CheckResult`] tagged `fixtures`.
    ///
    /// On `Ok(_)`, verdict is `Pass` with `setup_ok=1` evidence.
    /// On `Err(e)`, verdict is `Fail (Critical)` with `setup_failed`
    /// + `regression` tags and `setup_ok=0` evidence.
    ///
    /// Default impl wraps `set_up` and discards the output. Override
    /// if you need to inspect the produced value before returning.
    ///
    /// # Example
    ///
    /// ```
    /// use dev_fixtures::Fixture;
    /// use std::io;
    ///
    /// struct OkFixture;
    /// impl Fixture for OkFixture {
    ///     type Output = ();
    ///     fn set_up(&mut self) -> io::Result<()> { Ok(()) }
    ///     fn tear_down(&mut self) -> io::Result<()> { Ok(()) }
    /// }
    /// let check = OkFixture.set_up_checked("ok");
    /// assert!(check.has_tag("fixtures"));
    /// ```
    fn set_up_checked(&mut self, name: impl Into<String>) -> CheckResult {
        let name = format!("fixtures::{}", name.into());
        match self.set_up() {
            Ok(_) => {
                let mut c = CheckResult::pass(name).with_detail("set_up succeeded");
                c.tags = vec!["fixtures".to_string()];
                c.evidence = vec![Evidence::numeric("setup_ok", 1.0)];
                c
            }
            Err(e) => {
                let mut c = CheckResult::fail(name, Severity::Critical)
                    .with_detail(format!("set_up failed: {}", e));
                c.tags = vec![
                    "fixtures".to_string(),
                    "setup_failed".to_string(),
                    "regression".to_string(),
                ];
                c.evidence = vec![Evidence::numeric("setup_ok", 0.0)];
                c
            }
        }
    }
}

/// Producer wrapper that runs a self-test of fixture lifecycle and
/// emits a [`Report`].
///
/// The producer takes a closure that builds and tears down a fixture.
/// It emits one `CheckResult` for set_up and one for tear_down.
///
/// # Example
///
/// ```no_run
/// use dev_fixtures::{FixtureProducer, TempProject};
/// use dev_report::Producer;
///
/// let producer = FixtureProducer::new(
///     "temp_project_lifecycle",
///     "0.1.0",
///     || {
///         let p = TempProject::new()
///             .with_file("x.txt", "hi")
///             .build()?;
///         // path() exists, files exist; dropping `p` cleans up.
///         drop(p);
///         Ok(())
///     },
/// );
/// let report = producer.produce();
/// assert_eq!(report.checks.len(), 1);
/// ```
pub struct FixtureProducer<F>
where
    F: Fn() -> io::Result<()>,
{
    name: String,
    subject_version: String,
    run: F,
}

impl<F> FixtureProducer<F>
where
    F: Fn() -> io::Result<()>,
{
    /// Build a new producer.
    pub fn new(name: impl Into<String>, subject_version: impl Into<String>, run: F) -> Self {
        Self {
            name: name.into(),
            subject_version: subject_version.into(),
            run,
        }
    }
}

impl<F> Producer for FixtureProducer<F>
where
    F: Fn() -> io::Result<()>,
{
    fn produce(&self) -> Report {
        let check_name = format!("fixtures::{}", self.name);
        let started = std::time::Instant::now();
        let check = match (self.run)() {
            Ok(()) => {
                let elapsed = started.elapsed();
                let mut c = CheckResult::pass(check_name)
                    .with_duration_ms(elapsed.as_millis() as u64)
                    .with_detail("fixture lifecycle completed cleanly");
                c.tags = vec!["fixtures".to_string()];
                c.evidence = vec![
                    Evidence::numeric("setup_ok", 1.0),
                    Evidence::numeric("elapsed_ms", elapsed.as_millis() as f64),
                ];
                c
            }
            Err(e) => {
                let mut c = CheckResult::fail(check_name, Severity::Critical)
                    .with_detail(format!("fixture lifecycle failed: {}", e));
                c.tags = vec![
                    "fixtures".to_string(),
                    "setup_failed".to_string(),
                    "regression".to_string(),
                ];
                c.evidence = vec![Evidence::numeric("setup_ok", 0.0)];
                c
            }
        };
        let mut r = Report::new(self.name.clone(), self.subject_version.clone())
            .with_producer("dev-fixtures");
        r.push(check);
        r.finish();
        r
    }
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

    #[test]
    fn temp_project_cleans_up_on_panic() {
        let path = {
            let project = TempProject::new()
                .with_file("x.txt", "panicky")
                .build()
                .unwrap();
            let path = project.path().to_path_buf();
            // Simulate panic-then-drop by capturing the panic.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _proj = project;
                panic!("test panic");
            }));
            assert!(result.is_err());
            path
        };
        assert!(!path.exists());
    }

    struct OkFixture;
    impl Fixture for OkFixture {
        type Output = ();
        fn set_up(&mut self) -> io::Result<()> {
            Ok(())
        }
        fn tear_down(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailingFixture;
    impl Fixture for FailingFixture {
        type Output = ();
        fn set_up(&mut self) -> io::Result<()> {
            Err(io::Error::other("boom"))
        }
        fn tear_down(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn set_up_checked_pass_path() {
        let c = OkFixture.set_up_checked("ok");
        assert_eq!(c.verdict, dev_report::Verdict::Pass);
        assert!(c.has_tag("fixtures"));
    }

    #[test]
    fn set_up_checked_fail_path() {
        let c = FailingFixture.set_up_checked("bad");
        assert_eq!(c.verdict, dev_report::Verdict::Fail);
        assert!(c.has_tag("setup_failed"));
        assert!(c.has_tag("regression"));
    }

    #[test]
    fn fixture_producer_emits_report() {
        let producer = FixtureProducer::new("smoke", "0.1.0", || {
            let _p = TempProject::new().with_file("a.txt", "x").build()?;
            Ok(())
        });
        let report = producer.produce();
        assert_eq!(report.checks.len(), 1);
        assert_eq!(report.producer.as_deref(), Some("dev-fixtures"));
        assert_eq!(report.overall_verdict(), dev_report::Verdict::Pass);
    }

    #[test]
    fn fixture_producer_failed_setup_yields_fail() {
        let producer = FixtureProducer::new("broken", "0.1.0", || {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "nope"))
        });
        let report = producer.produce();
        assert_eq!(report.overall_verdict(), dev_report::Verdict::Fail);
    }
}
