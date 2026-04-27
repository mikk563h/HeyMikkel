/**
 * Deep ad hoc sign af debug .app (Mikrofon-entitlements i hele bundtet).
 * Brug efter: npm run tauri -- build --debug
 */
import { readdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const macosDir = join(root, "src-tauri", "target", "debug", "bundle", "macos");
const ent = join(root, "src-tauri", "Entitlements.plist");

if (!existsSync(macosDir)) {
  process.stderr.write("Mangler debug bundle. Kør først: npm run tauri -- build --debug\n");
  process.exit(1);
}
const apps = readdirSync(macosDir).filter((f) => f.endsWith(".app"));
if (!apps.length) {
  process.stderr.write("Ingen .app i bundle/macos.\n");
  process.exit(1);
}
const appPath = join(macosDir, apps[0]);
execFileSync(
  "codesign",
  ["--force", "--sign", "-", "--timestamp=none", "--deep", "--entitlements", ent, appPath],
  { stdio: "inherit" },
);
process.stdout.write(`[Hey Mikkel] codesign (deep) OK: ${appPath}\n`);
