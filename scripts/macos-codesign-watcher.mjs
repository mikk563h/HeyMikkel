/**
 * tauri dev: ad hoc + entitlements på den rigtige hey-mikkel (finder sti via cargo metadata
 * så Cursor/CARGO_TARGET_DIR-biblioteker stadig findes).
 */
import { statSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { getHeyMikkelBinaryPaths, findExistingHeyMikkelBin } from "./hey-mikkel-cargo-paths.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const ent = join(root, "src-tauri/Entitlements.plist");

let lastSignedMtime = 0;
let lastErrorAt = 0;

function signBinarySync() {
  const bin = findExistingHeyMikkelBin() ?? getHeyMikkelBinaryPaths().find((p) => existsSync(p));
  if (!bin) return;
  let mtime = 0;
  try {
    mtime = statSync(bin).mtimeMs;
  } catch {
    return;
  }
  if (mtime <= lastSignedMtime) return;
  try {
    execFileSync("codesign", [
      "--force",
      "--sign",
      "-",
      "--timestamp=none",
      "--entitlements",
      ent,
      bin,
    ], { stdio: "pipe" });
    // Efter codesign ændres mtime — læs igen, ellers looper vi (sign → ny mtime → sign igen).
    lastSignedMtime = statSync(bin).mtimeMs;
    process.stdout.write(
      `[Hey Mikkel] codesign OK\n  → ${bin}\n  Genstart Hey Mikkel (afslut i menu) og klik så "Tillad mikrofon".\n`,
    );
  } catch (e) {
    const now = Date.now();
    if (now - lastErrorAt < 10_000) return;
    lastErrorAt = now;
    process.stderr.write(
      `[Hey Mikkel] codesign fejlede (luk app og prøv: npm run tauri:sign-macos). ${String(e).slice(0, 200)}\n`,
    );
  }
}

setInterval(signBinarySync, 500);
setTimeout(signBinarySync, 200);
process.stdout.write(
  "[Hey Mikkel] Følger heymikkel-binæren (cargo metadata) og underskriver for Mikrofontilladelse…\n",
);
