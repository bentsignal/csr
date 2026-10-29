# Builds a player-ready Windows release of CSGO Rewrite:
#   dist\CSGO-Rewrite\                         the unzipped game folder (portable)
#   dist\CSGO-Rewrite-<version>-windows-x64.zip
#   dist\velopack\                             the installer and update feed:
#       CSGORewrite-win-Setup.exe              what players download and run
#       CSGORewrite-<version>-full.nupkg       full update package
#       CSGORewrite-<version>-delta.nupkg      delta from the previous release
#       releases.win.json, RELEASES            the update feed
#
# The release is a fresh install with no player data. On first launch the
# game finds the player's CS:GO Legacy install (CS:GO files are never shipped).
# CI runs this on every version tag (.github/workflows/release.yml) and
# publishes everything in dist\velopack plus the zip to the GitHub Release.
#
# Usage (from the repository root):
#   powershell -ExecutionPolicy Bypass -File tools\package-release.ps1
#     [-SkipBuild]          reuse the last build
#     [-Version 1.2.3]      package as this version (default: Cargo.toml)
#     [-NoInstaller]        only the zip (no Velopack installer)
#     [-DeltaFromGitHub]    fetch the latest published release first, so the
#                           new release includes a delta update from it
#     [-Releases <dir>]     installer output folder (default dist\velopack);
#                           packages already there are used for deltas

