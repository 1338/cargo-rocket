#!/usr/bin/env bash
# Verify an existing project's upload flow over HTTP. Requires curl and jq.
set -euo pipefail
: "${PROJECT_KEY:?Set PROJECT_KEY to an existing project API key}"
base="${CARGO_BASE_URL:-${HOST_URL:-http://localhost:3000}}"
base="${base%/}"
for command in curl jq cmp; do
  command -v "$command" >/dev/null || { printf 'Missing required command: %s\n' "$command" >&2; exit 1; }
done
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
printf 'hello' > "$scratch/source.txt"
# The project must allow text/plain files of at least five bytes.
ticket=$(curl --fail-with-body --silent --show-error "$base/api/uploads" \
  -H "Authorization: Bearer $PROJECT_KEY" \
  -H 'Content-Type: application/json' \
  -d '{"filename":"smoke-test.txt","content_type":"text/plain","size":5}')
url=$(printf '%s' "$ticket" | jq -er '.upload_url')
id=$(printf '%s' "$ticket" | jq -er '.id')
curl --fail-with-body --silent --show-error -X PUT "$url" \
  -H 'Content-Type: text/plain' --data-binary "@$scratch/source.txt" > "$scratch/result.json"
jq -e '.status == "complete"' "$scratch/result.json" >/dev/null
curl --fail-with-body --silent --show-error "$base/api/files/$id" \
  -H "Authorization: Bearer $PROJECT_KEY" | jq -e '.status == "complete" and .size == 5' >/dev/null
curl --fail-with-body --silent --show-error "$base/api/files/$id/content" \
  -H "Authorization: Bearer $PROJECT_KEY" -o "$scratch/download.txt"
cmp "$scratch/source.txt" "$scratch/download.txt"
status=$(curl --silent --show-error -o /dev/null -w '%{http_code}' -X PUT "$url" \
  -H 'Content-Type: text/plain' --data-binary "@$scratch/source.txt")
if [[ "$status" != 409 ]]; then
  printf 'Expected replay rejection (409), got %s\n' "$status" >&2
  exit 1
fi
printf 'PASS: upload, metadata, download, and replay protection. File ID: %s\n' "$id"
