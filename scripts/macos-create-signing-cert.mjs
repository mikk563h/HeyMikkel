/**
 * Opretter selvunderskrevet "HeyMikkelDev" certifikat til code signing.
 * Bruger en dedikeret keychain (~/.heymikkel-signing.keychain-db) med tomt
 * password — undgår at röre login keychain og dens adgangskode.
 *
 * Køreres automatisk som del af tauri:macos:app.
 */
import { execFileSync, execSync } from "node:child_process";
import { writeFileSync, unlinkSync, existsSync } from "node:fs";

const CN = "HeyMikkelDev";
const KEYCHAIN = `${process.env.HOME}/.heymikkel-signing.keychain-db`;

// Opret keychain med tomt password hvis den ikke eksisterer
if (!existsSync(KEYCHAIN)) {
  execFileSync("security", ["create-keychain", "-p", "", KEYCHAIN]);
  process.stdout.write(`[HeyMikkel] Oprettede dedikeret signing keychain.\n`);
}

// Lås altid op (tomt password) og sæt auto-lock til aldrig
execFileSync("security", ["unlock-keychain", "-p", "", KEYCHAIN]);
execFileSync("security", ["set-keychain-settings", KEYCHAIN]); // ingen auto-lock

// Check om certifikatet allerede eksisterer i vores keychain
try {
  execFileSync("security", ["find-certificate", "-c", CN, KEYCHAIN], { stdio: "pipe" });
  // Sørg for at keychain er i søgelisten ved hvert kald
  addToSearchList();
  process.stdout.write(`[HeyMikkel] Certifikat '${CN}' eksisterer allerede — intet at gøre.\n`);
  process.exit(0);
} catch {
  /* ikke fundet → opret */
}

process.stdout.write(`[HeyMikkel] Opretter selvunderskrevet certifikat '${CN}'...\n`);

const cfg = "/tmp/hm_cert.cnf";
writeFileSync(
  cfg,
  `[req]
default_md = sha256
prompt = no
distinguished_name = dn
x509_extensions = v3_req
[dn]
CN = ${CN}
[v3_req]
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
basicConstraints = critical,CA:FALSE
`,
);

const p12pass = "hmdev2024";

execSync(
  `openssl req -new -newkey rsa:2048 -days 3650 -nodes -x509 \
  -config ${cfg} -keyout /tmp/hm.key -out /tmp/hm.crt 2>/dev/null`,
);

execSync(
  `openssl pkcs12 -export -in /tmp/hm.crt -inkey /tmp/hm.key \
  -out /tmp/hm.p12 -passout pass:${p12pass} 2>/dev/null`,
);

execFileSync("security", [
  "import", "/tmp/hm.p12",
  "-k", KEYCHAIN,
  "-P", p12pass,
  "-T", "/usr/bin/codesign",
  "-A",
]);

// Giv codesign adgang til privatnøglen uden dialog (tomt password, vores keychain)
execFileSync("security", [
  "set-key-partition-list",
  "-S", "apple-tool:,apple:,codesign:",
  "-s", "-k", "",
  KEYCHAIN,
]);

// Trust certifikatet til code signing
execFileSync("security", [
  "add-trusted-cert", "-d", "-r", "trustRoot",
  "-k", KEYCHAIN,
  "/tmp/hm.crt",
]);

addToSearchList();

for (const f of [cfg, "/tmp/hm.key", "/tmp/hm.crt", "/tmp/hm.p12"]) {
  try { unlinkSync(f); } catch { /* ignore */ }
}

process.stdout.write(
  `[HeyMikkel] Certifikat '${CN}' oprettet — TCC-tilladelser overlever fremtidige rebuilds.\n`,
);

function addToSearchList() {
  // Tilføj vores keychain til brugerens keychain-søgeliste (idempotent)
  try {
    const current = execFileSync("security", ["list-keychains", "-d", "user"])
      .toString()
      .split("\n")
      .map((l) => l.trim().replace(/^"|"$/g, ""))
      .filter(Boolean);
    if (!current.includes(KEYCHAIN)) {
      execFileSync("security", ["list-keychains", "-d", "user", "-s", KEYCHAIN, ...current]);
    }
  } catch { /* ignore */ }
}
