# Storage backends

[← README](../README.md)

Add connections in the dashboard, or POST the same JSON to `/api/admin/backends` using the admin Bearer token. Connection definitions are immutable to preserve existing file locations. Create a new connection and project when moving to a different destination.

## Local disk

```json
{"name":"Local uploads","kind":"fs","options":{"root":"/srv/rocket-cargo/files"}}
```

## AWS S3 / compatible

```json
{
  "name":"Production S3",
  "kind":"s3",
  "options":{"bucket":"my-uploads","region":"eu-west-1","root":"uploads/"},
  "secrets":{"access_key_id":"AWS_ACCESS_KEY_ID","secret_access_key":"AWS_SECRET_ACCESS_KEY"}
}
```

Set the referenced variables on the **server process** before starting it. Omit `secrets` to use OpenDAL's AWS credential chain (for example an instance role). Optional `endpoint` supports MinIO and other S3-compatible services; default addressing is path-style, with optional `enable_virtual_host_style: "true"`. Optional session credentials can be referenced with `session_token`. The bucket must already exist.

## Azure Blob Storage

```json
{
  "name":"Azure uploads",
  "kind":"azblob",
  "options":{"container":"uploads","account_name":"myaccount","endpoint":"https://myaccount.blob.core.windows.net"},
  "secrets":{"account_key":"AZURE_STORAGE_KEY"}
}
```

The container must already exist. `sas_token` can be used instead of `account_key` through a secret reference.

## FTP / FTPS

```json
{
  "name":"FTP uploads",
  "kind":"ftp",
  "options":{"endpoint":"ftps://storage.example.com:21","user":"uploads","root":"/uploads/"},
  "secrets":{"password":"FTP_PASSWORD"}
}
```

Use `ftp://` for unencrypted FTP or `ftps://` for TLS. The account needs directory creation, file write, read, and delete permissions.

## SSH / SFTP (Unix)

```json
{
  "name":"SSH folder",
  "kind":"sftp",
  "options":{"endpoint":"ssh://storage.example.com:22","user":"uploads","root":"/srv/uploads/","key":"/home/service/.ssh/id_ed25519"}
}
```

`key` is a server-side private-key **file path**, not key contents. Omitting it uses OpenSSH's defaults. Host verification is strict; establish a trusted `known_hosts` entry for the service user first. SSH agent/config authentication is supported by the underlying OpenSSH client. Password login is not provided.

The connection test writes a unique probe, reads it back, then deletes it. Remote providers are integrated through [OpenDAL](https://docs.rs/opendal/0.54.1/opendal/services/index.html); live provider validation requires your own credentials.
