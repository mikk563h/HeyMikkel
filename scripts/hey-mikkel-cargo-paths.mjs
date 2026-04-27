/**
 * Finder alle plausible stier til hey-mikkel-binæren (inkl. CARGO_TARGET_DIR / Cursor-sandbox).
 */
import { readdirSync, statSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const tauriCwd = join(root, "src-tauri");

let cached = { t: 0, paths: [] };
const METADATA_TTL_MS = 4000;

function pathsFromTargetDir(tDir) {
  const out = [];
  const base = tDir;
  if (!existsSync(base)) return out;
  const direct = join(base, "debug", "hey-mikkel");
  if (existsSync(direct)) out.push(direct);
  try {
    for (const name of readdirSync(base)) {
      if (name === "debug" || name === "release" || name.startsWith(".")) continue;
      const p = join(base, name, "debug", "hey-mikkel");
      if (existsSync(p)) out.push(p);
    }
  } catch {
    // ignore
  }
  return out;
}

/**
 * @returns {string[]}
 */
export function getHeyMikkelBinaryPaths() {
  const now = Date.now();
  if (now - cached.t < METADATA_TTL_MS && cached.paths.length) {
    return cached.paths;
  }
  const fromMeta = [];
  try {
    const raw = execFileSync("cargo", ["metadata", "--format-version", "1", "--no-deps"], {
      encoding: "utf8",
      cwd: tauriCwd,
      stdio: ["ignore", "pipe", "ignore"],
    });
    const meta = JSON.parse(raw);
    if (meta.target_directory) {
      for (const p of pathsFromTargetDir(meta.target_directory)) {
        if (!fromMeta.includes(p)) fromMeta.push(p);
      }
    }
  } catch {
    // cargo metadata fejlede — brug static paths
  }
  const combined = [];
  const seen = new Set();
  for (const p of [...fromMeta, ...getStaticCandidatePaths()]) {
    if (!seen.has(p)) {
      seen.add(p);
      combined.push(p);
    }
  }
  cached = { t: now, paths: combined };
  return cached.paths;
}

function getStaticCandidatePaths() {
  return [
    join(root, "src-tauri/target/debug/hey-mikkel"),
    join(root, "src-tauri/target/aarch64-apple-darwin/debug/hey-mikkel"),
    join(root, "src-tauri/target/x86_64-apple-darwin/debug/hey-mikkel"),
    join(root, "target/debug/hey-mikkel"),
  ];
}

export function findExistingHeyMikkelBin() {
  for (const p of getHeyMikkelBinaryPaths()) {
    try {
      if (statSync(p).isFile()) return p;
    } catch {
      // continue
    }
  }
  return null;
}
