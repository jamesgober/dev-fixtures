# dev-fixtures — Project Specification (REPS)

> Rust Engineering Project Specification.
> Normative language follows RFC 2119.

## 1. Purpose

`dev-fixtures` MUST provide primitives for building deterministic,
auto-cleaning test environments. AI agents and CI harnesses MUST be
able to use these primitives to construct repeatable inputs without
worrying about cleanup or state leakage.

## 2. Scope

This crate MUST provide:

- A `TempProject` primitive backed by an OS temp directory.
- A `Fixture` trait with `set_up` / `tear_down` lifecycle.
- File-tree construction helpers.

This crate SHOULD provide (in future versions):

- Adversarial input generators (malformed, oversized, permission-denied).
- Mock data generators for common formats (CSV, JSON, bytes).
- Golden-file comparison utilities.
- Reset / reseed hooks for stateful fixtures.

This crate MUST NOT:

- Run tests
- Run benchmarks
- Inject failures (that is `dev-chaos`'s job)
- Require a global runtime
- Depend on tokio or any async runtime

## 3. Determinism

A fixture built twice with the same inputs MUST produce equivalent
on-disk state. "Equivalent" means: same files, same byte contents,
same permissions where the platform supports them. Timestamps MAY
differ.

## 4. Cleanup guarantees

`TempProject` MUST delete its directory when dropped, regardless of
whether the test passes or panics. Any panic in user code between
construction and drop MUST NOT prevent cleanup.

## 5. Cross-platform

This crate MUST work on Linux, macOS, and Windows. Path separators
MUST be handled portably. File modes that are not meaningful on Windows
MUST be silently ignored on Windows.

## 6. Integration with dev-report

A fixture set-up failure MUST be reportable as a `CheckResult` tagged
`fixtures` with verdict `Fail` and severity `Critical`. The
`Fixture::set_up_checked(name)` default method satisfies this.

### 6.1 Required tags and evidence

Every `CheckResult` from `set_up_checked`, `FixtureProducer`,
`Golden::compare`, etc. MUST carry the `fixtures` tag. Failure
checks MUST also carry the `regression` tag plus a per-cause tag:

- `setup_failed` — `Fixture::set_up` returned `Err`.
- `golden` (always, `Pass` or `Fail`) — golden-snapshot compare.
- `created` (Skip) — golden snapshot did not exist; created on first run.
- `updated` (Skip) — golden snapshot mismatched but `DEV_FIXTURES_UPDATE_GOLDEN` was set.
- `io_error` — read/write of the snapshot or stage failed.

Numeric `Evidence` MUST include `setup_ok` (1.0/0.0) for fixture
lifecycle checks and `actual_bytes` / `expected_bytes` for golden
checks.

## 7. File-tree builders

`dev_fixtures::tree::FileTree` materializes a tree under a chosen
root with `file`, `bytes`, `dir`, `symlink` builders. `symlink` is
a no-op on Windows.

Convenience helpers:

- `rust_crate(root, name, version)` — minimal crate layout.
- `rust_workspace(root, members)` — multi-crate workspace.

## 8. Adversarial inputs

`dev_fixtures::adversarial` provides deterministic generators for
inputs that exercise failure paths:

- `oversized_zeros(path, size)` — full byte buffer of zeros.
- `oversized_sparse(path, size)` — sparse file via `set_len`.
- `malformed_utf8(path)` — bytes that no UTF-8 decoder accepts.
- `random_bytes(path, n, seed)` — deterministic random byte stream.
- `unusual_names(count)` — Unicode, emoji, long, and dotted names.

All generators are deterministic given a seed (where applicable).

## 9. Golden snapshots

`dev_fixtures::golden::Golden::compare(name, actual)` returns a
`CheckResult`:

- Match -> `Pass`.
- Missing snapshot -> `Skip` with `created` tag (snapshot is written).
- Mismatch + `DEV_FIXTURES_UPDATE_GOLDEN` set -> `Skip` with `updated` tag.
- Mismatch (default) -> `Fail (Error)` with line-based diff in detail
  and full expected/actual/diff as `Evidence::snippet`.

## 10. Mock data

`dev_fixtures::mock` provides deterministic generators:

- `Rng::seeded(seed)` — splitmix64 RNG.
- `csv::generate(headers, rows, seed, row_factory)`.
- `json_array::generate(count, seed, element_factory)`.
- `bytes::{zeros, patterned, random}`.

All generators MUST produce byte-identical output across runs and
machines for the same inputs.

## 11. Producer integration

`FixtureProducer<F>` implements `dev_report::Producer`. Given a
closure `|| -> io::Result<()>` exercising a fixture lifecycle, the
producer emits a single-check `Report` with `producer = "dev-fixtures"`.
