# Contributor and release guide

## Local verification

`rust-toolchain.toml` pins local Cargo commands to Rust 1.88.0, the same compiler,
formatter, and Clippy used by CI. Rustup installs the pinned toolchain when
needed. Testing only with a newer default toolchain does not verify this release
gate: Clippy behavior can differ between versions.

```bash
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

The same checks run in GitHub Actions for pull requests and pushes to `master`.

On Linux with AoE installed, also run `bash scripts/test-aoe-manifest.sh`.
This loads the manifest through AoE's actual registry in a temporary XDG
configuration, without granting capabilities or starting a worker. It also
checks that the retired `settings-page` slot is rejected. CI runs this check
with checksum-pinned AoE 1.15.3, separately from the Rust unit tests.

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

For each new release, update the version in `Cargo.toml` and `aoe-plugin.toml`, refresh
`Cargo.lock`, run the verification commands above, and push the tested commit
to `master`.

When changing the supported Rust version, keep `rust-toolchain.toml`, the
workflow's `RUST_VERSION`, and `Cargo.toml`'s `rust-version` aligned. A failed
verification job skips both platform builds and release publication; inspect
the Actions run before expecting a new entry on the Releases page.
