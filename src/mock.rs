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
    /// Field values containing `,`, `"`, `\n`, or `\r` are escaped per
    /// [RFC 4180](https://datatracker.ietf.org/doc/html/rfc4180):
    /// the value is wrapped in double quotes and any internal `"` is
    /// doubled. Values without those characters pass through verbatim.
    ///
    /// Header values are escaped the same way.
    ///
    /// # Example
    ///
    /// ```
    /// use dev_fixtures::mock::csv::generate;
    /// let csv = generate(
    ///     &["id", "name"],
    ///     3,
    ///     42,
    ///     |rng| vec![rng.range(1000).to_string(), format!("name_{}", rng.range(100))],
    /// );
    /// assert!(csv.starts_with("id,name\n"));
    /// assert_eq!(csv.lines().count(), 4); // 1 header + 3 rows
    /// ```
    ///
    /// # Escaping example
    ///
    /// ```
    /// use dev_fixtures::mock::csv::generate;
    /// let csv = generate(&["a", "b"], 1, 0, |_rng| {
    ///     vec![r#"contains, comma"#.into(), r#"has "quotes""#.into()]
    /// });
    /// // Both fields are wrapped; the quote inside is doubled.
    /// assert!(csv.contains("\"contains, comma\""));
    /// assert!(csv.contains("\"has \"\"quotes\"\"\""));
    /// ```
    pub fn generate<F>(headers: &[&str], rows: usize, seed: u64, mut row_factory: F) -> String
    where
        F: FnMut(&mut Rng) -> Vec<String>,
    {
        let mut rng = Rng::seeded(seed);
        let mut out = String::new();
        let header_row: Vec<String> = headers.iter().map(|h| escape_field(h)).collect();
        out.push_str(&header_row.join(","));
        out.push('\n');
        for _ in 0..rows {
            let row = row_factory(&mut rng);
            let escaped: Vec<String> = row.iter().map(|f| escape_field(f)).collect();
            out.push_str(&escaped.join(","));
            out.push('\n');
        }
        out
    }

    /// Escape a single CSV field per RFC 4180.
    ///
    /// Returns the field unchanged when no special characters are
    /// present; otherwise returns the field wrapped in double quotes
    /// with any internal `"` doubled.
    pub fn escape_field(value: &str) -> String {
        if value.contains(',')
            || value.contains('"')
            || value.contains('\n')
            || value.contains('\r')
        {
            let escaped = value.replace('"', "\"\"");
            format!("\"{}\"", escaped)
        } else {
            value.to_string()
        }
    }

    /// Parse a CSV string back into a header row + data rows.
    ///
    /// Implements the same RFC 4180 escaping rules used by
    /// [`generate`]: quoted fields support embedded `,`, `"` (doubled),
    /// `\n`, and `\r`. Designed for round-trip with `generate`, not as
    /// a general-purpose CSV parser.
    ///
    /// Returns `(headers, rows)`. Each row has the same length as
    /// `headers` for well-formed input. Trailing empty lines are
    /// ignored. Returns `Err` with a one-line message on malformed
    /// input (e.g. unterminated quoted field).
    ///
    /// # Example
    ///
    /// ```
    /// use dev_fixtures::mock::csv::{generate, parse};
    ///
    /// let csv = generate(&["id", "name"], 3, 0, |rng| {
    ///     vec![rng.range(100).to_string(), format!("u{}", rng.range(10))]
    /// });
    /// let (headers, rows) = parse(&csv).unwrap();
    /// assert_eq!(headers, vec!["id", "name"]);
    /// assert_eq!(rows.len(), 3);
    /// for row in &rows {
    ///     assert_eq!(row.len(), 2);
    /// }
    /// ```
    pub fn parse(input: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
        let mut all_rows: Vec<Vec<String>> = Vec::new();
        let mut chars = input.chars().peekable();
        let mut current_field = String::new();
        let mut current_row: Vec<String> = Vec::new();
        let mut in_quotes = false;
        let mut row_has_content = false;
        loop {
            match chars.next() {
                None => {
                    // EOF
                    if in_quotes {
                        return Err("unterminated quoted field at EOF".to_string());
                    }
                    if !current_field.is_empty() || row_has_content {
                        current_row.push(std::mem::take(&mut current_field));
                        all_rows.push(std::mem::take(&mut current_row));
                    }
                    break;
                }
                Some(c) => {
                    if in_quotes {
                        match c {
                            '"' => {
                                if matches!(chars.peek(), Some('"')) {
                                    chars.next();
                                    current_field.push('"');
                                } else {
                                    in_quotes = false;
                                }
                            }
                            other => current_field.push(other),
                        }
                    } else {
                        match c {
                            '"' if current_field.is_empty() => {
                                in_quotes = true;
                                row_has_content = true;
                            }
                            ',' => {
                                current_row.push(std::mem::take(&mut current_field));
                                row_has_content = true;
                            }
                            '\r' => {
                                // Eat following \n if present.
                                if matches!(chars.peek(), Some('\n')) {
                                    chars.next();
                                }
                                current_row.push(std::mem::take(&mut current_field));
                                all_rows.push(std::mem::take(&mut current_row));
                                row_has_content = false;
                            }
                            '\n' => {
                                current_row.push(std::mem::take(&mut current_field));
                                all_rows.push(std::mem::take(&mut current_row));
                                row_has_content = false;
                            }
                            other => {
                                current_field.push(other);
                                row_has_content = true;
                            }
                        }
                    }
                }
            }
        }

        if all_rows.is_empty() {
            return Ok((Vec::new(), Vec::new()));
        }
        let headers = all_rows.remove(0);
        Ok((headers, all_rows))
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

    /// Like [`generate`] but validates the output as JSON before
    /// returning it.
    ///
    /// Performs a structural validation pass: every byte of the
    /// output must form a syntactically valid JSON value. Returns
    /// `Err(message)` if validation fails — typically because the
    /// `element_factory` returned malformed JSON.
    ///
    /// Useful as a defensive guard when the factory produces JSON
    /// from external/untrusted templates. Adds linear-in-output-size
    /// overhead.
    ///
    /// # Example
    ///
    /// ```
    /// use dev_fixtures::mock::json_array::generate_validated;
    ///
    /// let json = generate_validated(3, 0, |rng| format!("{{\"v\":{}}}", rng.range(10))).unwrap();
    /// assert!(json.starts_with("["));
    ///
    /// // A broken factory triggers a validation error.
    /// let err = generate_validated(1, 0, |_| "not_json".to_string()).unwrap_err();
    /// assert!(err.contains("invalid"));
    /// ```
    pub fn generate_validated<F>(
        count: usize,
        seed: u64,
        element_factory: F,
    ) -> Result<String, String>
    where
        F: FnMut(&mut Rng) -> String,
    {
        let out = generate(count, seed, element_factory);
        validate_json(&out)?;
        Ok(out)
    }

    /// Minimal JSON structural validator.
    ///
    /// Returns `Ok(())` if `s` is a syntactically valid JSON value
    /// (object, array, string, number, true/false/null). Does NOT
    /// validate semantics (e.g. duplicate keys are accepted).
    ///
    /// Hand-rolled, no `serde_json` dependency at the public surface.
    ///
    /// # Example
    ///
    /// ```
    /// use dev_fixtures::mock::json_array::validate_json;
    ///
    /// assert!(validate_json("[]").is_ok());
    /// assert!(validate_json("[{\"x\": 1}]").is_ok());
    /// assert!(validate_json("[invalid]").is_err());
    /// ```
    pub fn validate_json(s: &str) -> Result<(), String> {
        let mut parser = MiniJsonParser::new(s);
        parser.skip_ws();
        parser.parse_value()?;
        parser.skip_ws();
        if parser.pos < parser.bytes.len() {
            return Err(format!(
                "trailing characters after JSON value at position {}",
                parser.pos
            ));
        }
        Ok(())
    }

    struct MiniJsonParser<'a> {
        bytes: &'a [u8],
        pos: usize,
    }

    impl<'a> MiniJsonParser<'a> {
        fn new(s: &'a str) -> Self {
            Self {
                bytes: s.as_bytes(),
                pos: 0,
            }
        }

        fn skip_ws(&mut self) {
            while self.pos < self.bytes.len()
                && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\n' | b'\r')
            {
                self.pos += 1;
            }
        }

        fn parse_value(&mut self) -> Result<(), String> {
            self.skip_ws();
            if self.pos >= self.bytes.len() {
                return Err("unexpected end of input".to_string());
            }
            match self.bytes[self.pos] {
                b'{' => self.parse_object(),
                b'[' => self.parse_array(),
                b'"' => self.parse_string(),
                b't' | b'f' => self.parse_bool(),
                b'n' => self.parse_null(),
                b'-' | b'0'..=b'9' => self.parse_number(),
                other => Err(format!(
                    "invalid JSON: unexpected '{}' at position {}",
                    other as char, self.pos
                )),
            }
        }

        fn parse_object(&mut self) -> Result<(), String> {
            self.pos += 1; // consume '{'
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                return Ok(());
            }
            loop {
                self.skip_ws();
                self.parse_string()?;
                self.skip_ws();
                if self.peek() != Some(b':') {
                    return Err(format!("expected ':' at position {}", self.pos));
                }
                self.pos += 1;
                self.parse_value()?;
                self.skip_ws();
                match self.peek() {
                    Some(b',') => {
                        self.pos += 1;
                    }
                    Some(b'}') => {
                        self.pos += 1;
                        return Ok(());
                    }
                    _ => {
                        return Err(format!(
                            "expected ',' or '}}' in object at position {}",
                            self.pos
                        ));
                    }
                }
            }
        }

        fn parse_array(&mut self) -> Result<(), String> {
            self.pos += 1; // consume '['
            self.skip_ws();
            if self.peek() == Some(b']') {
                self.pos += 1;
                return Ok(());
            }
            loop {
                self.parse_value()?;
                self.skip_ws();
                match self.peek() {
                    Some(b',') => {
                        self.pos += 1;
                    }
                    Some(b']') => {
                        self.pos += 1;
                        return Ok(());
                    }
                    _ => {
                        return Err(format!(
                            "expected ',' or ']' in array at position {}",
                            self.pos
                        ));
                    }
                }
            }
        }

        fn parse_string(&mut self) -> Result<(), String> {
            if self.peek() != Some(b'"') {
                return Err(format!("expected string at position {}", self.pos));
            }
            self.pos += 1;
            while self.pos < self.bytes.len() {
                match self.bytes[self.pos] {
                    b'"' => {
                        self.pos += 1;
                        return Ok(());
                    }
                    b'\\' => {
                        self.pos += 1;
                        if self.pos >= self.bytes.len() {
                            return Err("unterminated escape in string".to_string());
                        }
                        self.pos += 1;
                    }
                    _ => self.pos += 1,
                }
            }
            Err("unterminated string".to_string())
        }

        fn parse_bool(&mut self) -> Result<(), String> {
            if self.bytes[self.pos..].starts_with(b"true") {
                self.pos += 4;
                Ok(())
            } else if self.bytes[self.pos..].starts_with(b"false") {
                self.pos += 5;
                Ok(())
            } else {
                Err(format!("invalid bool at position {}", self.pos))
            }
        }

        fn parse_null(&mut self) -> Result<(), String> {
            if self.bytes[self.pos..].starts_with(b"null") {
                self.pos += 4;
                Ok(())
            } else {
                Err(format!("invalid null at position {}", self.pos))
            }
        }

        fn parse_number(&mut self) -> Result<(), String> {
            let start = self.pos;
            if self.peek() == Some(b'-') {
                self.pos += 1;
            }
            while self.pos < self.bytes.len() {
                let c = self.bytes[self.pos];
                if c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-') {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos == start || (self.peek_at(start) == Some(b'-') && self.pos == start + 1) {
                return Err(format!("invalid number at position {}", start));
            }
            Ok(())
        }

        fn peek(&self) -> Option<u8> {
            self.bytes.get(self.pos).copied()
        }

        fn peek_at(&self, idx: usize) -> Option<u8> {
            self.bytes.get(idx).copied()
        }
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
    fn csv_escapes_commas_quotes_and_newlines() {
        let csv = csv::generate(&["a", "b"], 1, 0, |_rng| {
            vec![
                "value, with comma".into(),
                "value with \"quote\" and\nnewline".into(),
            ]
        });
        assert!(csv.contains("\"value, with comma\""));
        assert!(csv.contains("\"value with \"\"quote\"\" and\nnewline\""));
    }

    #[test]
    fn csv_escapes_in_headers_too() {
        let csv = csv::generate(&["plain", "with, comma"], 0, 0, |_rng| vec![]);
        assert_eq!(csv.trim(), "plain,\"with, comma\"");
    }

    #[test]
    fn csv_unescaped_when_no_special_chars() {
        let csv = csv::generate(&["a", "b"], 1, 0, |_rng| {
            vec!["plain".into(), "also plain".into()]
        });
        assert!(csv.contains("plain,also plain"));
        // No quotes added.
        assert!(!csv.contains("\""));
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
    fn json_array_validates_well_formed() {
        let json =
            json_array::generate_validated(3, 0, |rng| format!("{{\"v\":{}}}", rng.range(10)))
                .unwrap();
        assert!(json.starts_with("["));
    }

    #[test]
    fn json_array_validation_rejects_garbage_factory_output() {
        let err = json_array::generate_validated(2, 0, |_| "not_json".to_string()).unwrap_err();
        assert!(err.contains("invalid"));
    }

    #[test]
    fn json_validate_accepts_canonical_examples() {
        for s in &[
            "{}",
            "[]",
            "[1,2,3]",
            "{\"a\":1}",
            "[{\"k\":[true,false,null]}]",
            "[\"with \\\"quote\\\"\"]",
            "{\"n\": -3.14e2}",
        ] {
            assert!(json_array::validate_json(s).is_ok(), "should accept: {}", s);
        }
    }

    #[test]
    fn json_validate_rejects_malformed() {
        for s in &["{", "[", "[,]", "{1:1}", "[true,]"] {
            assert!(
                json_array::validate_json(s).is_err(),
                "should reject: {}",
                s
            );
        }
    }

    #[test]
    fn csv_round_trip_with_special_chars() {
        let csv = csv::generate(&["id", "note"], 2, 0, |rng| {
            vec![
                rng.range(100).to_string(),
                "value, with comma\nand newline".into(),
            ]
        });
        let (headers, rows) = csv::parse(&csv).unwrap();
        assert_eq!(headers, vec!["id", "note"]);
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert_eq!(row.len(), 2);
            assert!(row[1].contains("value, with comma"));
            assert!(row[1].contains('\n'));
        }
    }

    #[test]
    fn csv_parse_quoted_doubled_quote() {
        let csv = "a,b\nplain,\"has \"\"quote\"\" inside\"\n";
        let (h, r) = csv::parse(csv).unwrap();
        assert_eq!(h, vec!["a", "b"]);
        assert_eq!(r[0][1], "has \"quote\" inside");
    }

    #[test]
    fn csv_parse_rejects_unterminated_quote() {
        let csv = "a,b\n\"never closes,foo\n";
        assert!(csv::parse(csv).is_err());
    }

    #[test]
    fn csv_parse_handles_crlf() {
        let csv = "a,b\r\n1,2\r\n3,4\r\n";
        let (h, r) = csv::parse(csv).unwrap();
        assert_eq!(h, vec!["a", "b"]);
        assert_eq!(
            r,
            vec![
                vec!["1".to_string(), "2".to_string()],
                vec!["3".to_string(), "4".to_string()]
            ]
        );
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
