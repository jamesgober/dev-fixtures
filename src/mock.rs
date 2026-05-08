//! Deterministic mock data generators.
//!
//! Generators are seeded by a `u64`. Same seed + same configuration
//! produces byte-identical output across runs and machines. No
//! external dependencies; deterministic via splitmix64.
//!
//! ## Generators
//!
//! - [`csv`] — CSV with a configurable schema and row count.
//! - [`json_array`] — JSON array of records with a shape template.
//! - [`bytes`] — raw byte streams: random, zeroed, patterned.

/// A deterministic pseudo-random number generator.
///
/// Internally splitmix64. Cheap to construct, no allocation.
///
/// # Example
///
/// ```
/// use dev_fixtures::mock::Rng;
/// let mut a = Rng::seeded(42);
/// let mut b = Rng::seeded(42);
/// assert_eq!(a.next_u64(), b.next_u64());
/// ```
pub struct Rng {
    state: u64,
}

impl Rng {
    /// Build a new RNG from a seed.
    pub fn seeded(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Step and return the next 64-bit value.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        z
    }

    /// Return a value uniformly in `[0, n)`. For small `n` this is
    /// biased; the bias is bounded by `1 / 2^32` for `n` up to
    /// `2^31`, which is fine for fixture purposes.
    pub fn range(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        self.next_u64() % n
    }
}

/// Module containing CSV generation.
pub mod csv {
    use super::Rng;

    /// Generate a CSV string with `rows` rows, one per `Vec<String>`
    /// produced by `row_factory`. The first line is the header.
    ///
    /// # Example
    ///
    /// ```
    /// use dev_fixtures::mock::{csv::generate, Rng};
    /// let csv = generate(
    ///     &["id", "name"],
    ///     3,
    ///     42,
    ///     |rng| vec![rng.range(1000).to_string(), format!("name_{}", rng.range(100))],
    /// );
    /// assert!(csv.starts_with("id,name\n"));
    /// assert_eq!(csv.lines().count(), 4); // 1 header + 3 rows
    /// ```
    pub fn generate<F>(headers: &[&str], rows: usize, seed: u64, mut row_factory: F) -> String
    where
        F: FnMut(&mut Rng) -> Vec<String>,
    {
        let mut rng = Rng::seeded(seed);
        let mut out = String::new();
        out.push_str(&headers.join(","));
        out.push('\n');
        for _ in 0..rows {
            let row = row_factory(&mut rng);
            out.push_str(&row.join(","));
            out.push('\n');
        }
        out
    }
}

/// Module containing JSON array generation.
pub mod json_array {
    use super::Rng;

    /// Generate a JSON array of `count` elements. Each element is
    /// produced by `element_factory(rng)` and embedded verbatim in the
    /// array (the factory MUST return valid JSON).
    ///
    /// # Example
    ///
    /// ```
    /// use dev_fixtures::mock::{json_array::generate, Rng};
    /// let json = generate(3, 7, |rng| {
    ///     format!("{{\"id\": {}}}", rng.range(1000))
    /// });
    /// assert!(json.starts_with("["));
    /// assert!(json.ends_with("]"));
    /// ```
    pub fn generate<F>(count: usize, seed: u64, mut element_factory: F) -> String
    where
        F: FnMut(&mut Rng) -> String,
    {
        let mut rng = Rng::seeded(seed);
        let mut out = String::new();
        out.push('[');
        for i in 0..count {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&element_factory(&mut rng));
        }
        out.push(']');
        out
    }
}

/// Module containing raw-byte generation.
pub mod bytes {
    use super::Rng;

    /// `n` bytes of zeros.
    pub fn zeros(n: usize) -> Vec<u8> {
        vec![0u8; n]
    }

    /// `n` bytes of a repeating pattern.
    ///
    /// # Example
    ///
    /// ```
    /// use dev_fixtures::mock::bytes::patterned;
    /// let bytes = patterned(7, &[0xAB, 0xCD]);
    /// assert_eq!(bytes, vec![0xAB, 0xCD, 0xAB, 0xCD, 0xAB, 0xCD, 0xAB]);
    /// ```
    pub fn patterned(n: usize, pattern: &[u8]) -> Vec<u8> {
        if pattern.is_empty() {
            return zeros(n);
        }
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            out.push(pattern[out.len() % pattern.len()]);
        }
        out
    }

    /// `n` deterministic random bytes from `seed`.
    pub fn random(n: usize, seed: u64) -> Vec<u8> {
        let mut rng = Rng::seeded(seed);
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            let v = rng.next_u64();
            for b in v.to_le_bytes() {
                if out.len() < n {
                    out.push(b);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rng_is_deterministic() {
        let mut a = Rng::seeded(42);
        let mut b = Rng::seeded(42);
        for _ in 0..16 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn rng_differs_with_seed() {
        let mut a = Rng::seeded(1);
        let mut b = Rng::seeded(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn rng_range_bounds() {
        let mut r = Rng::seeded(7);
        for _ in 0..1000 {
            let v = r.range(10);
            assert!(v < 10);
        }
        assert_eq!(Rng::seeded(0).range(0), 0);
    }

    #[test]
    fn csv_generate_is_deterministic() {
        let g = |seed| {
            csv::generate(&["a", "b"], 5, seed, |rng| {
                vec![rng.range(100).to_string(), rng.range(100).to_string()]
            })
        };
        assert_eq!(g(42), g(42));
        assert_ne!(g(42), g(43));
    }

    #[test]
    fn csv_has_header_and_row_count() {
        let csv = csv::generate(&["x", "y"], 3, 0, |rng| {
            vec![rng.range(10).to_string(), rng.range(10).to_string()]
        });
        assert!(csv.starts_with("x,y\n"));
        assert_eq!(csv.lines().count(), 4);
    }

    #[test]
    fn json_array_round_trip_shape() {
        let json = json_array::generate(3, 0, |rng| format!("{{\"id\":{}}}", rng.range(100)));
        assert!(json.starts_with("["));
        assert!(json.ends_with("]"));
        // 3 elements -> 2 commas at top level.
        assert_eq!(json.matches(',').count(), 2);
    }

    #[test]
    fn bytes_zeros_and_patterned() {
        assert_eq!(bytes::zeros(4), vec![0, 0, 0, 0]);
        assert_eq!(bytes::patterned(5, &[1, 2]), vec![1, 2, 1, 2, 1]);
        assert_eq!(bytes::patterned(3, &[]), vec![0, 0, 0]);
    }

    #[test]
    fn bytes_random_is_deterministic() {
        assert_eq!(bytes::random(64, 7), bytes::random(64, 7));
        assert_ne!(bytes::random(64, 7), bytes::random(64, 8));
    }
}
