#!/usr/bin/env bash
#
# Install the `flex` CLI globally so you can launch it inside any
# project folder — it uses the current directory as its workdir by default,
# so `cd my-project && flex` just works.
#
# Usage:
#   ./scripts/install_mac.sh                              # build + install to ~/.local/bin
#   FLEX_BIN_DIR=/usr/local/bin ./scripts/install_mac.sh  # custom dir (may need sudo)
#
# Windows (Git Bash / WSL): see ./scripts/install_windows.sh
#
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk_dir="$repo_root/packages/sdk"
bin_name="flex"
dest_dir="${FLEX_BIN_DIR:-$HOME/.local/bin}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: 'cargo' not found — install Rust from https://rustup.rs and retry." >&2
  exit 1
fi

echo "==> Building ${bin_name} (release)…"
( cd "$sdk_dir" && cargo build --release )

built="$sdk_dir/target/release/${bin_name}"
if [ ! -x "$built" ]; then
  echo "error: build did not produce ${built}" >&2
  exit 1
fi

echo "==> Installing to ${dest_dir}/${bin_name}"
mkdir -p "$dest_dir"
install -m 0755 "$built" "${dest_dir}/${bin_name}"

echo "==> Installed ${dest_dir}/${bin_name}"
case ":$PATH:" in
  *":${dest_dir}:"*)
    echo "==> Done. Run '${bin_name}' inside any project folder."
    ;;
  *)
    echo
    echo "NOTE: ${dest_dir} is not on your PATH. Add this to your shell profile"
    echo "      (~/.zshrc or ~/.bashrc), then restart your shell:"
    echo
    echo "    export PATH=\"${dest_dir}:\$PATH\""
    echo
    echo "Then run '${bin_name}' inside any project folder."
    ;;
esac
