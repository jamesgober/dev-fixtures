<h1 align="center">
    <strong>dev-fixtures</strong>
    <br>
    <sup><sub>REPEATABLE TEST ENVIRONMENTS FOR RUST</sub></sup>
</h1>

<p align="center">
    <a href="https://crates.io/crates/dev-fixtures"><img alt="crates.io" src="https://img.shields.io/crates/v/dev-fixtures.svg"></a>
    <a href="https://docs.rs/dev-fixtures"><img alt="docs.rs" src="https://docs.rs/dev-fixtures/badge.svg"></a>
    <a href="https://github.com/jamesgober/dev-fixtures/blob/main/LICENSE"><img alt="License" src="https://img.shields.io/badge/license-Apache--2.0-blue.svg"></a>
</p>

<p align="center">
    Test environments, sample data, and controlled inputs.<br>
    Part of the <code>dev-*</code> verification suite.
</p>

---

## What it does

Builds disposable, deterministic test environments. The most common
primitive is `TempProject`, which lets you stage a project tree, run
your test against it, and have the directory cleaned up on drop.

## Quick start

```toml
[dependencies]
dev-fixtures = "0.1"
```

```rust
use dev_fixtures::TempProject;

let project = TempProject::new()
    .with_file("Cargo.toml", "[package]\nname = \"sample\"\n")
    .with_file("src/lib.rs", "pub fn answer() -> u32 { 42 }")
    .build()
    .unwrap();

// project.path() points at a temp directory.
// It is deleted automatically when `project` is dropped.
```

## What's planned

- File-tree builders with golden-file comparison helpers.
- Adversarial input generators (oversized, malformed, permission-denied).
- Mock data primitives (CSV, JSON, plain bytes).
- Reset / reseed hooks for stateful fixtures.
- Integration with `dev-report` for fixture-setup verdicts.

## The `dev-*` suite

`dev-fixtures` is one of the producer crates in the `dev-*` verification
suite. See [`dev-tools`](https://github.com/jamesgober/dev-tools) for the
umbrella crate.

## License

Apache-2.0. See [LICENSE](LICENSE).
