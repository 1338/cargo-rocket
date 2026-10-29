"use strict";
let token = "";
let backends = [],
  projects = [];
const $ = (id) => document.getElementById(id);
const providers = {
  fs: "Local disk",
  s3: "Amazon S3",
  azblob: "Azure Blob",
  sftp: "SSH / SFTP",
  ftp: "FTP / FTPS",
};
const fields = {
  fs: [["root", "Folder path", "/absolute/path/to/uploads", true]],
  s3: [
    ["bucket", "Bucket", "my-bucket", true],
    ["region", "Region", "eu-west-1", true],
    ["endpoint", "Custom endpoint (optional)", "https://s3.example.com"],
    ["root", "Prefix (optional)", "uploads/"],
    [
      "access_key_id",
      "Access key · environment variable",
      "AWS_ACCESS_KEY_ID",
      false,
      true,
    ],
    [
      "secret_access_key",
      "Secret key · environment variable",
      "AWS_SECRET_ACCESS_KEY",
      false,
      true,
    ],
  ],
  azblob: [
    ["container", "Container", "uploads", true],
    [
      "endpoint",
      "Blob endpoint",
      "https://account.blob.core.windows.net",
      true,
    ],
    ["account_name", "Account name", "myaccount", true],
    [
      "account_key",
      "Account key · environment variable",
      "AZURE_STORAGE_KEY",
      true,
      true,
    ],
  ],
  sftp: [
    ["endpoint", "SSH endpoint", "ssh://storage.example.com:22", true],
    ["root", "Remote folder", "/srv/uploads", true],
    ["user", "SSH user", "uploads", true],
    ["key", "Private key path (optional)", "/home/service/.ssh/id_ed25519"],
  ],
  ftp: [
    ["endpoint", "FTP endpoint", "ftps://storage.example.com:21", true],
    ["root", "Remote folder", "/uploads", true],
    ["user", "FTP user", "uploads", true],
    ["password", "Password · environment variable", "FTP_PASSWORD", true, true],
  ],
};
function notify(message, error = false) {
  $("notice").textContent = message;
  $("notice").classList.toggle("error", error);
  $("notice").hidden = false;
  clearTimeout(notify.timer);
  notify.timer = setTimeout(() => ($("notice").hidden = true), 6000);
}
async function api(path, options = {}) {
  const res = await fetch(path, {
    ...options,
    headers: {
      Authorization: `Bearer ${token}`,
      "Content-Type": "application/json",
      ...options.headers,
    },
  });
  const data = await res.json();
  if (!res.ok) {
    if (res.status === 401) lock();
    throw new Error(data.error || `Request failed (${res.status})`);
  }
  return data;
}
function el(tag, text, cls) {
  const node = document.createElement(tag);
  if (text !== undefined) node.textContent = text;
  if (cls) node.className = cls;
  return node;
}
function empty(target, title, description) {
  const div = el("div", undefined, "empty");
  div.append(
    el("div", "◇", "empty-icon"),
    el("strong", title),
    el("span", description),
  );
  target.append(div);
}
function lock() {
  token = "";
  $("console").hidden = true;
  $("login").hidden = false;
  $("logout").hidden = true;
  $("admin-token").value = "";
  $("new-key").textContent = "";
  $("key-dialog").close();
}
function tab(name) {
  document.querySelectorAll(".tab").forEach((n) => (n.hidden = n.id !== name));
  document
    .querySelectorAll("[data-tab]")
    .forEach((n) =>
      n.setAttribute("aria-selected", String(n.dataset.tab === name)),
    );
}
function renderFields() {
  const wrap = $("backend-fields");
  wrap.replaceChildren();
  for (const [key, label, placeholder, required, secret] of fields[
    $("backend-kind").value
  ]) {
    const field = el("div", undefined, "field");
    const l = el("label", label);
    l.htmlFor = `opt-${key}`;
    const input = el("input");
    input.id = `opt-${key}`;
    input.name = key;
    input.placeholder = placeholder;
    input.required = !!required;
    input.autocomplete = "off";
    input.dataset.secret = String(!!secret);
    field.append(l, input);
    wrap.append(field);
  }
  if ($("backend-kind").value === "sftp")
    wrap.append(
      el(
        "p",
        "Uses the server’s OpenSSH configuration. Provision known_hosts and key access for the service user first.",
        "hint",
      ),
    );
  if ($("backend-kind").value === "s3")
    wrap.append(
      el(
        "p",
        "Leave credential references blank to use the server’s AWS credential chain.",
        "hint",
      ),
    );
}
function size(bytes) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${(bytes / 1048576).toFixed(1)} MiB`;
}
function showKey(key) {
  $("new-key").textContent = key;
  $("key-dialog").showModal();
}
async function busy(button, work) {
  button.disabled = true;
  try {
    await work();
  } catch (e) {
    notify(e.message, true);
  } finally {
    button.disabled = false;
  }
}
async function refresh() {
  [backends, projects] = await Promise.all([
    api("/api/admin/backends"),
    api("/api/admin/projects"),
  ]);
  const uploads = await api("/api/admin/uploads");
  const list = $("backend-list");
  list.replaceChildren();
  $("backend-count").textContent =
    `${backends.length} connection${backends.length === 1 ? "" : "s"}`;
  if (!backends.length)
    empty(
      list,
      "Connect your first storage",
      "A local folder is a good place to start.",
    );
  for (const b of backends) {
    const row = el("div", undefined, "row"),
      info = el("div", undefined, "row-info"),
      desc = el("div");
    desc.append(
      el("div", b.name, "row-title"),
      el(
        "div",
        `${providers[b.kind]} · ${b.options.bucket || b.options.container || b.options.root || b.options.endpoint || "Default root"}`,
        "row-sub",
      ),
    );
    info.append(el("div", b.kind === "fs" ? "▱" : "☁", "backend-icon"), desc);
    const test = el("button", "Test", "btn small");
    test.onclick = () =>
      busy(test, async () => {
        await api(`/api/admin/backends/${b.id}/test`, { method: "POST" });
        notify("Connection verified: write, read, and delete succeeded.");
      });
    row.append(info, test);
    list.append(row);
  }
  const projectList = $("project-list");
  projectList.replaceChildren();
  $("project-count").textContent =
    `${projects.length} project${projects.length === 1 ? "" : "s"}`;
  if (!projects.length)
    empty(
      projectList,
      "Give your application a key",
      "Create a project to start requesting uploads.",
    );
  for (const p of projects) {
    const row = el("div", undefined, "row"),
      desc = el("div");
    desc.append(
      el("div", p.name, "row-title"),
      el(
        "div",
        `${backends.find((b) => b.id === p.backend_id)?.name || "Unknown storage"} · ${size(p.max_size)} limit`,
        "row-sub",
      ),
      el("div", p.allowed_types.join(", ") || "All content types", "hint"),
    );
    const rotate = el("button", "Rotate key", "btn small");
    rotate.onclick = () => {
      if (
        !confirm(
          `Replace the API key for ${p.name}? The previous key stops working immediately.`,
        )
      )
        return;
      busy(rotate, async () => {
        const r = await api(`/api/admin/projects/${p.id}/rotate-key`, {
          method: "POST",
        });
        showKey(r.api_key);
      });
    };
    row.append(desc, rotate);
    projectList.append(row);
  }
  const select = $("project-backend");
  select.replaceChildren();
  if (!backends.length) {
    const opt = el("option", "Add a storage connection first");
    opt.value = "";
    select.append(opt);
  }
  for (const b of backends) {
    const opt = el("option", b.name);
    opt.value = b.id;
    select.append(opt);
  }
  const tbody = $("upload-list");
  tbody.replaceChildren();
  $("upload-empty").hidden = uploads.length > 0;
  for (const f of uploads) {
    const row = el("tr");
    row.append(
      el("td", f.filename, "filename"),
      el(
        "td",
        projects.find((p) => p.id === f.project_id)?.name || f.project_id,
      ),
      el("td", size(f.size)),
    );
    const status = el("td");
    status.append(el("span", f.status, `badge ${f.status}`));
    row.append(
      status,
      el("td", new Date(f.created_at * 1000).toLocaleString()),
    );
    tbody.append(row);
  }
}
$("login-form").onsubmit = (e) => {
  e.preventDefault();
  busy(e.submitter || e.currentTarget.querySelector("button"), async () => {
    token = $("admin-token").value;
    await refresh();
    $("admin-token").value = "";
    $("login").hidden = true;
    $("console").hidden = false;
    $("logout").hidden = false;
  });
};
$("logout").onclick = lock;
document
  .querySelectorAll("[data-tab]")
  .forEach((b) => (b.onclick = () => tab(b.dataset.tab)));
$("backend-kind").onchange = renderFields;
$("backend-form").onsubmit = (e) => {
  e.preventDefault();
  busy(e.submitter || e.currentTarget.querySelector("button"), async () => {
    const options = {},
      secrets = {};
    for (const input of $("backend-fields").querySelectorAll("input")) {
      const value = input.value.trim();
      if (value)
        (input.dataset.secret === "true" ? secrets : options)[input.name] =
          value;
    }
    await api("/api/admin/backends", {
      method: "POST",
      body: JSON.stringify({
        name: $("backend-name").value.trim(),
        kind: $("backend-kind").value,
        options,
        secrets,
      }),
    });
    $("backend-form").reset();
    renderFields();
    await refresh();
    notify("Storage connection added. Use Test to verify access.");
  });
};
$("project-form").onsubmit = (e) => {
  e.preventDefault();
  busy(e.submitter || e.currentTarget.querySelector("button"), async () => {
    const r = await api("/api/admin/projects", {
      method: "POST",
      body: JSON.stringify({
        name: $("project-name").value.trim(),
        backend_id: $("project-backend").value,
        max_size: Math.floor(Number($("project-size").value) * 1048576),
        allowed_types: $("project-types")
          .value.split(",")
          .map((s) => s.trim())
          .filter(Boolean),
      }),
    });
    $("project-form").reset();
    showKey(r.api_key);
    await refresh();
  });
};
$("refresh").onclick = (e) => busy(e.currentTarget, refresh);
$("close-key").onclick = () => {
  $("key-dialog").close();
  $("new-key").textContent = "";
};
$("key-dialog").addEventListener(
  "close",
  () => ($("new-key").textContent = ""),
);
$("copy-key").onclick = () =>
  busy($("copy-key"), async () => {
    await navigator.clipboard.writeText($("new-key").textContent);
    notify("API key copied.");
  });
$("request-example").textContent =
  `curl -X POST '${location.origin}/api/uploads' \\\n  -H 'Authorization: Bearer YOUR_PROJECT_KEY' \\\n  -H 'Content-Type: application/json' \\\n  -d '{\n    "filename": "photo.png",\n    "content_type": "image/png",\n    "size": 12345\n  }'`;
renderFields();
