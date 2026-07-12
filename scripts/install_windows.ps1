#Requires -Version 5.1
<#
.SYNOPSIS
  Install the flex CLI and the Windows desktop app from a local checkout.

.DESCRIPTION
  Release-builds the runner from packages/sdk and the Tauri desktop app from
  packages/desktop, then installs both for the current user.

.PARAMETER CliOnly
  Install only the flex.exe runner.

.PARAMETER DesktopOnly
  Install only the desktop app.

.EXAMPLE
  .\scripts\install_windows.ps1

.EXAMPLE
  $env:FLEX_BIN_DIR = "$env:USERPROFILE\bin"
  .\scripts\install_windows.ps1

.EXAMPLE
  .\scripts\install_windows.ps1 -CliOnly

.NOTES
  Defaults:
    CLI  -> %USERPROFILE%\.local\bin\flex.exe
    App  -> %LOCALAPPDATA%\Programs\Flex\Flex.exe
  Override with FLEX_BIN_DIR / FLEX_APP_DIR.
  macOS: see .\scripts\install_mac.sh
#>
[CmdletBinding()]
param(
  [switch]$CliOnly,
  [switch]$DesktopOnly
)

$ErrorActionPreference = "Stop"

if ($CliOnly -and $DesktopOnly) {
  throw "Use only one of -CliOnly / -DesktopOnly."
}

$InstallCli = -not $DesktopOnly
$InstallDesktop = -not $CliOnly

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot "..")
$SdkDir = Join-Path $RepoRoot "packages\sdk"
$DesktopDir = Join-Path $RepoRoot "packages\desktop"
$BinName = "flex"
$AppExeName = "Flex.exe"

$DestDir = if ($env:FLEX_BIN_DIR) { $env:FLEX_BIN_DIR } else {
  Join-Path $env:USERPROFILE ".local\bin"
}
$AppDestDir = if ($env:FLEX_APP_DIR) { $env:FLEX_APP_DIR } else {
  Join-Path $env:LOCALAPPDATA "Programs\Flex"
}

function Test-FlexCommand {
  param([string]$Name)
  return [bool](Get-Command $Name -ErrorAction SilentlyContinue)
}

function Assert-FlexCommand {
  param(
    [string]$Name,
    [string]$Hint
  )
  if (-not (Test-FlexCommand $Name)) {
    throw "error: '$Name' not found - $Hint"
  }
}

