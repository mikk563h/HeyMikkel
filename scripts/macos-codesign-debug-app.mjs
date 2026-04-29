/**
 * Sign debug .app med det stabile "HeyMikkelDev" certifikat fra dedikeret keychain.
 * Stabil signatur = TCC-tilladelser (mikrofon, skærmoptagelse) overlever rebuilds.
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
const KEYCHAIN = `${process.env.HOME}/.heymikkel-signing.keychain-db`;

if (!existsSync(macosDir)) {
  process.stderr.write("Mangler debug bundle. Kør først: npm run tauri -- build --debug\n");
  process.exit(1);
}
const apps = readdirSync(macosDir).filter((f) => f.endsWith(".app"));
if (!apps.length) {
  process.stderr.write("Ingen .app i bundle/macos.\n");
  process.exit(1);
}

// Lås keychain op (tomt password) så codesign kan bruge privatnøglen
try {
  execFileSync("security", ["unlock-keychain", "-p", "", KEYCHAIN], { stdio: "pipe" });
} catch { /* ignore */ }

const appPath = join(macosDir, apps[0]);
execFileSync(
  "codesign",
  [
    "--force",
    "--sign", "HeyMikkelDev",
    "--keychain", KEYCHAIN,
    "--timestamp=none",
    "--deep",
    "--entitlements", ent,
    appPath,
  ],
  { stdio: "inherit" },
);
process.stdout.write(`[Hey Mikkel] codesign (deep) OK: ${appPath}\n`);
