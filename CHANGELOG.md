# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-11-07

### Added

- `tcamp login`, `logout`, `status`, and `doctor` commands for managing one
  tokencamp Connection and its Credential.
- Claude Code support through a tcamp-owned Overlay and `claude --settings`,
  with Launch-time API key injection.
- Pi support through `providers.tokencamp` in `models.json` and five exposed
  Catalog models.
- Kimi Code support through `[providers.tokencamp]` in `config.toml` and
  `tokencamp/*` model aliases.
- Ownership manifest, atomic writes, backups, Drift detection, Conflict
  handling with `--force`, and safe `tcamp uninstall`.
- Release archives for aarch64/x86_64 macOS and x86_64/aarch64 Linux musl,
  with `SHA256SUMS`, build provenance, and `install.sh`.
- Release gates for archive size and offline startup latency.

[Unreleased]: https://github.com/0xnicholas/tokencamp-tools/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/0xnicholas/tokencamp-tools/releases/tag/v0.1.0
