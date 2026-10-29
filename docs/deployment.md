# Configuration and deployment

[← README](../README.md)

| Variable | Default | Purpose |
|---|---|---|
| `ADMIN_TOKEN` | Required, 32+ characters | Dashboard and management API secret |
| `BIND_ADDR` | `127.0.0.1:3000` | HTTP listener |
| `PUBLIC_BASE_URL` | `http://localhost:3000` | External URL used to generate upload/file URLs |
| `DATABASE_PATH` | `data/upload-tool.db` | SQLite database |
| `CORS_ORIGINS` | No cross-origin access | Comma-separated allowed browser origins |
| `RUST_LOG` | Unset | Log filter, e.g. `cargo_rocket=info` |

Use HTTPS through a reverse proxy for an exposed instance; set `PUBLIC_BASE_URL` to its external HTTPS URL. Keep project keys on application servers; browsers only need the issued upload URL. Configure the proxy for raw streaming uploads, an appropriate body size, and a timeout of at least five minutes. Avoid logging query strings: upload URLs contain bearer credentials.

## Operational limits

Run **one service instance per SQLite database**. Transfers stream in chunks (8 MiB writer buffer) with at most 16 simultaneous uploads and a five-minute total transfer timeout. Interrupted transfers are marked failed with best-effort object cleanup. On restart, in-progress records are marked failed; an abrupt process stop can leave partial objects or unfinished cloud multipart uploads. Configure provider lifecycle cleanup for abandoned multipart uploads. Upload records are retained; automatic retention, file deletion, quotas, and resumable/multipart client uploads are outside this initial scope.

## Backups

Back up SQLite (using its online backup mechanism, or stop the service first) and the configured local folders. Environment secrets and SSH keys stay outside SQLite. Protect database and storage folders with service-user permissions.

## Docker

```sh
export ADMIN_TOKEN="$(openssl rand -hex 32)"
docker compose up --build
```

Open http://localhost:3000. For a local connection inside the container use `/data/files`. The named volume retains the database and files. Pass provider environment variables to the container as needed; mount SSH keys and known_hosts read-only for SSH storage. The image runs as UID 10001.

The host default database path remains `data/upload-tool.db` for compatibility with early local installs. Set `DATABASE_PATH` to use another location.

Environment files are not loaded by the Rust binary automatically. Export variables in your shell or configure your service manager. Docker Compose reads `.env` for interpolation, but only variables listed under `environment` are passed to the container.
