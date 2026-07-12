#!/usr/bin/env bash
#
# Install the `flex` CLI and the macOS desktop app from a local checkout.
#
# Usage (from repo root):
#   ./scripts/install_mac.sh                              # CLI + desktop
#   ./scripts/install_mac.sh --cli-only                   # runner only
#   ./scripts/install_mac.sh --desktop-only               # Flex.app only
#   FLEX_BIN_DIR=/usr/local/bin ./scripts/install_mac.sh  # custom CLI dir
#   FLEX_APP_DIR="$HOME/Applications" ./scripts/install_mac.sh
#
# Defaults:
#   CLI  → ~/.local/bin/flex
#   App  → /Applications/Flex.app
#
# Windows: see ./scripts/install_windows.ps1 (native) or ./scripts/install_windows.sh
#
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk_dir="$repo_root/packages/sdk"
desktop_dir="$repo_root/packages/desktop"
bin_name="flex"
app_name="Flex.app"
dest_dir="${FLEX_BIN_DIR:-$HOME/.local/bin}"
app_dest_dir="${FLEX_APP_DIR:-/Applications}"

install_cli=1
install_desktop=1
for arg in "$@"; do
  case "$arg" in
    --cli-only) install_desktop=0 ;;
    --desktop-only) install_cli=0 ;;
    -h|--help)
      sed -n '2,16p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "error: unknown argument: $arg (try --cli-only / --desktop-only)" >&2
      exit 1
      ;;
  esac
done

if [ "$(uname -s)" != "Darwin" ] && [ "$install_desktop" -eq 1 ]; then
  echo "note: desktop install is macOS-only; skipping Flex.app on $(uname -s)."
  install_desktop=0
  if [ "$install_cli" -eq 0 ]; then
    echo "error: nothing to install (--desktop-only on non-macOS)." >&2
    exit 1
  fi
fi

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: '$1' not found — $2" >&2
    exit 1
  fi
}

install_flex_cli() {
  require_cmd cargo "install Rust from https://rustup.rs and retry."

  echo "==> Building ${bin_name} (release)…"
  ( cd "$sdk_dir" && cargo build --release )

  built="$sdk_dir/target/release/${bin_name}"
  if [ ! -x "$built" ]; then
    echo "error: build did not produce ${built}" >&2
    exit 1
  fi

  echo "==> Installing CLI to ${dest_dir}/${bin_name}"
  mkdir -p "$dest_dir"
  install -m 0755 "$built" "${dest_dir}/${bin_name}"
  echo "==> Installed ${dest_dir}/${bin_name}"

  case ":$PATH:" in
    *":${dest_dir}:"*)
      echo "==> CLI ready. Run '${bin_name}' inside any project folder."
      ;;
    *)
      echo
      echo "NOTE: ${dest_dir} is not on your PATH. Add this to ~/.zshrc or ~/.bashrc,"
      echo "      then restart your shell:"
      echo
      echo "    export PATH=\"${dest_dir}:\$PATH\""
      echo
      ;;
  esac
}

install_flex_desktop() {
  require_cmd pnpm "install from https://pnpm.io and retry."
  require_cmd cargo "install Rust from https://rustup.rs and retry."

  echo "==> Installing desktop JS deps…"
  ( cd "$desktop_dir" && pnpm install )

  echo "==> Building ${app_name} (release)…"
  ( cd "$desktop_dir" && pnpm exec tauri build --bundles app )

  built_app="$desktop_dir/src-tauri/target/release/bundle/macos/${app_name}"
  if [ ! -d "$built_app" ]; then
    echo "error: build did not produce ${built_app}" >&2
    exit 1
  fi

  dest_app="${app_dest_dir}/${app_name}"
  echo "==> Installing desktop to ${dest_app}"
  mkdir -p "$app_dest_dir"
  rm -rf "$dest_app"
  cp -R "$built_app" "$dest_app"
  echo "==> Installed ${dest_app}"
  echo "==> Desktop ready. Open with: open \"${dest_app}\""
}

if [ "$install_cli" -eq 1 ]; then
  install_flex_cli
fi
if [ "$install_desktop" -eq 1 ]; then
  install_flex_desktop
fi

echo "==> Done."
