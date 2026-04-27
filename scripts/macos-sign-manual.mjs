import { existsSync, statSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { getHeyMikkelBinaryPaths, findExistingHeyMikkelBin } from "./hey-mikkel-cargo-paths.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const root = join(here, "..");
const ent = join(root, "src-tauri/Entitlements.plist");

const bin = findExistingHeyMikkelBin() ?? getHeyMikkelBinaryPaths().find((p) => {
  try {
    return statSync(p).isFile();
  } catch {
    return false;
  }
});

if (!bin || !existsSync(bin)) {
  process.stderr.write("Fandt ikke hey-mikkel. Kør: cd src-tauri && cargo build\n");
  process.exit(1);
}

execFileSync("codesign", [
  "--force",
  "--sign",
  "-",
  "--timestamp=none",
  "--entitlements",
  ent,
  bin,
], { stdio: "inherit" });
process.stdout.write(`OK: ${bin}\n`);
