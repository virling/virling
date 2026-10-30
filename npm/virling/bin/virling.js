#!/usr/bin/env node
// Downloads the Virling release binary for this machine once, checks its checksum, then runs it with the given arguments.
const { spawn, spawnSync } = require("node:child_process");
const crypto = require("node:crypto");
const fs = require("node:fs");
const https = require("node:https");
const os = require("node:os");
const path = require("node:path");

const version = require("../package.json").version;
const targets = {
  "linux-x64": "x86_64-unknown-linux-gnu",
  "linux-arm64": "aarch64-unknown-linux-gnu",
  "darwin-x64": "x86_64-apple-darwin",
  "darwin-arm64": "aarch64-apple-darwin",
};
const target = targets[`${process.platform}-${process.arch}`];
if (!target) {
  console.error(`virling: no release binary for ${process.platform} ${process.arch}. Use Docker or build from source: https://docs.virling.com/install/`);
  process.exit(1);
}
const name = `virling-v${version}-${target}`;
const dir = path.join(process.env.VIRLING_CACHE || path.join(os.homedir(), ".cache", "virling"), version);
const bin = path.join(dir, name, "virling");
const base = `https://github.com/virling/virling/releases/download/v${version}/${name}.tar.gz`;

function get(url) {
  return new Promise((resolve, reject) => {
    https.get(url, { headers: { "User-Agent": "virling-npm" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        return resolve(get(new URL(res.headers.location, url).toString()));
      }
      if (res.statusCode !== 200) {
        res.resume();
        return reject(new Error(`${url} answered ${res.statusCode}`));
      }
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => resolve(Buffer.concat(chunks)));
    }).on("error", reject);
  });
}

async function install() {
  const [archive, sum] = await Promise.all([get(base), get(`${base}.sha256`)]);
  const want = sum.toString().trim().split(/\s+/)[0];
  const got = crypto.createHash("sha256").update(archive).digest("hex");
  if (want !== got) throw new Error(`checksum mismatch for ${name}.tar.gz`);
  fs.mkdirSync(dir, { recursive: true });
  const file = path.join(dir, `${name}.tar.gz`);
  fs.writeFileSync(file, archive);
  const tar = spawnSync("tar", ["xzf", file, "-C", dir], { stdio: "inherit" });
  fs.rmSync(file);
  if (tar.status !== 0) throw new Error("could not unpack the release archive");
}

(async () => {
  if (!fs.existsSync(bin)) {
    console.error(`virling: downloading v${version} for ${target}...`);
    await install();
  }
  const child = spawn(bin, process.argv.slice(2), { stdio: "inherit" });
  for (const sig of ["SIGINT", "SIGTERM"]) process.on(sig, () => child.kill(sig));
  child.on("exit", (code, signal) => (signal ? process.kill(process.pid, signal) : process.exit(code ?? 1)));
})().catch((err) => {
  console.error(`virling: ${err.message}`);
  process.exit(1);
});
