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

A fixture set-up failure SHOULD be reportable as a `CheckResult` with
verdict `Fail` and severity `Critical`. This integration is optional in
`0.1.x` and SHOULD become first-class in `0.3.x`.
