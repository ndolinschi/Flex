#!/usr/bin/env bash
#
# Install the `flex` CLI and the Windows desktop app (Git Bash / MSYS2).
# For native PowerShell prefer: .\scripts\install_windows.ps1
#
# Usage (from repo root, in Git Bash / MSYS2):
#   ./scripts/install_windows.sh
#   ./scripts/install_windows.sh --cli-only
#   ./scripts/install_windows.sh --desktop-only
#   FLEX_BIN_DIR="$HOME/bin" ./scripts/install_windows.sh
#   FLEX_APP_DIR="$LOCALAPPDATA/Programs/Flex" ./scripts/install_windows.sh
#
# Defaults:
#   CLI  → %USERPROFILE%\.local\bin\flex.exe
#   App  → %LOCALAPPDATA%\Programs\Flex\Flex.exe
#
# WSL: installs the Linux CLI only (desktop needs native Windows / PowerShell).
# macOS: see ./scripts/install_mac.sh
#
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sdk_dir="$repo_root/packages/sdk"
desktop_dir="$repo_root/packages/desktop"
bin_name="flex"
app_exe_name="Flex.exe"
dest_dir="${FLEX_BIN_DIR:-$HOME/.local/bin}"

# Prefer LOCALAPPDATA when present (Git Bash exports it on native Windows).
if [ -n "${FLEX_APP_DIR:-}" ]; then
  app_dest_dir="$FLEX_APP_DIR"
elif [ -n "${LOCALAPPDATA:-}" ]; then
  app_dest_dir="${LOCALAPPDATA}/Programs/Flex"
else
  app_dest_dir="${HOME}/AppData/Local/Programs/Flex"
fi

install_cli=1
install_desktop=1
for arg in "$@"; do
  case "$arg" in
    --cli-only) install_desktop=0 ;;
    --desktop-only) install_cli=0 ;;
    -h|--help)
      sed -n '2,18p' "$0" | sed 's/^# \?//'
      exit 0
      ;;
    *)
      echo "error: unknown argument: $arg (try --cli-only / --desktop-only)" >&2
      exit 1
      ;;
  esac
done

uname_s="$(uname -s 2>/dev/null || echo unknown)"
case "$uname_s" in
  MINGW*|MSYS*|CYGWIN*)
    native_windows=1
    ;;
  *)
    native_windows=0
    ;;
esac

if [ "$native_windows" -eq 0 ] && [ "$install_desktop" -eq 1 ]; then
  echo "note: desktop install needs native Windows (Git Bash/MSYS or PowerShell)."
  echo "      Skipping desktop on ${uname_s}. Use .\\scripts\\install_windows.ps1 on Windows."
  install_desktop=0
  if [ "$install_cli" -eq 0 ]; then
    echo "error: nothing to install (--desktop-only outside native Windows)." >&2
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

  case "$uname_s" in
    MINGW*|MSYS*|CYGWIN*)
      dest_name="${bin_name}.exe"
      ;;
    *)
      dest_name="$bin_name"
      ;;
  esac

  dest_path="${dest_dir}/${dest_name}"
  echo "==> Installing CLI to ${dest_path}"
  mkdir -p "$dest_dir"
  cp "$built" "$dest_path"
  chmod 0755 "$dest_path" 2>/dev/null || true
  echo "==> Installed ${dest_path}"

  path_has_dest=0
  case ":${PATH}:" in
    *":${dest_dir}:"*) path_has_dest=1 ;;
  esac
  win_dest="$(cd "$dest_dir" 2>/dev/null && pwd -W 2>/dev/null || true)"
  if [ -n "${win_dest:-}" ]; then
    case ";${PATH};:${PATH}:" in
      *"${win_dest}"*|*"${dest_dir}"*) path_has_dest=1 ;;
    esac
  fi

  if [ "$path_has_dest" -eq 1 ]; then
    echo "==> CLI ready. Run '${bin_name}' inside any project folder."
  else
    echo
    echo "NOTE: ${dest_dir} is not on your PATH."
    echo
    echo "  Git Bash — add to ~/.bashrc, then restart the shell:"
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
  fi
}

install_flex_desktop() {
  require_cmd pnpm "install from https://pnpm.io and retry."
  require_cmd cargo "install Rust from https://rustup.rs and retry."

  echo "==> Installing desktop JS deps…"
  ( cd "$desktop_dir" && pnpm install )

  echo "==> Building desktop (release, nsis)…"
  ( cd "$desktop_dir" && pnpm exec tauri build --bundles nsis )

  release_dir="$desktop_dir/src-tauri/target/release"
  built_exe="${release_dir}/${app_exe_name}"
  if [ ! -f "$built_exe" ]; then
    if [ -f "${release_dir}/desktop.exe" ]; then
      built_exe="${release_dir}/desktop.exe"
    else
      echo "error: build did not produce ${release_dir}/${app_exe_name}" >&2
      exit 1
    fi
  fi

  dest_exe="${app_dest_dir}/${app_exe_name}"
  echo "==> Installing desktop to ${dest_exe}"
  mkdir -p "$app_dest_dir"
  cp "$built_exe" "$dest_exe"
  if [ -d "${release_dir}/resources" ]; then
    rm -rf "${app_dest_dir}/resources"
    cp -R "${release_dir}/resources" "${app_dest_dir}/resources"
  fi
  echo "==> Installed ${dest_exe}"
  echo "==> Desktop ready. Launch: \"${dest_exe}\""
}

if [ "$install_cli" -eq 1 ]; then
  install_flex_cli
fi
if [ "$install_desktop" -eq 1 ]; then
  install_flex_desktop
fi

echo "==> Done."
