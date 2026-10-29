# Security

Cargo Rocket is early-stage software. Its current security boundary separates trusted operators, project API keys, and single-use upload grants. It is not a replacement for application-level user authorization or a malware scanning service.

## Reporting a vulnerability

Please do not open a public issue containing credentials, exploit details, or private file URLs. Send a private report to **maxvandersluis@yahoo.com** with reproduction steps, affected version/commit, expected behavior, and impact. No response-time SLA is offered.

Security fixes target the latest code on `main`; no older release support policy is established yet.

## Deployment expectations

- Terminate HTTPS before exposing an instance beyond localhost.
- Keep the admin token and project keys on trusted servers. A project key can request uploads and read every completed file in that project.
- Treat upload URLs as bearer credentials; do not log their query strings.
- Run as a dedicated user with access only to intended storage paths and credentials. Admins can configure filesystem roots and network endpoints.
- Set explicit browser origins if enabling CORS, and apply request/connection limits at the proxy.
- Verify SFTP host keys in advance and use FTPS when FTP traffic requires transport encryption.
- Do not assume declared MIME types are verified file contents. Uploaded files are untrusted.
- Retain backups and configure provider cleanup for abandoned multipart uploads.

See [architecture](docs/architecture.md) for the token and isolation model, and [deployment](docs/deployment.md) for operational limits.
