# Development

This doc covers local development workflow, quality checks, and release flow.

## Prerequisites

- Rust stable toolchain (`rust-toolchain.toml`)
- `cargo`, `rustfmt`, `clippy`

## Local loop

Format and test before pushing:

```bash
cargo fmt
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Run otell locally:

```bash
otell run
```

Probe quickly:

```bash
otell intro
```

## Repo docs

- `docs/CLI.md` command reference
- `docs/API.md` transport/API reference
- `docs/ARCHITECTURE.md` internals
- `docs/CONFIG.md` env + runtime config

## CI

Workflow: `.github/workflows/ci.yml`

On push + PR it runs:

- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

## Release

Release automation uses `release-plz`.

Config: `release-plz.toml`

Workflows:

- `.github/workflows/release-pr.yml`
  - runs `release-plz release-pr`
  - opens/updates a release PR with version/changelog updates
- `.github/workflows/release.yml`
  - runs `release-plz release`
  - publishes crates and creates tags/releases
- `.github/workflows/release-binaries.yml`
  - triggered on GitHub Release `published`
  - builds and uploads prebuilt binaries for Linux/macOS/Windows

### Publishing scope

- Public crates: `otell-core`, `otell-store`, `otell-ingest`, `otell`
- Non-published crate: `testkit` (`publish = false`)

### Required secrets

- `CRATES_TOKEN` for crates.io publish

### Typical release steps

1. Merge regular changes into `main`.
2. Wait for release PR from `release-plz`.
3. Review and merge release PR.
4. `release.yml` publishes crates + creates GitHub release/tag.
5. `release-binaries.yml` attaches platform binaries to the release.
