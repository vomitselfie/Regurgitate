# Contributor and release guide

## Local verification

```bash
cargo fmt --all -- --check
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

The same checks run in GitHub Actions for pull requests and pushes to `master`.

## Release behavior

A passing push to `master` publishes the version declared in `Cargo.toml` when
that version does not already have a GitHub release. The workflow builds with
the declared minimum Rust version and creates these assets:

- Linux x86-64 archive
- Apple Silicon macOS archive
- Intel macOS archive
- standalone `install.sh` bootstrap
- combined `SHA256SUMS`

The checksum set covers the three archives and the bootstrap. CI runs the
bootstrap against a local release fixture to verify pinned Codex setup,
idempotent reruns, and rejection of a corrupted archive.

The workflow tags the tested commit and generates release notes. Existing
release tags and complete asset sets are never replaced by a later commit.

For each new release, update the package version in `Cargo.toml`, refresh
`Cargo.lock`, run the verification commands above, and push the tested commit
to `master`.
