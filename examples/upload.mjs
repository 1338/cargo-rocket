/**
 * Server-side Node.js 22+ example, with no external dependencies.
 * PROJECT_KEY=... node examples/upload.mjs ./photo.png image/png
 * Optional: HOST_URL=https://cargo.example.com (CARGO_BASE_URL also supported)
 * Keep the project key on your application server, never in browser code.
 */
import { open, stat } from "node:fs/promises";
import { basename } from "node:path";

const [path, contentType = "application/octet-stream"] = process.argv.slice(2);
const key = process.env.PROJECT_KEY;
const base = (
  process.env.CARGO_BASE_URL ||
  process.env.HOST_URL ||
  "http://localhost:3000"
).replace(/\/$/, "");
if (!path || !key) {
  console.error(
    "Usage: PROJECT_KEY=... node examples/upload.mjs <file> [content-type]",
  );
  process.exit(1);
}

async function checked(response) {
  if (!response.ok)
    throw new Error(`HTTP ${response.status}: ${await response.text()}`);
  return response;
}

try {
  const info = await stat(path);
  if (!info.isFile()) throw new Error("Input must be a regular file");
  const ticket = await (
    await checked(
      await fetch(`${base}/api/uploads`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${key}`,
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          filename: basename(path),
          content_type: contentType,
          size: info.size,
        }),
      }),
    )
  ).json();

  const file = await open(path, "r");
  try {
    const response = await checked(
      await fetch(ticket.upload_url, {
        method: ticket.method,
        headers: { ...ticket.headers, "Content-Length": String(info.size) },
        body: file.createReadStream({ autoClose: false }),
        duplex: "half",
      }),
    );
    const result = await response.json();
    // The upload URL is a secret; only print the completed file's metadata.
    console.log(
      JSON.stringify(
        { id: result.id, status: result.status, file_url: ticket.file_url },
        null,
        2,
      ),
    );
  } finally {
    await file.close();
  }
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
