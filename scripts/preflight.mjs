#!/usr/bin/env node
// 发布前预检：版本一致性、sidecar 架构、图标、必要文件。
// 用法：node scripts/preflight.mjs   （或 pnpm preflight）
// 退出码非 0 表示有阻断项，CI 里应当失败。

import fs from "node:fs";
import path from "node:path";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");

const red = (s) => `\x1b[31m${s}\x1b[0m`;
const green = (s) => `\x1b[32m${s}\x1b[0m`;
const yellow = (s) => `\x1b[33m${s}\x1b[0m`;
const cyan = (s) => `\x1b[36m${s}\x1b[0m`;

let errors = 0;
let warnings = 0;

const step = (msg) => console.log(cyan(`▸ ${msg}`));
const ok = (msg) => console.log(green(`  ✓ ${msg}`));
const warn = (msg) => {
  warnings += 1;
  console.log(yellow(`  ! ${msg}`));
};
const bad = (msg) => {
  errors += 1;
  console.log(red(`  ✗ ${msg}`));
};

const readJson = (p) => JSON.parse(fs.readFileSync(path.join(ROOT, p), "utf8"));
const exists = (p) => fs.existsSync(path.join(ROOT, p));

/* ---------------------------------------------------------------- 1. 版本 */

step("版本一致性");
const pkg = readJson("package.json");
const tauriConf = readJson("src-tauri/tauri.conf.json");
const cargoToml = fs.readFileSync(path.join(ROOT, "src-tauri/Cargo.toml"), "utf8");
const cargoVersion = (cargoToml.match(/^version\s*=\s*"([^"]+)"/m) || [])[1];

const versions = [
  ["package.json", pkg.version],
  ["tauri.conf.json", tauriConf.version],
  ["Cargo.toml", cargoVersion],
];
const distinct = new Set(versions.map(([, v]) => v));
if (distinct.size === 1) {
  ok(`全部为 ${pkg.version}`);
} else {
  bad(versions.map(([f, v]) => `${f}=${v}`).join("  "));
}

/* ------------------------------------------------------------- 2. sidecar */

step("frpc sidecar");
const triples = {
  "darwin-arm64": "aarch64-apple-darwin",
  "darwin-x64": "x86_64-apple-darwin",
  "win32-x64": "x86_64-pc-windows-msvc",
  "linux-x64": "x86_64-unknown-linux-gnu",
};
const currentTriple = triples[`${process.platform}-${process.arch}`];
const binDir = path.join(ROOT, "src-tauri/binaries");
const suffix = process.platform === "win32" ? ".exe" : "";

const listed = fs.existsSync(binDir)
  ? fs.readdirSync(binDir).filter((f) => f.startsWith("frpc-"))
  : [];

if (listed.length === 0) {
  warn("binaries/ 里还没有内核，运行时只能靠用户手动指定路径");
} else {
  ok(`已内置：${listed.join(", ")}`);
}

if (!currentTriple) {
  warn(`未识别的平台 ${process.platform}/${process.arch}，跳过当前平台检查`);
} else {
  const expected = `frpc-${currentTriple}${suffix}`;
  const p = path.join(binDir, expected);
  if (fs.existsSync(p)) {
    ok(`${expected}（${(fs.statSync(p).size / 1024 / 1024).toFixed(1)} MB）`);
    try {
      const out = execFileSync(p, ["-v"], { encoding: "utf8", timeout: 5000 }).trim();
      ok(`版本：${out || "(空)"}`);
    } catch (e) {
      warn(`无法执行 -v：${e.message.split("\n")[0]}`);
    }
    if (process.platform !== "win32") {
      const mode = fs.statSync(p).mode & 0o777;
      if ((mode & 0o111) === 0) bad(`${expected} 没有执行权限，chmod +x 一下`);
      else ok("执行权限正常");
    }
  } else {
    bad(`缺少当前平台内核：src-tauri/binaries/${expected}`);
    if (process.platform === "darwin" && process.arch === "arm64") {
      console.log("      cp ../frp_0.52.3_darwin_arm64/frpc src-tauri/binaries/" + expected);
    }
  }
}

/* ----------------------------------------------------------------- 3. 图标 */

step("图标与 entitlements");
for (const need of ["icons/icon.icns", "icons/icon.ico", "icons/32x32.png"]) {
  const p = path.join("src-tauri", need);
  if (exists(p)) ok(p);
  else bad(`缺少 ${p} —— 先跑 pnpm tauri icon ./logo.png`);
}
if (exists("src-tauri/entitlements.plist")) ok("src-tauri/entitlements.plist");
else bad("缺少 entitlements.plist（hardened runtime 必需）");

/* ------------------------------------------------------- 4. 构建配置核对 */

step("bundle 配置");
const bundle = tauriConf.bundle || {};
if ((bundle.externalBin || []).includes("binaries/frpc")) ok("externalBin 已声明 binaries/frpc");
else bad("tauri.conf.json 的 bundle.externalBin 缺 binaries/frpc");

if (bundle.macOS?.hardenedRuntime) ok("macOS hardenedRuntime 已开启");
else warn("macOS hardenedRuntime 未开启，公证大概率失败");

/* --------------------------------------------------------------- 汇总 */

console.log("");
if (errors > 0) {
  console.log(red(`预检失败：${errors} 个阻断项，${warnings} 个警告`));
  process.exit(1);
}
if (warnings > 0) {
  console.log(yellow(`预检通过（有 ${warnings} 个警告）`));
} else {
  console.log(green("预检通过"));
}
