# Arbejd på Hey Mikkel fra en anden computer

## 1. Få koden op på GitHub (én gang, fra en maskine med projektmappen)

1. Opret et **tomt** repository på [github.com](https://github.com/new) (uden README, uden .gitignore — Cursor har allerede det).
2. I Terminal (i projektmappen):

```bash
cd "/sti/til/HeyMikkel"
git remote add origin https://github.com/DIN-BRUGER/DIT-REPO-NAVN.git
git push -u origin main
```

Brug **HTTPS**-URL’en som GitHub viser, eller **SSH** hvis du har nøgler.

Hvis `git remote` allerede findes, tjek med `git remote -v` — skift URL med `git remote set-url origin ...`.

## 2. På den anden Mac

Installer:

- [Node.js](https://nodejs.org/) (LTS)
- [Rust](https://rustup.rs/) (`curl … | sh`)
- På **macOS**: **Xcode Command Line Tools** (fx `xcode-select --install`).

Klon og kør:

```bash
git clone https://github.com/DIN-BRUGER/DIT-REPO-NAVN.git
cd DIT-REPO-NAVN
npm install
npm run tauri dev
```

(Producerings-build: `npm run tauri:macos:app` — kræver Apple-codesign-kæde lokal; til udvikling rækker `tauri dev`.)

## 3. Mikrofon: når Lyd ind er tom («Ingen enheder»)

**Det er ikke en fejl i Hey Mikkel** — macOS ser ingen lyd-**input**-kilder. Især **Mac mini / Mac Studio** har ofte **ingen indbygget mikrofon**. Du skal tilslutte f.eks. **USB-mik, USB/headset** eller andet, indtil **Systemindstillinger → Lyd → Lyd ind** viser mindst **én** enhed. Derefter virker `npm run tauri dev` + Test mikrofon som regel, når appen også har lov under **Privatliv → Mikrofon**.

Efter nyt udstyr: prøv `sudo killall coreaudiod` i Terminal, eller genstart Mac’en.
