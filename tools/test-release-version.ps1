# Actual Cargo metadata and command-exit regressions; no target compilation,
# game install, packaging, registry writes or release publication.
$ErrorActionPreference = 'Stop'
$guard = Join-Path $PSScriptRoot 'verify-release-version.ps1'
$manifest = Join-Path (Split-Path -Parent $PSScriptRoot) 'Cargo.toml'
$actual = & $guard -ManifestPath $manifest
$checks = 0
function Check-Guard([string]$Manifest, [string]$Tag, [bool]$Accepted, [string]$Expected) {
    $ErrorActionPreference = 'Continue' # Capture expected native stderr as data.
    $output = & powershell -NoProfile -File $guard -ManifestPath $Manifest -Tag $Tag 2>&1
    $code = $LASTEXITCODE
    $text = ($output | ForEach-Object { $_.ToString() }) -join "`n"
    if (($code -eq 0) -ne $Accepted -or -not $text.Contains($Expected)) {
        throw "Unexpected guard result for '$Tag': exit $code; $text"
    }
    $script:checks++
}
Check-Guard $manifest "v$actual" $true $actual
Check-Guard $manifest "v$actual-wrong" $false 'does not match Cargo workspace version'
Check-Guard $manifest "V$actual" $false 'does not match Cargo workspace version'
Check-Guard $manifest "refs/tags/v$actual" $false 'does not match Cargo workspace version'

# A standalone package version override must also fail before any staging or
# build. Assert the release script reaches this guard, not a missing-exe error.
$package = Join-Path $PSScriptRoot 'package-release.ps1'
$ErrorActionPreference = 'Continue'
$output = & powershell -NoProfile -File $package -SkipBuild -NoInstaller -Version "$actual-wrong" 2>&1
$code = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($code -eq 0 -or -not (($output | ForEach-Object { $_.ToString() }) -join "`n").Contains('Package version')) {
    throw 'Packaging did not reject its mismatched version before staging.'
}
$checks++

$tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath())
$fixture = Join-Path $tempBase ("csr-release-version-" + [Guid]::NewGuid().ToString('N'))
$verifiedFixture = [IO.Path]::GetFullPath($fixture)
try {
    New-Item -ItemType Directory -Path (Join-Path $fixture 'app\src') -Force | Out-Null
    $utf8 = [Text.UTF8Encoding]::new($false)
    $fixtureManifest = Join-Path $fixture 'Cargo.toml'
    [IO.File]::WriteAllText($fixtureManifest, @'
[workspace]
members = ["app"]
resolver = "3"
[workspace.package]
version = "1.2.3-beta.2"
edition = "2024"
'@, $utf8)
    [IO.File]::WriteAllText((Join-Path $fixture 'app\Cargo.toml'), @'
[package]
name = "release_guard_fixture"
version.workspace = true
edition.workspace = true
'@, $utf8)
    [IO.File]::WriteAllText((Join-Path $fixture 'app\src\lib.rs'), '', $utf8)
    Check-Guard $fixtureManifest 'v1.2.3-beta.2' $true '1.2.3-beta.2'
    Check-Guard $fixtureManifest 'v1.2.3' $false 'does not match Cargo workspace version'
    # Protect existing staging too: a rejected version may not remove it.
    $fixtureTools = Join-Path $fixture 'tools'
    New-Item -ItemType Directory -Path $fixtureTools -Force | Out-Null
    Copy-Item -LiteralPath $guard -Destination $fixtureTools
    Copy-Item -LiteralPath $package -Destination $fixtureTools
    $staging = Join-Path $fixture 'dist\CSGO-Rewrite'
    New-Item -ItemType Directory -Path $staging -Force | Out-Null
    $sentinel = Join-Path $staging 'owned-sentinel.txt'
    [IO.File]::WriteAllText($sentinel, 'Existing staging must survive rejection.', $utf8)
    $ErrorActionPreference = 'Continue'
    $output = & powershell -NoProfile -File (Join-Path $fixtureTools 'package-release.ps1') -SkipBuild -NoInstaller -Version '1.2.4' 2>&1
    $code = $LASTEXITCODE
    $ErrorActionPreference = 'Stop'
    if ($code -eq 0 -or -not (($output | ForEach-Object { $_.ToString() }) -join "`n").Contains('Package version') -or
        [IO.File]::ReadAllText($sentinel) -cne 'Existing staging must survive rejection.') {
        throw 'Rejected package version modified the existing staging directory.'
    }
    $checks++
    # Cargo really resolves two inconsistent member versions; no metadata mock.
    New-Item -ItemType Directory -Path (Join-Path $fixture 'other\src') -Force | Out-Null
    [IO.File]::WriteAllText((Join-Path $fixture 'other\Cargo.toml'), @'
[package]
name = "release_guard_other"
version = "1.2.4"
edition = "2024"
'@, $utf8)
    [IO.File]::WriteAllText((Join-Path $fixture 'other\src\lib.rs'), '', $utf8)
    [IO.File]::WriteAllText($fixtureManifest, @'
[workspace]
members = ["app", "other"]
resolver = "3"
[workspace.package]
version = "1.2.3-beta.2"
edition = "2024"
'@, $utf8)
    Check-Guard $fixtureManifest 'v1.2.3-beta.2' $false 'must share the same Cargo version'
    if (Test-Path -LiteralPath (Join-Path $fixture 'target')) {
        throw 'Metadata regression unexpectedly created build artifacts.'
    }
} finally {
    $resolved = [IO.Path]::GetFullPath($fixture)
    $prefix = $tempBase.TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar
    if ($resolved -cne $verifiedFixture -or -not $resolved.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Refusing cleanup outside the verified temporary fixture.'
    }
    if (Test-Path -LiteralPath $resolved) { Remove-Item -LiteralPath $resolved -Recurse -Force }
}
Write-Host "Release version guard: $checks actual-metadata cases passed (no builds or publication)."
