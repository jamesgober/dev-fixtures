# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `examples/temp_project.rs` — runnable demonstration of the `TempProject` lifecycle: stage text + binary files, build the directory, inspect declared paths, then verify cleanup on drop.

### Changed

- CI: `actions/checkout` bumped from `v4` to `v5` (removes Node 20 deprecation warnings).

## [0.9.2] - 2026-05-10

### Added

- `mock::csv::parse(input)` — round-trip CSV parser. Reads RFC-4180-encoded CSV produced by `generate` and yields `(headers, rows)`. Supports quoted fields with embedded commas, doubled-quote escapes, and `CRLF`/`LF` line endings.
- `mock::json_array::generate_validated(...)` — same as `generate` but returns `Err(message)` if the produced output isn't structurally valid JSON. Useful as a defensive guard when the factory builds JSON from external templates.
- `mock::json_array::validate_json(s)` — minimal hand-rolled JSON structural validator (no `serde_json` dependency at the public surface).

### Changed

- `Golden::compare` now uses an LCS-based diff (longest common subsequence) instead of position-aligned line-by-line comparison. Insertions and deletions in the middle of a snapshot now produce a single `+` or `-` line, not a cascade of edit pairs. Hand-rolled implementation; no new dependencies.

[0.9.2]: https://github.com/jamesgober/dev-fixtures/releases/tag/v0.9.2

## [0.9.1] - 2026-05-09

### Added

- `BinaryGolden` snapshot type for byte-content comparison. Mirrors `Golden` but for `Vec<u8>`. On mismatch, evidence includes byte counts, the first differing offset, and a hex preview of expected/actual at that offset. Tagged `fixtures` + `golden` + `binary`.
- `mock::csv::escape_field` public helper.

### Fixed

- `mock::csv::generate` now escapes field values per [RFC 4180](https://datatracker.ietf.org/doc/html/rfc4180): values containing `,`, `"`, `\n`, or `\r` are wrapped in double quotes, with internal `"` doubled. Previously, such values produced malformed CSV. Header values are escaped with the same rules.

[0.9.1]: https://github.com/jamesgober/dev-fixtures/releases/tag/v0.9.1

## [0.9.0] - 2026-05-08

### Added

#### Adoption of dev-report 0.9

- Added `dev-report = "0.9"` dependency.
- `Fixture::set_up_checked(name)` default method emits a `CheckResult` tagged `fixtures` (and `setup_failed` + `regression` on Err) with numeric `setup_ok` evidence.
- `FixtureProducer<F>` adapter implementing `dev_report::Producer` for fixture-lifecycle self-tests.

#### File-tree builders (v0.2 milestone)

- New `tree` module with `FileTree` builder: `file`, `bytes`, `dir`, `symlink` (Unix; no-op on Windows).
- `rust_crate(root, name, version)` helper for minimal crate layout.
- `rust_workspace(root, members)` helper for multi-crate workspaces.

#### Adversarial input generators (v0.3 milestone)

- New `adversarial` module:
  - `oversized_zeros(path, size)` — buffer of zeros.
  - `oversized_sparse(path, size)` — sparse via `set_len`.
  - `malformed_utf8(path)` — invalid UTF-8 bytes after valid prefix.
  - `random_bytes(path, n, seed)` — deterministic random stream (splitmix64).
  - `unusual_names(count)` — Unicode, emoji, long, dotted, etc.

#### Golden snapshots (v0.4 milestone)

- New `golden` module with `Golden::compare(name, actual)` emitting `CheckResult`:
  - First run -> `Skip` + `created` tag, snapshot written.
  - Match -> `Pass`.
  - Mismatch + `DEV_FIXTURES_UPDATE_GOLDEN` set -> `Skip` + `updated` tag.
  - Mismatch -> `Fail (Error)` with line-based diff in detail and `Evidence::snippet` for expected/actual/diff.

#### Mock data generators (v0.5 milestone)

- New `mock` module:
  - `Rng::seeded(seed)` — splitmix64 RNG.
  - `csv::generate(headers, rows, seed, row_factory)`.
  - `json_array::generate(count, seed, element_factory)`.
  - `bytes::{zeros, patterned, random}`.

### Documentation

- All public items have rustdoc with at least one example.
- REPS.md expanded: §6 (dev-report integration + required tags/evidence), §7 (file-tree builders), §8 (adversarial inputs), §9 (golden snapshots), §10 (mock data), §11 (producer integration).

[0.9.0]: https://github.com/jamesgober/dev-fixtures/releases/tag/v0.9.0

## [0.1.0] - 2026-05-07

### Added

- Initial crate skeleton.
- `TempProject` builder for disposable project directories backed by `tempfile`.
- `Fixture` trait for arbitrary set-up / tear-down lifecycles.
- Smoke tests covering build + auto-cleanup.

### Note

This is a name-claim release. The public API will expand in `0.2.x` as
adversarial input generators and golden-file helpers land.

[Unreleased]: https://github.com/jamesgober/dev-fixtures/compare/v0.9.2...HEAD
[0.1.0]: https://github.com/jamesgober/dev-fixtures/releases/tag/v0.1.0
