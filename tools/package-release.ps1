# Builds a player-ready Windows release of CSGO Rewrite:
#   dist\CSGO-Rewrite\                        the game folder (exe, launch scripts)
#   dist\CSGO-Rewrite-<version>-windows-x64.zip
#
# The release is a fresh install with no player data. On first launch the
# game finds the player's CS:GO Legacy install (CS:GO files are never shipped).
# CI runs this on every version tag (.github/workflows/release.yml).
#
# Usage (from the repository root):
#   powershell -ExecutionPolicy Bypass -File tools\package-release.ps1 [-SkipBuild]

param(
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$name = 'CSGO-Rewrite'
$dist = Join-Path $root 'dist'
$out = Join-Path $dist $name
$zip = Join-Path $dist "$name-$version-windows-x64.zip"

# --- Build -----------------------------------------------------------------
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
# crt-static changes every crate's flags, so keep it apart from normal builds.
$targetDir = if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR 'dist' } else { Join-Path $root 'target\dist' }
if (-not $SkipBuild) {
    Write-Host "Building release $version (static C runtime, thin LTO). This takes a while..."
    $env:RUSTFLAGS = '-C target-feature=+crt-static'
    & $cargo build --release --locked -p launcher --target-dir $targetDir
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    Remove-Item Env:RUSTFLAGS
}
$exe = Join-Path $targetDir 'release\csr.exe'
if (-not (Test-Path $exe)) { throw "Missing $exe (run without -SkipBuild)" }

# --- Lay out the game folder -----------------------------------------------
if (Test-Path $out) { Remove-Item $out -Recurse -Force }
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item $exe (Join-Path $out "$name.exe")

# Marks a release: player data goes to userdata\ next to the exe.
Set-Content -Encoding ascii (Join-Path $out 'csgo-rewrite.portable') @'
This file marks a release copy of CSGO Rewrite. Player data (settings, skins,
cases and the CS:GO folder choice) is kept in the userdata folder next to the
game. Delete userdata to start over as a fresh install.
'@

function Write-Bat($file, $body) {
    Set-Content -Encoding ascii (Join-Path $out $file) ("@echo off`r`ncd /d `"%~dp0`"`r`n" + $body)
}
Write-Bat 'Play CSGO.bat' @"
"$name.exe" play de_dust2
if errorlevel 1 pause
"@
Write-Bat 'Change CSGO Folder.bat' @"
"$name.exe" setup
pause
"@

Copy-Item 'docs\PLAYERS.md' (Join-Path $out 'README.txt')
Copy-Item 'LICENSE' (Join-Path $out 'LICENSE.txt') -ErrorAction SilentlyContinue

# Nothing personal goes out.
foreach ($p in 'userdata', 'config', 'logs') {
    if (Test-Path (Join-Path $out $p)) { throw "Release unexpectedly contains $p" }
}

# --- Zip -------------------------------------------------------------------
if (Test-Path $zip) { Remove-Item $zip -Force }
Write-Host 'Zipping...'
$tar = Join-Path $env:SystemRoot 'System32\tar.exe'
& $tar -a -c -f $zip -C $dist $name
if ($LASTEXITCODE -ne 0) { throw 'Creating the zip failed' }
$size = [math]::Round((Get-Item $zip).Length / 1MB)
Write-Host ""
Write-Host "Release ready: $zip ($size MB)"
Write-Host "Folder:        $out"
