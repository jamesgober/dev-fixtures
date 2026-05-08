//! Golden-file snapshot verification.
//!
//! [`Golden`] compares an actual value against a stored expected
//! value. On mismatch, the result is a [`CheckResult`] with a
//! line-based diff in `detail` and `EvidenceData::Snippet` evidence.
//!
//! Set the `DEV_FIXTURES_UPDATE_GOLDEN` environment variable to any
//! non-empty value to overwrite the stored snapshot with the actual
//! value on mismatch — useful for intentional changes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use dev_report::{CheckResult, Evidence, Severity};

/// A stored snapshot of an expected text output.
///
/// # Example
///
/// ```
/// use dev_fixtures::golden::Golden;
/// let dir = tempfile::tempdir().unwrap();
/// let path = dir.path().join("snap.txt");
/// std::fs::write(&path, "hello\n").unwrap();
///
/// let g = Golden::new(&path);
/// let check = g.compare("greet", "hello\n");
/// assert!(matches!(check.verdict, dev_report::Verdict::Pass));
/// ```
pub struct Golden {
    path: PathBuf,
}

impl Golden {
    /// Build a golden bound to a path on disk. The path may not exist
    /// yet; on first run the snapshot is created and verdict is `Skip`
    /// with a `created` tag.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Compare `actual` against the stored snapshot and emit a
    /// [`CheckResult`] tagged `fixtures` and `golden`.
    ///
    /// Verdicts:
    /// - Snapshot matches -> `Pass`.
    /// - Snapshot missing -> `Skip` with `created` tag (the snapshot
    ///   is written to disk for the first time).
    /// - Snapshot mismatch + update mode -> `Skip` with `updated` tag
    ///   (the snapshot is overwritten with `actual`).
    /// - Snapshot mismatch (default) -> `Fail (Error)` with `regression`
    ///   tag, line-based diff in `detail`, and full actual/expected
    ///   as snippet evidence.
    pub fn compare(&self, name: impl AsRef<str>, actual: &str) -> CheckResult {
        let name = format!("fixtures::golden::{}", name.as_ref());
        let evidence_base = vec![Evidence::numeric("actual_bytes", actual.len() as f64)];

        if !self.path.exists() {
            // First run: write the snapshot, return Skip.
            if let Err(e) = self.write_snapshot(actual) {
                let mut c = CheckResult::fail(name, Severity::Error)
                    .with_detail(format!("could not create snapshot: {}", e));
                c.tags = vec![
                    "fixtures".to_string(),
                    "golden".to_string(),
                    "io_error".to_string(),
                    "regression".to_string(),
                ];
                c.evidence = evidence_base;
                return c;
            }
            let mut c = CheckResult::skip(name)
                .with_detail(format!("created snapshot at {}", self.path.display()));
            c.tags = vec![
                "fixtures".to_string(),
                "golden".to_string(),
                "created".to_string(),
            ];
            c.evidence = evidence_base;
            return c;
        }

        let expected = match fs::read_to_string(&self.path) {
            Ok(s) => s,
            Err(e) => {
                let mut c = CheckResult::fail(name, Severity::Error)
                    .with_detail(format!("could not read snapshot: {}", e));
                c.tags = vec![
                    "fixtures".to_string(),
                    "golden".to_string(),
                    "io_error".to_string(),
                    "regression".to_string(),
                ];
                c.evidence = evidence_base;
                return c;
            }
        };

        if actual == expected {
            let mut c = CheckResult::pass(name).with_detail("snapshot matched");
            c.tags = vec!["fixtures".to_string(), "golden".to_string()];
            c.evidence = vec![
                Evidence::numeric("actual_bytes", actual.len() as f64),
                Evidence::numeric("expected_bytes", expected.len() as f64),
            ];
            return c;
        }

        // Mismatch.
        if update_mode_enabled() {
            if let Err(e) = self.write_snapshot(actual) {
                let mut c = CheckResult::fail(name, Severity::Error)
                    .with_detail(format!("could not update snapshot: {}", e));
                c.tags = vec![
                    "fixtures".to_string(),
                    "golden".to_string(),
                    "io_error".to_string(),
                    "regression".to_string(),
                ];
                c.evidence = evidence_base;
                return c;
            }
            let mut c = CheckResult::skip(name)
                .with_detail(format!("updated snapshot at {}", self.path.display()));
            c.tags = vec![
                "fixtures".to_string(),
                "golden".to_string(),
                "updated".to_string(),
            ];
            c.evidence = evidence_base;
            return c;
        }

        let diff = line_diff(&expected, actual);
        let mut c = CheckResult::fail(name, Severity::Error)
            .with_detail(format!("snapshot mismatch:\n{}", diff));
        c.tags = vec![
            "fixtures".to_string(),
            "golden".to_string(),
            "regression".to_string(),
        ];
        c.evidence = vec![
            Evidence::numeric("actual_bytes", actual.len() as f64),
            Evidence::numeric("expected_bytes", expected.len() as f64),
            Evidence::snippet("expected", expected),
            Evidence::snippet("actual", actual.to_string()),
            Evidence::snippet("diff", diff),
        ];
        c
    }

