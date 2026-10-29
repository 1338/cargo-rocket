# Building and publishing releases

[← README](../README.md)

The [Release binaries workflow](../.github/workflows/release.yml) builds native Linux x64/ARM64 and macOS Intel/Apple Silicon executables. All storage adapters remain enabled. Each target runs the Rust tests and smoke-tests the optimized binary's HTTP server and embedded dashboard before packaging it.

## Publish a version

1. Update the package version in `Cargo.toml` and refresh `Cargo.lock` with `cargo check`. Commit and push the version change with all intended release changes.
2. Confirm CI passes on that commit.
3. Create and push a version tag matching `Cargo.toml` exactly, prefixed with `v`:

```sh
# For Cargo.toml version = "0.1.0":
git tag -a v0.1.0 -m "Cargo Rocket v0.1.0"
git push origin v0.1.0
```

The workflow checks out the tagged commit, verifies its package version, builds all four targets, and uploads one `.tar.gz` per platform plus `SHA256SUMS` to a GitHub Release with generated release notes. Each archive contains `cargo-rocket`, installation instructions, `LICENSE`, and `.env.example`. The publication job requires all target builds to pass and verifies that exactly the four expected archives are present.

Tags such as `v0.2.0-rc.1` create prereleases and must match a package version such as `0.2.0-rc.1`. Release creation uses the workflow's automatic `GITHUB_TOKEN`; no personal access token or repository secret is needed. Only the publication job has `contents: write`; build jobs have read access. Repository visibility is unchanged, so releases in a private repository require authenticated access.

The workflow does not run for an ordinary push to `main`. Adding the workflow does not create a release; the first release requires a version tag containing the release workflow and scripts.

## Manual builds and failed releases

The workflow can also be run from the Actions tab with an **existing version tag**. Leave **publish** unchecked to build downloadable Actions artifacts without creating a release. Check it to publish after the builds pass. This is also useful when retrying a failed tag-triggered run.

A release stays in draft until all archive and checksum uploads finish. A retry can replace assets on that draft and finish publication. An already published release is never overwritten by this workflow; use a new version tag for changed binaries. Builds from existing tags use the release scripts and code at that tag, not newer source from `main`.

## Local verification

Requires Python 3.11+, Rust 1.89+, and native build tools for the chosen target. From a Linux x64 checkout at version 0.1.0:

```sh
cargo test --locked
cargo build --release --locked --target x86_64-unknown-linux-gnu
python3 scripts/release.py validate v0.1.0
python3 scripts/release.py smoke target/x86_64-unknown-linux-gnu/release/cargo-rocket
python3 scripts/release.py package v0.1.0 x86_64-unknown-linux-gnu \
  target/x86_64-unknown-linux-gnu/release/cargo-rocket
python3 -m unittest discover -s scripts -p 'test_*.py'
```

Output goes into ignored `dist/`. `scripts/release.py checksums <tag>` accepts only a complete set of all four archives, preventing partial releases. The release jobs build natively on Ubuntu 22.04, macOS 15 Intel, and macOS 14 ARM; the binaries target glibc 2.35+ on Linux and macOS 14+. macOS binaries are currently unsigned and not notarized.
