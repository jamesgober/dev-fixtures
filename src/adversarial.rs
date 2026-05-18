//! Adversarial input generators.
//!
//! These generators produce inputs that exercise failure paths:
//! oversized files, malformed UTF-8, unusual filenames. Every
//! generator is deterministic given a seed (where applicable).
//!
//! ## Use cases
//!
//! - **Oversized**: confirm that buffer-allocation paths handle
//!   files larger than common assumptions.
//! - **Malformed UTF-8**: confirm that decoders return errors instead
//!   of panicking on invalid bytes.
//! - **Unusual names**: confirm that path-handling code copes with
//!   Unicode, emoji, very long names, etc.

use std::fs;
use std::io;
use std::path::Path;

/// Write `size_bytes` of zeroes to `path`. The parent directory MUST
/// exist.
///
/// Backed by `fs::write` of a single buffer; for very large sizes use
/// [`oversized_sparse`] instead to save memory.
///
/// # Example
///
/// ```
/// use dev_fixtures::adversarial::oversized_zeros;
/// let dir = mod_tempdir::TempDir::new().unwrap();
/// let path = dir.path().join("big.bin");
/// oversized_zeros(&path, 4096).unwrap();
/// assert_eq!(std::fs::metadata(&path).unwrap().len(), 4096);
/// ```
pub fn oversized_zeros(path: &Path, size_bytes: u64) -> io::Result<()> {
    let buf = vec![0u8; size_bytes as usize];
    fs::write(path, buf)
}

/// Write `size_bytes` to `path` using `set_len`, which on most
/// platforms creates a sparse file (no actual disk space used until
/// written to).
///
/// Useful for testing very large file handling without consuming
/// disk space.
///
/// # Example
///
/// ```
/// use dev_fixtures::adversarial::oversized_sparse;
/// let dir = mod_tempdir::TempDir::new().unwrap();
/// let path = dir.path().join("sparse.bin");
/// oversized_sparse(&path, 1_000_000).unwrap();
/// assert_eq!(std::fs::metadata(&path).unwrap().len(), 1_000_000);
/// ```
pub fn oversized_sparse(path: &Path, size_bytes: u64) -> io::Result<()> {
    let f = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)?;
    f.set_len(size_bytes)?;
    Ok(())
}

/// Write a deliberately malformed UTF-8 byte sequence to `path`.
///
/// The file contains valid ASCII followed by bytes that no UTF-8
/// decoder accepts (`0xFF, 0xFE, 0xFD, 0xFC` — invalid lead bytes).
///
/// # Example
///
/// ```
/// use dev_fixtures::adversarial::malformed_utf8;
/// let dir = mod_tempdir::TempDir::new().unwrap();
/// let path = dir.path().join("bad.txt");
/// malformed_utf8(&path).unwrap();
/// let bytes = std::fs::read(&path).unwrap();
/// assert!(std::str::from_utf8(&bytes).is_err());
/// ```
pub fn malformed_utf8(path: &Path) -> io::Result<()> {
    let mut bytes = b"valid ascii prefix\n".to_vec();
    bytes.extend_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC]);
    bytes.extend_from_slice(b"more after the bad bytes\n");
    fs::write(path, bytes)
}

/// Write `n` deterministic bytes to `path` using a splitmix64-derived
/// stream seeded by `seed`.
///
/// # Example
///
/// ```
/// use dev_fixtures::adversarial::random_bytes;
/// let dir = mod_tempdir::TempDir::new().unwrap();
/// let path = dir.path().join("rand.bin");
/// random_bytes(&path, 32, 42).unwrap();
/// let a = std::fs::read(&path).unwrap();
/// random_bytes(&path, 32, 42).unwrap();
/// let b = std::fs::read(&path).unwrap();
/// assert_eq!(a, b); // deterministic from seed
/// ```
pub fn random_bytes(path: &Path, n: usize, seed: u64) -> io::Result<()> {
    let mut state = seed;
    let mut bytes = Vec::with_capacity(n);
    while bytes.len() < n {
        // splitmix64 step.
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        let chunk = z.to_le_bytes();
        for b in chunk {
            if bytes.len() < n {
                bytes.push(b);
            }
        }
    }
    fs::write(path, bytes)
}

/// Names that are valid on most filesystems but exercise edge cases:
/// long names, Unicode, emoji, leading dot.
///
/// Returns up to `count` names; the available pool is finite (~10).
///
/// # Example
///
/// ```
/// use dev_fixtures::adversarial::unusual_names;
/// let names = unusual_names(5);
/// assert_eq!(names.len(), 5);
/// ```
pub fn unusual_names(count: usize) -> Vec<String> {
    let pool = vec![
        ".hidden_file".to_string(),
        "with space.txt".to_string(),
        "with-many-dashes-in-name.txt".to_string(),
        "résumé.tex".to_string(),
        "файл.txt".to_string(),
        "ファイル.txt".to_string(),
        "emoji-✅-name.txt".to_string(),
        format!("{}{}", "long_name_", "x".repeat(120)),
        "trailing.dot.".to_string(),
        "no_extension".to_string(),
    ];
    pool.into_iter().take(count).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_zeros_writes_exact_size() {
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path = dir.path().join("big.bin");
        oversized_zeros(&path, 1024).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 1024);
        assert!(bytes.iter().all(|&b| b == 0));
    }

    #[test]
    fn oversized_sparse_reports_exact_size() {
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path = dir.path().join("sparse.bin");
        oversized_sparse(&path, 4096).unwrap();
        let meta = fs::metadata(&path).unwrap();
        assert_eq!(meta.len(), 4096);
    }

    #[test]
    fn malformed_utf8_is_not_valid_utf8() {
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path = dir.path().join("bad.txt");
        malformed_utf8(&path).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(std::str::from_utf8(&bytes).is_err());
    }

    #[test]
    fn random_bytes_are_deterministic_from_seed() {
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path = dir.path().join("rand.bin");
        random_bytes(&path, 64, 7).unwrap();
        let a = fs::read(&path).unwrap();
        random_bytes(&path, 64, 7).unwrap();
        let b = fs::read(&path).unwrap();
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn random_bytes_differ_with_seed() {
        let dir = mod_tempdir::TempDir::new().unwrap();
        let path_a = dir.path().join("a.bin");
        let path_b = dir.path().join("b.bin");
        random_bytes(&path_a, 64, 1).unwrap();
        random_bytes(&path_b, 64, 2).unwrap();
        let a = fs::read(&path_a).unwrap();
        let b = fs::read(&path_b).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn unusual_names_returns_requested_count() {
        let names = unusual_names(3);
        assert_eq!(names.len(), 3);
        let big = unusual_names(100);
        // Pool is finite; bounded by pool size.
        assert!(big.len() <= 10);
    }
}