    fn write_snapshot(&self, content: &str) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, content)
    }

    /// The path this golden is bound to.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// `true` if `DEV_FIXTURES_UPDATE_GOLDEN` is set to a non-empty value.
fn update_mode_enabled() -> bool {
    std::env::var("DEV_FIXTURES_UPDATE_GOLDEN")
        .map(|v| !v.is_empty())
        .unwrap_or(false)
}

/// Produce a line-based diff in unified-diff-ish format.
///
/// Lines unique to `expected` are prefixed with `-`. Lines unique to
/// `actual` are prefixed with `+`. Common lines are prefixed with ` `.
/// Implementation is naive (LCS-free); fine for short snapshots.
fn line_diff(expected: &str, actual: &str) -> String {
    let exp_lines: Vec<&str> = expected.lines().collect();
    let act_lines: Vec<&str> = actual.lines().collect();
    let mut out = String::new();
    let max = exp_lines.len().max(act_lines.len());
    for i in 0..max {
        match (exp_lines.get(i), act_lines.get(i)) {
            (Some(e), Some(a)) if e == a => {
                out.push(' ');
                out.push_str(e);
                out.push('\n');
            }
            (Some(e), Some(a)) => {
                out.push('-');
                out.push_str(e);
                out.push('\n');
                out.push('+');
                out.push_str(a);
                out.push('\n');
            }
            (Some(e), None) => {
                out.push('-');
                out.push_str(e);
                out.push('\n');
            }
            (None, Some(a)) => {
                out.push('+');
                out.push_str(a);
                out.push('\n');
            }
            (None, None) => break,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use dev_report::Verdict;
    use std::sync::Mutex;

    // Serialize tests that touch DEV_FIXTURES_UPDATE_GOLDEN. Without
    // this guard, parallel tests race on the env var.
    static ENV_GUARD: Mutex<()> = Mutex::new(());

    #[test]
    fn first_run_creates_snapshot_and_skips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snap.txt");
        let g = Golden::new(&path);
        let c = g.compare("greet", "hello\n");
        assert_eq!(c.verdict, Verdict::Skip);
        assert!(c.has_tag("created"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\n");
    }

    #[test]
    fn matching_snapshot_passes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snap.txt");
        fs::write(&path, "hello\n").unwrap();
        let c = Golden::new(&path).compare("greet", "hello\n");
        assert_eq!(c.verdict, Verdict::Pass);
    }

    #[test]
    fn mismatching_snapshot_fails_with_diff() {
        let _g = ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        // Make sure update mode is off for this test.
        std::env::remove_var("DEV_FIXTURES_UPDATE_GOLDEN");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snap.txt");
        fs::write(&path, "hello\nworld\n").unwrap();
        let c = Golden::new(&path).compare("greet", "hello\nuniverse\n");
        assert_eq!(c.verdict, Verdict::Fail);
        assert!(c.has_tag("regression"));
        let detail = c.detail.as_deref().unwrap();
        assert!(detail.contains("-world"));
        assert!(detail.contains("+universe"));
    }

    #[test]
    fn update_mode_overwrites_snapshot() {
        let _g = ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snap.txt");
        fs::write(&path, "old\n").unwrap();
        std::env::set_var("DEV_FIXTURES_UPDATE_GOLDEN", "1");
        let c = Golden::new(&path).compare("greet", "new\n");
        std::env::remove_var("DEV_FIXTURES_UPDATE_GOLDEN");
        assert_eq!(c.verdict, Verdict::Skip);
        assert!(c.has_tag("updated"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new\n");
    }

    #[test]
    fn line_diff_marks_added_and_removed() {
        let d = line_diff("a\nb\nc\n", "a\nx\nc\n");
        assert!(d.contains(" a"));
        assert!(d.contains("-b"));
        assert!(d.contains("+x"));
        assert!(d.contains(" c"));
    }
}
