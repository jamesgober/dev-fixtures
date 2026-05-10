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

/// A binary-content snapshot bound to a path on disk.
///
/// Like [`Golden`] but for arbitrary byte content. Useful for verifying
/// generated images, archives, encoded protocol frames, etc.
///
/// On mismatch, the diff is reported as byte-length deltas and a hex
/// preview of the first differing region — *not* a line-based diff,
/// because binary content has no meaningful line structure.
///
/// Set the `DEV_FIXTURES_UPDATE_GOLDEN` environment variable to update
/// snapshots on intentional changes.
///
/// # Example
///
/// ```
/// use dev_fixtures::golden::BinaryGolden;
/// let dir = tempfile::tempdir().unwrap();
/// let path = dir.path().join("snap.bin");
/// std::fs::write(&path, &[0u8, 1, 2, 3]).unwrap();
///
/// let g = BinaryGolden::new(&path);
/// let check = g.compare("frame", &[0u8, 1, 2, 3]);
/// assert!(matches!(check.verdict, dev_report::Verdict::Pass));
/// ```
pub struct BinaryGolden {
    path: PathBuf,
}

impl BinaryGolden {
    /// Build a binary-golden bound to a path on disk.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The path this golden is bound to.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Compare `actual` against the stored snapshot and emit a
    /// [`CheckResult`] tagged `fixtures` and `golden`.
    ///
    /// Verdicts mirror [`Golden::compare`]:
    /// - Match -> `Pass`.
    /// - Missing -> `Skip` with `created` tag (snapshot is written).
    /// - Mismatch + `DEV_FIXTURES_UPDATE_GOLDEN` set -> `Skip` with `updated`.
    /// - Mismatch -> `Fail (Error)` with `regression` tag, byte-length and
    ///   first-diff-offset evidence.
    pub fn compare(&self, name: impl AsRef<str>, actual: &[u8]) -> CheckResult {
        let name = format!("fixtures::golden::{}", name.as_ref());
        let evidence_base = vec![Evidence::numeric("actual_bytes", actual.len() as f64)];

        if !self.path.exists() {
            if let Err(e) = self.write_snapshot(actual) {
                let mut c = CheckResult::fail(name, Severity::Error)
                    .with_detail(format!("could not create snapshot: {}", e));
                c.tags = vec![
                    "fixtures".to_string(),
                    "golden".to_string(),
                    "binary".to_string(),
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
                "binary".to_string(),
                "created".to_string(),
            ];
            c.evidence = evidence_base;
            return c;
        }

        let expected = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) => {
                let mut c = CheckResult::fail(name, Severity::Error)
                    .with_detail(format!("could not read snapshot: {}", e));
                c.tags = vec![
                    "fixtures".to_string(),
                    "golden".to_string(),
                    "binary".to_string(),
                    "io_error".to_string(),
                    "regression".to_string(),
                ];
                c.evidence = evidence_base;
                return c;
            }
        };

        if actual == expected {
            let mut c = CheckResult::pass(name).with_detail("snapshot matched");
            c.tags = vec![
                "fixtures".to_string(),
                "golden".to_string(),
                "binary".to_string(),
            ];
            c.evidence = vec![
                Evidence::numeric("actual_bytes", actual.len() as f64),
                Evidence::numeric("expected_bytes", expected.len() as f64),
            ];
            return c;
        }

        if update_mode_enabled() {
            if let Err(e) = self.write_snapshot(actual) {
                let mut c = CheckResult::fail(name, Severity::Error)
                    .with_detail(format!("could not update snapshot: {}", e));
                c.tags = vec![
                    "fixtures".to_string(),
                    "golden".to_string(),
                    "binary".to_string(),
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
                "binary".to_string(),
                "updated".to_string(),
            ];
            c.evidence = evidence_base;
            return c;
        }

        let first_diff = first_diff_offset(&expected, actual);
        let preview_expected = hex_preview(&expected, first_diff, 32);
        let preview_actual = hex_preview(actual, first_diff, 32);
        let detail = format!(
            "binary mismatch (expected {} bytes, actual {} bytes, first diff at offset {})",
            expected.len(),
            actual.len(),
            first_diff
        );
        let mut c = CheckResult::fail(name, Severity::Error).with_detail(detail);
        c.tags = vec![
            "fixtures".to_string(),
            "golden".to_string(),
            "binary".to_string(),
            "regression".to_string(),
        ];
        c.evidence = vec![
            Evidence::numeric("actual_bytes", actual.len() as f64),
            Evidence::numeric("expected_bytes", expected.len() as f64),
            Evidence::numeric("first_diff_offset", first_diff as f64),
            Evidence::snippet("expected_hex_preview", preview_expected),
            Evidence::snippet("actual_hex_preview", preview_actual),
        ];
        c
    }

    fn write_snapshot(&self, content: &[u8]) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, content)
    }
}

fn first_diff_offset(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

fn hex_preview(bytes: &[u8], from: usize, len: usize) -> String {
    let end = (from + len).min(bytes.len());
    let slice = &bytes[from..end];
    let hex: String = slice
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ");
    format!("offset {}: {}", from, hex)
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

    #[test]
    fn binary_golden_first_run_creates_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snap.bin");
        let g = BinaryGolden::new(&path);
        let c = g.compare("frame", &[1u8, 2, 3, 4]);
        assert_eq!(c.verdict, Verdict::Skip);
        assert!(c.has_tag("created"));
        assert!(c.has_tag("binary"));
        let written = std::fs::read(&path).unwrap();
        assert_eq!(written, vec![1u8, 2, 3, 4]);
    }

    #[test]
    fn binary_golden_matching_passes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snap.bin");
        std::fs::write(&path, [1u8, 2, 3]).unwrap();
        let c = BinaryGolden::new(&path).compare("frame", &[1u8, 2, 3]);
        assert_eq!(c.verdict, Verdict::Pass);
        assert!(c.has_tag("binary"));
    }

    #[test]
    fn binary_golden_mismatch_fails_with_offset_and_preview() {
        let _g = ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var("DEV_FIXTURES_UPDATE_GOLDEN");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("snap.bin");
        std::fs::write(&path, [1u8, 2, 3, 4, 5]).unwrap();
        let c = BinaryGolden::new(&path).compare("frame", &[1u8, 2, 99, 4, 5]);
        assert_eq!(c.verdict, Verdict::Fail);
        assert!(c.has_tag("regression"));
        let labels: Vec<&str> = c.evidence.iter().map(|e| e.label.as_str()).collect();
        assert!(labels.contains(&"first_diff_offset"));
        assert!(labels.contains(&"expected_hex_preview"));
        assert!(labels.contains(&"actual_hex_preview"));
    }

    #[test]
    fn first_diff_offset_handles_equal_and_unequal() {
        assert_eq!(first_diff_offset(&[1, 2, 3], &[1, 2, 3]), 3);
        assert_eq!(first_diff_offset(&[1, 2, 3], &[1, 2, 9]), 2);
        assert_eq!(first_diff_offset(&[], &[]), 0);
        // Different lengths: takes the prefix length.
        assert_eq!(first_diff_offset(&[1, 2], &[1, 2, 3]), 2);
    }
}
