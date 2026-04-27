/**
 * På macOS: fjern CARGO_TARGET_DIR (sættes ofte af Cursor/IDE), så binæren bygges
 * i src-tauri/target/ og det samme sted som vores codesign rammer.
 */
import { spawn } from "node:child_process";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { existsSync } from "node:fs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const tauriBin = join(root, "node_modules", ".bin", "tauri");
const args = process.argv.slice(2);
const env = { ...process.env };
if (process.platform === "darwin") {
  delete env.CARGO_TARGET_DIR;
}
const useCmd = existsSync(tauriBin) ? tauriBin : "npx";
const useArgs = existsSync(tauriBin) ? args : ["--no", "tauri", ...args];
const child = spawn(useCmd, useArgs, { stdio: "inherit", env, shell: false, cwd: root });
child.on("exit", (c) => process.exit(c ?? 0));
child.on("error", (e) => {
  process.stderr.write(String(e) + "\n");
  process.exit(1);
});
