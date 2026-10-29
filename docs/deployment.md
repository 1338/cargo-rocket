# Configuration and deployment

[← README](../README.md)

| Variable | Default | Purpose |
|---|---|---|
| `ADMIN_TOKEN` | Required, 32+ characters | Dashboard and management API secret |
| `HOST_URL` | `http://localhost:3000` | HTTP listener hostname/port and generated upload/file URL base |
| `BIND_ADDR` | Hostname/port from `HOST_URL` | Optional listener override for proxies and containers |
| `PUBLIC_BASE_URL` | `HOST_URL` | Optional override for generated upload/file URLs; retained for existing deployments |
| `HOST_PORT` | `3000` | Docker Compose host-side published port |
| `DATABASE_PATH` | `data/upload-tool.db` | SQLite database |
| `CORS_ORIGINS` | No cross-origin access | Comma-separated allowed browser origins |
| `RUST_LOG` | Unset | Log filter, e.g. `cargo_rocket=info` |

## Listening address

```sh
export HOST_URL="http://localhost:8080/"
export ADMIN_TOKEN="$(openssl rand -hex 32)"
cargo run --locked
```

The dashboard, API, and generated upload links are now available at `http://localhost:8080`. Trailing slashes are normalized. Without an explicit port, HTTP uses port 80. IPv4, DNS names, and bracketed IPv6 addresses such as `http://[::1]:8080/` are accepted.

For direct hosting on a DNS name, the name must already resolve to an address assigned to this server. The process binds that hostname and port; `HOST_URL` does not provision DNS. For access through a proxy or a different network interface, set `BIND_ADDR` explicitly.

Origins must use HTTP(S) and cannot include credentials, a path prefix, a query, a fragment, or port zero. `BIND_ADDR` overrides only the listener; `PUBLIC_BASE_URL` overrides only generated file/upload links. Client examples use `HOST_URL` by default; `CARGO_BASE_URL` remains an optional client-only override.

## HTTPS behind a reverse proxy

```sh
export HOST_URL="https://cargo.example.com/"
export BIND_ADDR="127.0.0.1:3000"
export ADMIN_TOKEN="$(openssl rand -hex 32)"
cargo run --locked
```

Point the DNS name at your proxy, terminate TLS there, and forward requests to `http://127.0.0.1:3000`. Generated links use the external HTTPS origin. Cargo Rocket serves HTTP internally; specifying an HTTPS `HOST_URL` requires an explicit `BIND_ADDR` so it cannot accidentally advertise TLS on a plain HTTP listener.

Use HTTPS through a reverse proxy for an exposed instance. Keep project keys on application servers; browsers only need the issued upload URL. Configure the proxy for raw streaming uploads, an appropriate body size, and a timeout of at least five minutes. Avoid logging query strings: upload URLs contain bearer credentials.

## Operational limits

Run **one service instance per SQLite database**. Transfers stream in chunks (8 MiB writer buffer) with at most 16 simultaneous uploads and a five-minute total transfer timeout. Interrupted transfers are marked failed with best-effort object cleanup. On restart, in-progress records are marked failed; an abrupt process stop can leave partial objects or unfinished cloud multipart uploads. Configure provider lifecycle cleanup for abandoned multipart uploads. Upload records are retained; automatic retention, file deletion, quotas, and resumable/multipart client uploads are outside this initial scope.

## Backups

Back up SQLite (using its online backup mechanism, or stop the service first) and the configured local folders. Environment secrets and SSH keys stay outside SQLite. Protect database and storage folders with service-user permissions.

## Docker

```sh
export HOST_URL="http://localhost:3000/"
export ADMIN_TOKEN="$(openssl rand -hex 32)"
docker compose up --build
```

Open http://localhost:3000. For a local connection inside the container use `/data/files`. The named volume retains the database and files. Pass provider environment variables to the container as needed; mount SSH keys and known_hosts read-only for SSH storage. The image runs as UID 10001 and listens on `0.0.0.0:3000` inside the container via its `BIND_ADDR` override. Compose publishes that port to localhost on the host.

For another host-side port, set both the published port and advertised origin:

```sh
HOST_PORT=8080 HOST_URL="http://localhost:8080/" docker compose up --build
```

`HOST_URL` does not rewrite Docker port mappings. Compose accepts `PUBLIC_BASE_URL` as an override for existing deployments. The Dockerfile's `BIND_ADDR` stays fixed at `0.0.0.0:3000` regardless of the external URL.

The host default database path remains `data/upload-tool.db` for compatibility with early local installs. Set `DATABASE_PATH` to use another location.

Environment files are not loaded by the Rust binary automatically. Export variables in your shell or configure your service manager. Docker Compose reads `.env` for interpolation, but only variables listed under `environment` are passed to the container.