param(
    [switch]$SkipBuild,
    [string]$Version,
    [switch]$NoInstaller,
    [switch]$DeltaFromGitHub,
    [switch]$Prerelease,
    [string]$ReleaseNotes,
    [string]$Releases
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

if (-not $Version) {
    $Version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
}
$name = 'CSGO-Rewrite'
$packId = 'CSGORewrite'        # Velopack app id: never change it, or installed copies stop updating
$vpkVersion = '1.2.161'        # keep in step with the velopack crate in crates/launcher/Cargo.toml
$repoUrl = 'https://github.com/bentsignal/csr'
$dist = Join-Path $root 'dist'
$out = Join-Path $dist $name
$zip = Join-Path $dist "$name-$Version-windows-x64.zip"
if (-not $Releases) { $Releases = Join-Path $dist 'velopack' }

# --- Build -----------------------------------------------------------------
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
# crt-static changes every crate's flags, so keep it apart from normal builds.
$targetDir = if ($env:CARGO_TARGET_DIR) { Join-Path $env:CARGO_TARGET_DIR 'dist' } else { Join-Path $root 'target\dist' }
if (-not $SkipBuild) {
    Write-Host "Building release $Version (static C runtime, thin LTO). This takes a while..."
    $env:RUSTFLAGS = '-C target-feature=+crt-static'
    & $cargo build --release --locked -p launcher --target-dir $targetDir
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
    Remove-Item Env:RUSTFLAGS
}
$exe = Join-Path $targetDir 'release\csr.exe'
if (-not (Test-Path $exe)) { throw "Missing $exe (run without -SkipBuild)" }

# --- Lay out the game folder -----------------------------------------------
function Remove-PackageFolder([string]$Folder) {
    $absolute = [IO.Path]::GetFullPath($Folder)
    $distRoot = [IO.Path]::GetFullPath($dist).TrimEnd('\') + '\'
    if (-not $absolute.StartsWith($distRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw "Package cleanup escapes dist: $absolute"
    }
    foreach ($candidate in @($dist, $absolute)) {
        if ((Test-Path -LiteralPath $candidate) -and ((Get-Item -LiteralPath $candidate -Force).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            throw "Package cleanup refuses a linked folder: $candidate"
        }
    }
    foreach ($saved in @('userdata', 'config', 'logs')) {
        if (Test-Path -LiteralPath (Join-Path $absolute $saved)) { throw "Preserving player data in $absolute; move this played copy before packaging." }
    }
    if (Test-Path -LiteralPath $absolute) { Remove-Item -LiteralPath $absolute -Recurse -Force }
}
Remove-PackageFolder $out
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item $exe (Join-Path $out "$name.exe")
Copy-Item 'docs\PLAYERS.md' (Join-Path $out 'README.txt')
Copy-Item 'LICENSE' (Join-Path $out 'LICENSE.txt') -ErrorAction SilentlyContinue
# Optional provider and developer tools are separate release downloads. None
# enter the base installer or update package. Packs request a matching runtime.
$optional = Join-Path $dist 'optional-skate'
Remove-PackageFolder $optional
& (Join-Path $root 'tools/build-skate-tools.ps1') -OutputDir $optional
if ($LASTEXITCODE -ne 0) { throw 'Optional content tools build failed' }
$runtimeZip = Join-Path $dist 'Skate-Runtime-windows-x64.zip'
$authoringZip = Join-Path $dist 'Skate-Authoring-windows-x64.zip'
Compress-Archive -LiteralPath @((Join-Path $optional 'Skate-Runtime.exe'),(Join-Path $optional 'Skate-GPL-3.0.txt'),(Join-Path $optional 'Skate-NOTICE.txt'),(Join-Path $optional 'Skate-SOURCE.txt')) -DestinationPath $runtimeZip -Force
Copy-Item -LiteralPath 'docs/SKATE.md' -Destination (Join-Path $optional 'README.txt')
Compress-Archive -Path (Join-Path $optional '*') -DestinationPath $authoringZip -Force
if (-not $ReleaseNotes) { $ReleaseNotes = Join-Path $root 'docs\LAUNCHER-RELEASE-NOTES.md' }
if (-not (Test-Path -LiteralPath $ReleaseNotes -PathType Leaf)) { throw 'Release notes file is missing' }
Copy-Item -LiteralPath $ReleaseNotes -Destination (Join-Path $out 'RELEASE-NOTES.md')

# The installed app gets the same files, minus the portable marker and the
# .bat launchers (it has Start menu and desktop shortcuts instead).
$appDir = Join-Path $dist "$name-app"
Remove-PackageFolder $appDir
Copy-Item $out $appDir -Recurse

# Marks the zip copy as portable: player data goes to userdata\ next to the exe.
Set-Content -Encoding ascii (Join-Path $out 'csgo-rewrite.portable') @'
This file marks a portable copy of CSGO Rewrite. Player data (settings, skins,
cases and the CS:GO folder choice) is kept in the userdata folder next to the
game. Delete userdata to start over as a fresh install.
'@

function Write-Bat($file, $body) {
    Set-Content -Encoding ascii (Join-Path $out $file) ("@echo off`r`ncd /d `"%~dp0`"`r`n" + $body)
}
Write-Bat 'Play CSGO.bat' @"
"$name.exe" launcher
if errorlevel 1 pause
"@
Write-Bat 'Change CSGO Folder.bat' @"
"$name.exe" setup
pause
"@

# Nothing personal goes out.
foreach ($dir in $out, $appDir) {
    foreach ($p in 'userdata', 'config', 'logs') {
        if (Test-Path (Join-Path $dir $p)) { throw "Release unexpectedly contains $p" }
    }
}

# --- Zip -------------------------------------------------------------------
if (Test-Path $zip) { Remove-Item $zip -Force }
Write-Host 'Zipping...'
$tar = Join-Path $env:SystemRoot 'System32\tar.exe'
& $tar -a -c -f $zip -C $dist $name
if ($LASTEXITCODE -ne 0) { throw 'Creating the zip failed' }
$size = [math]::Round((Get-Item $zip).Length / 1MB)
Write-Host ""
Write-Host "Zip ready:       $zip ($size MB)"

if ($NoInstaller) { return }

# --- Installer and update packages (Velopack) -------------------------------
$vpk = (Get-Command vpk -ErrorAction SilentlyContinue).Source
if (-not $vpk) { $vpk = Join-Path $env:USERPROFILE '.dotnet\tools\vpk.exe' }
if (-not (Test-Path $vpk)) {
    Write-Host "Installing the Velopack packer (vpk $vpkVersion, a .NET tool)..."
    & dotnet tool install -g vpk --version $vpkVersion
    if ($LASTEXITCODE -ne 0) { throw 'Could not install vpk. Install the .NET 8 SDK or newer, or pass -NoInstaller.' }
}

# Our own artwork, drawn by the game itself (no image files in the repo).
$art = Join-Path $dist 'art'
& (Join-Path $out "$name.exe") art $art
if ($LASTEXITCODE -ne 0) { throw 'Drawing the artwork failed' }

New-Item -ItemType Directory -Force $Releases | Out-Null
if ($DeltaFromGitHub) {
    # The previous full package lets vpk build a small delta update.
    $tokenArgs = @()
    if ($env:GH_TOKEN) { $tokenArgs = @('--token', $env:GH_TOKEN) }
    if ($Prerelease) { $tokenArgs += '--pre' }
    & $vpk download github --repoUrl $repoUrl -o $Releases @tokenArgs
    if ($LASTEXITCODE -ne 0) { Write-Host 'No earlier installer release found; this one has no delta.' }
}

Write-Host "Packing installer $Version..."
& $vpk pack --packId $packId --packVersion $Version --packDir $appDir --mainExe "$name.exe" `
    --packTitle 'CSGO Rewrite' --packAuthors 'CSGO Rewrite contributors' `
    --icon (Join-Path $art 'icon.ico') --splashImage (Join-Path $art 'splash.png') `
    --releaseNotes $ReleaseNotes `
    --runtime win-x64 --shortcuts 'Desktop,StartMenuRoot' --noPortable --outputDir $Releases
if ($LASTEXITCODE -ne 0) { throw 'vpk pack failed' }

$setup = Join-Path $Releases "$packId-win-Setup.exe"
if (-not (Test-Path $setup)) { throw "Missing $setup" }
$size = [math]::Round((Get-Item $setup).Length / 1MB)
Write-Host "Installer ready: $setup ($size MB)"
