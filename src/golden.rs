//! Golden-file snapshot verification.
//!
//! [`Golden`] compares an actual value against a stored expected
//! value. On mismatch, the result is a [`CheckResult`] with a
//! line-based diff in `detail` and `EvidenceData::Snippet` evidence.
//!
//! Set the `DEV_FIXTURES_UPDATE_GOLDEN` environment variable (for
//! example to `1`) to overwrite the stored snapshot with the actual
//! value on mismatch, which is useful for intentional changes. The
//! values `0`, `false`, `no`, `off` (any case) and the empty string
//! leave update mode off, so `DEV_FIXTURES_UPDATE_GOLDEN=0` in a CI
//! config does not rewrite snapshots.
//!
//! [`Golden`] compares text with `\r\n` and `\n` treated as the same
//! line ending, so snapshots checked out with Git's `core.autocrlf` on
//! Windows still match. Use [`BinaryGolden`] when the exact bytes,
//! including line endings, matter.

use std::borrow::Cow;
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
/// let dir = mod_tempdir::TempDir::new().unwrap();
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
    /// Line endings are normalized before comparing: `\r\n` in either
    /// the snapshot or `actual` is treated as `\n`. Any other difference,
    /// including a missing or extra final newline, is a mismatch.
    ///
    /// Verdicts:
    /// - Snapshot matches -> `Pass`.
    /// - Snapshot missing -> `Skip` with `created` tag (the snapshot
    ///   is written to disk for the first time).
    /// - Snapshot mismatch + update mode (`DEV_FIXTURES_UPDATE_GOLDEN`
    ///   set to a value other than empty, `0`, `false`, `no` or `off`)
    ///   -> `Skip` with `updated` tag (the snapshot is overwritten with
    ///   `actual`).
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

        // CRLF and LF are treated as the same line ending, so a snapshot
        // checked out with `core.autocrlf=true` on Windows still matches.
        let expected_norm = normalize_newlines(&expected);
        let actual_norm = normalize_newlines(actual);
        if actual_norm == expected_norm {
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

        let mut diff = line_diff(&expected_norm, &actual_norm);
        if !diff
            .lines()
            .any(|l| l.starts_with('-') || l.starts_with('+'))
        {
            // Every line compares equal, so the difference is invisible in
            // a line diff: a missing or extra final newline, or a bare `\r`.
            diff.push_str("note: the lines are identical; the snapshot and the actual value differ only in the final newline or a bare carriage return\n");
        }
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
/// Set the `DEV_FIXTURES_UPDATE_GOLDEN` environment variable (for
/// example to `1`) to update snapshots on intentional changes; see the
/// [module docs](self) for the values that leave it off.
///
/// # Example
///
/// ```
/// use dev_fixtures::golden::BinaryGolden;
/// let dir = mod_tempdir::TempDir::new().unwrap();
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

/// Name of the environment variable that turns on update mode.
const UPDATE_ENV: &str = "DEV_FIXTURES_UPDATE_GOLDEN";

/// `true` if `DEV_FIXTURES_UPDATE_GOLDEN` is set to a value that means
/// "on".
///
/// Unset, empty, `0`, `false`, `no` and `off` (any case, surrounding
/// whitespace ignored) mean off. Any other value, including a value that
/// is not valid Unicode, means on.
fn update_mode_enabled() -> bool {
    match std::env::var_os(UPDATE_ENV) {
        None => false,
        Some(v) => update_value_enables(&v.to_string_lossy()),
    }
}

fn update_value_enables(v: &str) -> bool {
    let v = v.trim();
    !(v.is_empty()
        || v == "0"
        || v.eq_ignore_ascii_case("false")
        || v.eq_ignore_ascii_case("no")
        || v.eq_ignore_ascii_case("off"))
}

/// Replace every `\r\n` with `\n`. Borrows when there is nothing to do.
fn normalize_newlines(s: &str) -> Cow<'_, str> {
    if s.contains("\r\n") {
        Cow::Owned(s.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(s)
    }
}

/// Largest LCS table (in cells) [`line_diff`] builds. Above this, the
/// differing middle section is reported as a block of removed lines
/// followed by a block of added lines. 4M cells of `u32` is 16 MiB.
const MAX_LCS_CELLS: usize = 4_000_000;

/// Produce a line-based diff in unified-diff-ish format using LCS.
///
/// Lines unique to `expected` are prefixed with `-`. Lines unique to
/// `actual` are prefixed with `+`. Common lines are prefixed with ` `.
///
/// The common leading and trailing lines are matched first; the
/// standard longest-common-subsequence algorithm then runs on the
/// differing middle section only, so insertions and deletions are
/// handled correctly: inserting one line in the middle produces one `+`
/// line, not a cascade of edit pairs from the position-aligned naive
/// diff. LCS needs O(N*M) memory, so when the middle section would need
/// more than [`MAX_LCS_CELLS`] cells it is shown as all `-` lines then
/// all `+` lines instead of allocating a huge table.
///
/// Hand-rolled, no external dependencies.
fn line_diff(expected: &str, actual: &str) -> String {
    let exp_lines: Vec<&str> = expected.lines().collect();
    let act_lines: Vec<&str> = actual.lines().collect();

    let prefix = exp_lines
        .iter()
        .zip(act_lines.iter())
        .take_while(|(a, b)| a == b)
        .count();
    let suffix = exp_lines[prefix..]
        .iter()
        .rev()
        .zip(act_lines[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let exp_mid = &exp_lines[prefix..exp_lines.len() - suffix];
    let act_mid = &act_lines[prefix..act_lines.len() - suffix];

    let mut ops: Vec<(char, &str)> = Vec::with_capacity(exp_lines.len() + act_lines.len());
    ops.extend(exp_lines[..prefix].iter().map(|l| (' ', *l)));
    lcs_ops(exp_mid, act_mid, &mut ops);
    ops.extend(
        exp_lines[exp_lines.len() - suffix..]
            .iter()
            .map(|l| (' ', *l)),
    );

    let mut out = String::new();
    for (sign, line) in ops {
        out.push(sign);
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Append the edit script for `exp` -> `act` to `ops`.
fn lcs_ops<'a>(exp: &[&'a str], act: &[&'a str], ops: &mut Vec<(char, &'a str)>) {
    let n = exp.len();
    let m = act.len();
    let cells = (n + 1).saturating_mul(m + 1);
    if n == 0 || m == 0 || cells > MAX_LCS_CELLS {
        ops.extend(exp.iter().map(|l| ('-', *l)));
        ops.extend(act.iter().map(|l| ('+', *l)));
        return;
    }

    // Flat LCS length table: lcs[i * w + j] = LCS of exp[..i], act[..j].
    let w = m + 1;
    let mut lcs = vec![0u32; cells];
    for i in 0..n {
        for j in 0..m {
            lcs[(i + 1) * w + j + 1] = if exp[i] == act[j] {
                lcs[i * w + j] + 1
            } else {
                lcs[i * w + j + 1].max(lcs[(i + 1) * w + j])
            };
        }
    }

    // Walk back to produce the edit script.
    let start = ops.len();
    let (mut i, mut j) = (n, m);
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && exp[i - 1] == act[j - 1] {
            ops.push((' ', exp[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || lcs[i * w + j - 1] >= lcs[(i - 1) * w + j]) {
            ops.push(('+', act[j - 1]));
            j -= 1;
        } else {
            ops.push(('-', exp[i - 1]));
            i -= 1;
        }
    }
    ops[start..].reverse();
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
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path = dir.path().join("snap.txt");
        let g = Golden::new(&path);
        let c = g.compare("greet", "hello\n");
        assert_eq!(c.verdict, Verdict::Skip);
        assert!(c.has_tag("created"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "hello\n");
    }

    #[test]
    fn matching_snapshot_passes() {
        let dir = mod_tempdir::TempDir::new().unwrap();
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
        let dir = mod_tempdir::TempDir::new().unwrap();
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
        let dir = mod_tempdir::TempDir::new().unwrap();
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
    fn crlf_snapshot_matches_lf_actual_and_back() {
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path = dir.path().join("snap.txt");
        fs::write(&path, "line one\r\nline two\r\n").unwrap();
        let c = Golden::new(&path).compare("crlf", "line one\nline two\n");
        assert_eq!(c.verdict, Verdict::Pass, "{:?}", c.detail);

        fs::write(&path, "line one\nline two\n").unwrap();
        let c = Golden::new(&path).compare("crlf", "line one\r\nline two\r\n");
        assert_eq!(c.verdict, Verdict::Pass, "{:?}", c.detail);
    }

    #[test]
    fn crlf_snapshot_with_real_change_shows_only_that_line() {
        let _g = ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(UPDATE_ENV);
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path = dir.path().join("snap.txt");
        fs::write(&path, "a\r\nb\r\nc\r\n").unwrap();
        let c = Golden::new(&path).compare("x", "a\nB\nc\n");
        assert_eq!(c.verdict, Verdict::Fail);
        let detail = c.detail.unwrap();
        assert!(detail.contains("-b\n+B\n"), "{detail}");
        assert!(!detail.contains('\r'));
    }

    #[test]
    fn trailing_newline_only_mismatch_explains_itself() {
        let _g = ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        std::env::remove_var(UPDATE_ENV);
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path = dir.path().join("snap.txt");
        fs::write(&path, "hello\n").unwrap();
        let c = Golden::new(&path).compare("x", "hello");
        assert_eq!(c.verdict, Verdict::Fail);
        assert!(c
            .detail
            .unwrap()
            .contains("differ only in the final newline"));
    }

    #[test]
    fn update_env_values() {
        for on in ["1", "true", "yes", "on", "always", " 1 "] {
            assert!(update_value_enables(on), "{on:?} should enable");
        }
        for off in ["", " ", "0", "false", "FALSE", "No", "off", " off "] {
            assert!(!update_value_enables(off), "{off:?} should not enable");
        }
    }

    #[test]
    fn update_env_zero_does_not_overwrite() {
        let _g = ENV_GUARD.lock().unwrap_or_else(|e| e.into_inner());
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path = dir.path().join("snap.txt");
        fs::write(&path, "old\n").unwrap();
        std::env::set_var(UPDATE_ENV, "0");
        let c = Golden::new(&path).compare("greet", "new\n");
        let b = BinaryGolden::new(dir.path().join("snap.bin"));
        fs::write(b.path(), [1u8]).unwrap();
        let cb = b.compare("bin", &[2u8]);
        std::env::remove_var(UPDATE_ENV);
        assert_eq!(c.verdict, Verdict::Fail);
        assert_eq!(fs::read_to_string(&path).unwrap(), "old\n");
        assert_eq!(cb.verdict, Verdict::Fail);
        assert_eq!(fs::read(b.path()).unwrap(), vec![1u8]);
    }

    #[test]
    fn line_diff_large_inputs_use_bounded_memory() {
        // 5000 x 5000 distinct lines would need a 25M-cell table; the
        // diff falls back to a removed block followed by an added block.
        let exp: String = (0..5000).map(|i| format!("e{i}\n")).collect();
        let act: String = (0..5000).map(|i| format!("a{i}\n")).collect();
        let d = line_diff(&exp, &act);
        let lines: Vec<&str> = d.lines().collect();
        assert_eq!(lines.len(), 10_000);
        assert_eq!(lines[0], "-e0");
        assert_eq!(lines[4999], "-e4999");
        assert_eq!(lines[5000], "+a0");
    }

    #[test]
    fn line_diff_trims_common_prefix_and_suffix() {
        // One changed line in the middle of a big file: exact output and
        // no quadratic table (prefix/suffix are matched first).
        let exp: Vec<String> = (0..20_000).map(|i| format!("line {i}")).collect();
        let mut act = exp.clone();
        act[10_000] = "changed".to_string();
        let exp_s = exp.join("\n");
        let act_s = act.join("\n");
        let d = line_diff(&exp_s, &act_s);
        let changed: Vec<&str> = d.lines().filter(|l| !l.starts_with(' ')).collect();
        assert_eq!(changed, vec!["-line 10000", "+changed"]);
        assert_eq!(d.lines().count(), 20_001);
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
    fn line_diff_handles_insertion_in_middle() {
        // LCS-based diff should recognize "a, b, c" inserts "x" between a and b,
        // not output a cascade of edit pairs from position-aligned diff.
        let d = line_diff("a\nb\nc\n", "a\nx\nb\nc\n");
        // Common: a, b, c. Inserted: x. So we expect:
        // " a", "+x", " b", " c"
        let lines: Vec<&str> = d.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], " a");
        assert_eq!(lines[1], "+x");
        assert_eq!(lines[2], " b");
        assert_eq!(lines[3], " c");
    }

    #[test]
    fn line_diff_handles_deletion_in_middle() {
        let d = line_diff("a\nb\nc\nd\n", "a\nc\nd\n");
        let lines: Vec<&str> = d.lines().collect();
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], " a");
        assert_eq!(lines[1], "-b");
        assert_eq!(lines[2], " c");
        assert_eq!(lines[3], " d");
    }

    #[test]
    fn line_diff_empty_inputs_yield_empty_output() {
        assert_eq!(line_diff("", ""), "");
    }

    #[test]
    fn line_diff_only_additions() {
        let d = line_diff("", "a\nb\n");
        let lines: Vec<&str> = d.lines().collect();
        assert_eq!(lines, vec!["+a", "+b"]);
    }

    #[test]
    fn line_diff_only_deletions() {
        let d = line_diff("a\nb\n", "");
        let lines: Vec<&str> = d.lines().collect();
        assert_eq!(lines, vec!["-a", "-b"]);
    }

    #[test]
    fn binary_golden_first_run_creates_snapshot() {
        let dir = mod_tempdir::TempDir::new().unwrap();
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
        let dir = mod_tempdir::TempDir::new().unwrap();
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
        let dir = mod_tempdir::TempDir::new().unwrap();
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
