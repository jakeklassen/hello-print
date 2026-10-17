# Hello Print

A cross-platform desktop application built with Tauri that automates photo printing workflows for makers and small business owners. Users configure a watch folder, select their printer and settings, and the app automatically detects new zip files, extracts images, and sends them to the printer with the correct preset configuration.

## Tech Stack

- **Framework:** Tauri v2
- **Frontend:** React 19 + TypeScript, Vite 7
- **Backend:** Rust
- **Styling:** Tailwind CSS + shadcn/ui
- **Platforms:** Windows (primary), macOS (secondary)

## Prerequisites

Install the required toolchain via [mise](https://mise.jdx.dev/) (`mise install`) or manually:

- Node.js 26.3.0
- Rust 1.99.1
- pnpm 11.5.3

### Building on Linux / WSL

Linux is not a shipping target (releases are Windows and macOS), but the build
links against CUPS via the `printers` crate, so a Linux/WSL build needs the CUPS
dev libraries. Install them once with:

```bash
mise run setup-linux   # runs: sudo apt-get install -y libcups2-dev
```

Without this you'll hit `rust-lld: error: unable to find library -lcups` during
`pnpm tauri build`. (Debian/Ubuntu only; adjust the package for other distros.)

## Getting Started

```bash
# Install dependencies
pnpm install

# Run the full Tauri app in dev mode
pnpm tauri dev

# Build distributable app (Windows .exe / macOS .dmg)
pnpm tauri build
```

## Recommended IDE Setup

- [VS Code](https://code.visualstudio.com/) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

## Releasing

Releases are **tag-driven**: pushing a `v*` tag triggers the
[release workflow](.github/workflows/release.yml), which builds and publishes a
GitHub Release with installers for Windows and macOS (Apple Silicon + Intel)
plus the auto-updater manifest.

### Cut a release

1. Make sure `main` is green in CI and you're up to date (`git pull`).
2. Bump the version. This updates `package.json` and `src-tauri/Cargo.toml`,
   commits as `vX.Y.Z`, and creates a matching git tag
   (`src-tauri/tauri.conf.json` reads its version from `package.json`):

   ```bash
   pnpm bump patch        # 0.1.11 → 0.1.12
   pnpm bump minor        # 0.1.11 → 0.2.0
   pnpm bump major        # 0.1.11 → 1.0.0
   pnpm bump 1.2.3         # set an explicit version
   ```

3. Push the commit and the tag:

   ```bash
   git push && git push origin vX.Y.Z
   ```

4. The tag push starts the release workflow. It builds all three targets,
   generates release notes from the commit log since the previous tag, signs
   the updater artifacts, and publishes the GitHub Release. Watch it with
   `gh run watch` or in the Actions tab.

### What gets published

- `Hello Print_X.Y.Z_x64-setup.exe` — Windows installer (NSIS)
- `Hello Print_X.Y.Z_aarch64.dmg` — macOS Apple Silicon (M-series)
- `Hello Print_X.Y.Z_x64.dmg` — macOS Intel
- `latest.json` + `.sig` files — consumed by the in-app auto-updater

### Updater signing

The auto-updater requires artifacts to be signed with the key whose public half
is baked into `tauri.conf.json`.

- **CI:** the private key is provided via the `TAURI_SIGNING_PRIVATE_KEY`
  repository secret.
- **Local signed builds:** `mise.toml` points `TAURI_SIGNING_PRIVATE_KEY` at
  `.keys/hello-print.key` (gitignored). Run `mise trust` once, then
  `pnpm tauri build` produces signed artifacts. Use `pnpm tauri build --no-bundle`
  for an unsigned compile-only check.

### macOS note

Builds are **not** code-signed/notarized (no Apple Developer account). On first
launch macOS users must clear the quarantine flag:
`xattr -cr "/Applications/Hello Print.app"`. Tracked in issue #13.
