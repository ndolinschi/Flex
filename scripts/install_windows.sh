#!/usr/bin/env bash
#
# Install the `flex` CLI globally on Windows (Git Bash, MSYS2, or WSL).
# Same job as scripts/install_mac.sh: release-build from packages/sdk and
# copy into a user bin dir so `flex` works from any project folder.
#
# Usage (from repo root, in Git Bash / WSL / MSYS2):
#   ./scripts/install_windows.sh
#   FLEX_BIN_DIR="$HOME/bin" ./scripts/install_windows.sh
#
# Default install dir: %USERPROFILE%\.local\bin  (i.e. $HOME/.local/bin in bash)
# Override with FLEX_BIN_DIR.
#
# macOS / Linux: see ./scripts/install_mac.sh
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

# Native Windows rustc emits flex.exe; WSL/Linux cross-ish hosts emit flex.
built=""
for candidate in \
  "$sdk_dir/target/release/${bin_name}.exe" \
  "$sdk_dir/target/release/${bin_name}"
do
  if [ -f "$candidate" ]; then
    built="$candidate"
    break
  fi
done

if [ -z "$built" ]; then
  echo "error: build did not produce ${sdk_dir}/target/release/${bin_name}[.exe]" >&2
  exit 1
fi

# Keep the .exe suffix on native Windows so cmd.exe / PowerShell can find it;
# drop it under WSL/Linux where the executable bit is enough.
case "$(uname -s 2>/dev/null || echo unknown)" in
  MINGW*|MSYS*|CYGWIN*)
    dest_name="${bin_name}.exe"
    ;;
  *)
    dest_name="$bin_name"
    ;;
esac

dest_path="${dest_dir}/${dest_name}"
echo "==> Installing to ${dest_path}"
mkdir -p "$dest_dir"

# `install` is not always available in Git Bash; cp + chmod is portable.
cp "$built" "$dest_path"
chmod 0755 "$dest_path" 2>/dev/null || true

echo "==> Installed ${dest_path}"

# Normalize PATH check for Git Bash (Windows paths often use ; or mixed separators).
path_has_dest=0
case ":${PATH}:" in
  *":${dest_dir}:"*) path_has_dest=1 ;;
esac
# Also accept the Windows-style form when running under MSYS.
win_dest="$(cd "$dest_dir" 2>/dev/null && pwd -W 2>/dev/null || true)"
if [ -n "${win_dest:-}" ]; then
  case ";${PATH};:${PATH}:" in
    *"${win_dest}"*|*"${dest_dir}"*) path_has_dest=1 ;;
  esac
fi

if [ "$path_has_dest" -eq 1 ]; then
  echo "==> Done. Run '${bin_name}' inside any project folder."
else
  echo
  echo "NOTE: ${dest_dir} is not on your PATH."
  echo
  echo "  Git Bash / WSL — add to ~/.bashrc (or ~/.bash_profile), then restart the shell:"
  echo
  echo "    export PATH=\"${dest_dir}:\$PATH\""
  echo
  echo "  Native Windows (PowerShell as current user) — then open a new terminal:"
  echo
  echo "    \$dir = Join-Path \$env:USERPROFILE '.local\\bin'"
  echo "    [Environment]::SetEnvironmentVariable("
  echo "      'Path',"
  echo "      \$env:Path + ';' + \$dir,"
  echo "      'User')"
  echo
  echo "Then run '${bin_name}' inside any project folder."
fi
