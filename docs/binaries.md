# Run a Cargo Rocket binary

Release archives include the executable, this guide, the MIT license, and an environment-variable template. Rust, a C compiler, Node.js, and a separate SQLite installation are not required. The dashboard and SQLite library are embedded.

## Choose an archive

Download an archive and `SHA256SUMS` from [GitHub Releases](https://github.com/1338/cargo-rocket/releases). Assets are created when a version tag is released; before the first release, use the source or Docker quick start in the [project README](https://github.com/1338/cargo-rocket#quick-start).

| Platform | Target in the archive filename | Runtime |
|---|---|---|
| Linux x64 | `x86_64-unknown-linux-gnu` | glibc 2.35+ (Ubuntu 22.04 or newer, for example) |
| Linux ARM64 | `aarch64-unknown-linux-gnu` | glibc 2.35+ |
| macOS Intel | `x86_64-apple-darwin` | macOS 14+ |
| macOS Apple Silicon | `aarch64-apple-darwin` | macOS 14+ |

Windows is not currently supported because the SSH/SFTP adapter uses a Unix-only dependency. The Linux binaries are not musl builds for Alpine. Install your OS's CA certificate bundle for remote TLS connections. SSH storage also needs the `ssh` executable, service-user keys, and trusted `known_hosts` entries. macOS binaries are unsigned and not notarized; macOS may require approval in Privacy & Security before first use.

While the repository is private, download through GitHub while signed in, or use an authenticated GitHub CLI:

```sh
tag=v0.1.0 # Replace with the release you want.
target=x86_64-unknown-linux-gnu # Choose from the table above.
archive="cargo-rocket-$tag-$target.tar.gz"
gh release download "$tag" --repo 1338/cargo-rocket \
  --pattern "$archive" --pattern SHA256SUMS
```

After the repository is public, downloads are also available without authentication at `https://github.com/1338/cargo-rocket/releases/download/<tag>/<archive>`.

## Verify and unpack

From the directory containing the archive and `SHA256SUMS`, select the checksum for your download. Set `archive` to your downloaded filename if you did not use the command above.

```sh
awk -v file="$archive" '$2 == file' SHA256SUMS > selected.sha256

# Linux:
sha256sum --check selected.sha256

# macOS: use this instead of sha256sum.
shasum -a 256 --check selected.sha256
```

Continue only if the checksum reports `OK`. SHA-256 checksums verify file integrity; they are not code-signing signatures.

```sh
tar -xzf "$archive"
cd "${archive%.tar.gz}"
export HOST_URL="http://localhost:3000/"
export ADMIN_TOKEN="$(openssl rand -hex 32)"
./cargo-rocket
```

Open the URL in `HOST_URL`. Add a storage connection, test it, create a project, and save the generated project API key. For a local storage connection, use an absolute directory writable by the user running the binary.

The default database is `data/upload-tool.db` relative to the working directory, retained for compatibility with early installs. Set an absolute `DATABASE_PATH` for a service deployment. Downloaded `.env.example` is a template only; the binary does not automatically load `.env` files. Preserve your admin token across restarts rather than generating a new one every time you start a managed service.

`HOST_URL` sets the listener hostname/port and generated URLs. For HTTPS behind a reverse proxy, use an HTTPS `HOST_URL` and an explicit internal `BIND_ADDR`, such as `127.0.0.1:3000`. See [deployment documentation](https://github.com/1338/cargo-rocket/blob/main/docs/deployment.md) for networking, Docker, CORS, credentials, backups, and operational limits.

Run one instance per SQLite database. Keep that database and your storage folders outside the extracted version directory when upgrading, then stop the old process before starting the new binary. Keep the previous binary and a database backup available when planning an upgrade.
