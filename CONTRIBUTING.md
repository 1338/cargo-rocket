# Contributing

Cargo Rocket is a small Rust service. Keep changes focused on the upload flow, storage configuration, or the operation of that service.

## Local setup

Use Linux with Rust 1.89 or newer, a C compiler, and Node.js 22+ for JavaScript checks/examples. OpenSSH is needed only for SFTP storage. SQLite is bundled by the Rust dependency.

```sh
export ADMIN_TOKEN="$(openssl rand -hex 32)"
cargo run --locked
```

The server listens at http://localhost:3000. `cargo run` does not load `.env` automatically. HTML and JavaScript are embedded at compile time; restart the process after changing dashboard assets.

If Cargo reports `unknown proxy name` inside an AppImage terminal using zsh, run `unset ARGV0` in that terminal before invoking Cargo.

## Checks

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
node --check static/dashboard.js
node --check examples/upload.mjs
bash -n scripts/smoke-test.sh
```

Tests use temporary SQLite databases and real temporary local folders, with no cloud credentials. Add behavior-level integration coverage for changes to authorization, upload claims, size enforcement, or persistence. Remote adapters require a manual connection test against an environment you control.

For a running local instance with a configured project, `PROJECT_KEY=... scripts/smoke-test.sh` verifies upload, metadata, download, and replay rejection over HTTP. It leaves the test upload in storage because file deletion is not implemented.

## Layout

| Path | Purpose |
|---|---|
| `src/main.rs` | Process configuration, listener, and shutdown |
| `src/lib.rs` | Router, persistence, authentication, backend adapters, and transfer lifecycle |
| `static/` | Embedded dashboard without a build step |
| `tests/api.rs` | API integration tests |
| `examples/upload.mjs` | Dependency-free Node.js client example |
| `scripts/smoke-test.sh` | HTTP integration check for a running instance |
| `docs/` | API, storage, deployment, and architecture guides |

## Pull requests

Explain the behavior being changed, why it is needed, and how you verified it. Keep public API changes documented and preserve project isolation. Do not commit database files, uploaded content, credentials, private keys, or environment secrets. Avoid unrelated formatting and dependency changes.

Contributions are licensed under the repository's MIT license.