function Test-UserPathEntry {
  param([string]$Dir)
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (-not $userPath) { $userPath = "" }
  $parts = $userPath -split ";" | Where-Object { $_ -ne "" }
  $normalized = $Dir.TrimEnd("\")
  foreach ($part in $parts) {
    if ($part.TrimEnd("\") -ieq $normalized) {
      return $true
    }
  }
  return $false
}

function Add-UserPathEntry {
  param([string]$Dir)
  $userPath = [Environment]::GetEnvironmentVariable("Path", "User")
  if (-not $userPath) { $userPath = "" }
  $newPath = if ($userPath.Trim() -eq "") { $Dir } else { "$userPath;$Dir" }
  [Environment]::SetEnvironmentVariable("Path", $newPath, "User")
  $env:Path = "$Dir;$env:Path"
}

function Install-FlexCli {
  Assert-FlexCommand "cargo" "install Rust from https://rustup.rs and retry."

  Write-Host "==> Building $BinName (release)..."
  Push-Location $SdkDir
  try {
    & cargo build --release
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed ($LASTEXITCODE)" }
  } finally {
    Pop-Location
  }

  $built = Join-Path $SdkDir "target\release\${BinName}.exe"
  if (-not (Test-Path -LiteralPath $built)) {
    throw "error: build did not produce $built"
  }

  $destPath = Join-Path $DestDir "${BinName}.exe"
  Write-Host "==> Installing CLI to $destPath"
  New-Item -ItemType Directory -Force -Path $DestDir | Out-Null
  Copy-Item -LiteralPath $built -Destination $destPath -Force
  Write-Host "==> Installed $destPath"

  if (Test-UserPathEntry $DestDir) {
    Write-Host "==> CLI ready. Run '$BinName' inside any project folder."
  } else {
    Write-Host ""
    Write-Host "NOTE: $DestDir is not on your User PATH. Adding it now..."
    Add-UserPathEntry $DestDir
    Write-Host "==> Added to User PATH. Open a new terminal, then run '$BinName'."
  }
}

function New-StartMenuShortcut {
  param(
    [string]$TargetPath,
    [string]$ShortcutName
  )
  $programs = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
  New-Item -ItemType Directory -Force -Path $programs | Out-Null
  $lnk = Join-Path $programs $ShortcutName
  $shell = New-Object -ComObject WScript.Shell
  $shortcut = $shell.CreateShortcut($lnk)
  $shortcut.TargetPath = $TargetPath
  $shortcut.WorkingDirectory = Split-Path -Parent $TargetPath
  $shortcut.Description = "Flex desktop"
  $shortcut.Save()
  Write-Host "==> Start Menu shortcut: $lnk"
}

function Install-FlexDesktop {
  Assert-FlexCommand "pnpm" "install from https://pnpm.io and retry."
  Assert-FlexCommand "cargo" "install Rust from https://rustup.rs and retry."

  Write-Host "==> Installing desktop JS deps..."
  Push-Location $DesktopDir
  try {
    & pnpm install
    if ($LASTEXITCODE -ne 0) { throw "pnpm install failed ($LASTEXITCODE)" }

    Write-Host "==> Building desktop (release, nsis + exe)..."
    & pnpm exec tauri build --bundles nsis
    if ($LASTEXITCODE -ne 0) { throw "tauri build failed ($LASTEXITCODE)" }
  } finally {
    Pop-Location
  }

  $releaseDir = Join-Path $DesktopDir "src-tauri\target\release"
  $builtExe = Join-Path $releaseDir $AppExeName
  if (-not (Test-Path -LiteralPath $builtExe)) {
    # Fallback: package name may emit desktop.exe depending on crate config.
    $alt = Join-Path $releaseDir "desktop.exe"
    if (Test-Path -LiteralPath $alt) {
      $builtExe = $alt
    } else {
      throw "error: build did not produce $AppExeName under $releaseDir"
    }
  }

  $destExe = Join-Path $AppDestDir $AppExeName
  Write-Host "==> Installing desktop to $destExe"
  New-Item -ItemType Directory -Force -Path $AppDestDir | Out-Null
  Copy-Item -LiteralPath $builtExe -Destination $destExe -Force

  # Copy sidecar resources if Tauri emitted them next to the exe.
  $resources = Join-Path $releaseDir "resources"
  if (Test-Path -LiteralPath $resources) {
    $destResources = Join-Path $AppDestDir "resources"
    if (Test-Path -LiteralPath $destResources) {
      Remove-Item -LiteralPath $destResources -Recurse -Force
    }
    Copy-Item -LiteralPath $resources -Destination $destResources -Recurse -Force
  }

  New-StartMenuShortcut -TargetPath $destExe -ShortcutName "Flex.lnk"
  Write-Host "==> Installed $destExe"
  Write-Host "==> Desktop ready. Launch from Start Menu or run: $destExe"

  # Prefer silent NSIS when the installer exists (registers uninstaller, etc.).
  $nsisDir = Join-Path $releaseDir "bundle\nsis"
  if (Test-Path -LiteralPath $nsisDir) {
    $setup = Get-ChildItem -LiteralPath $nsisDir -Filter "*-setup.exe" -File |
      Select-Object -First 1
    if ($setup) {
      Write-Host "==> Optional: silent NSIS installer also available at $($setup.FullName)"
      Write-Host "    Run with: Start-Process -FilePath '$($setup.FullName)' -ArgumentList '/S' -Wait"
    }
  }
}

if ($InstallCli) {
  Install-FlexCli
}
if ($InstallDesktop) {
  Install-FlexDesktop
}

Write-Host "==> Done."
