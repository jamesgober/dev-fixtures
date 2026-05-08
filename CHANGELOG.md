# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-05-07

### Added

- Initial crate skeleton.
- `TempProject` builder for disposable project directories backed by `tempfile`.
- `Fixture` trait for arbitrary set-up / tear-down lifecycles.
- Smoke tests covering build + auto-cleanup.

### Note

This is a name-claim release. The public API will expand in `0.2.x` as
adversarial input generators and golden-file helpers land.

[Unreleased]: https://github.com/jamesgober/dev-fixtures/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jamesgober/dev-fixtures/releases/tag/v0.1.0
