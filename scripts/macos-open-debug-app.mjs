/**
 * Åbner den debug-byggede .app (korrekt bundle + plists i forhold til rå heymikkel-binær).
 * Brug hvis tauri dev + codesign ikke giver række i Systemindstillinger.
 */
import { readdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const macosDir = join(root, "src-tauri", "target", "debug", "bundle", "macos");

if (!existsSync(macosDir)) {
  process.stderr.write("Mangler debug-bundle. Kør: npm run tauri:macos:app (uden extra args)\n");
  process.exit(1);
}
const apps = readdirSync(macosDir).filter((f) => f.endsWith(".app"));
if (!apps.length) {
  process.stderr.write("Ingen .app i bundle/macos. Kør: npm run tauri:macos:app\n");
  process.exit(1);
}
const app = join(macosDir, apps[0]);
execFileSync("open", [app], { stdio: "inherit" });
process.stdout.write(`Åbner: ${app}\n`);
