# Resolve versions through Cargo's actual workspace metadata; never infer a
# release version from the first version-like line in a TOML dependency table.
param(
    [string]$Tag,
    [string]$Version,
    [string]$ManifestPath = (Join-Path (Split-Path -Parent $PSScriptRoot) 'Cargo.toml')
)
$ErrorActionPreference = 'Stop'

$manifest = (Resolve-Path -LiteralPath $ManifestPath).Path
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
if (-not $cargo) { $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe' }
$json = & $cargo metadata --no-deps --format-version 1 --locked --offline --manifest-path $manifest
if ($LASTEXITCODE -ne 0) { throw 'Could not read Cargo release metadata.' }
$metadata = ($json -join "`n") | ConvertFrom-Json
$members = @($metadata.workspace_members)
$packages = @($metadata.packages | Where-Object { $members -ccontains $_.id })
if ($members.Count -eq 0 -or $packages.Count -ne $members.Count) {
    throw 'Cargo metadata did not contain every workspace member.'
}
$actual = [string]$packages[0].version
if (-not $actual -or @($packages | Where-Object { $_.version -cne $actual }).Count -ne 0) {
    throw 'Every release workspace member must share the same Cargo version.'
}
if ($Version -and $Version -cne $actual) {
    throw "Package version '$Version' does not match Cargo workspace version '$actual'."
}
if ($Tag -and $Tag -cne "v$actual") {
    throw "Release tag '$Tag' does not match Cargo workspace version '$actual' (expected 'v$actual')."
}
# Emit just the resolved version so packaging and publishing can select its
# exact filenames. No builds, dependency downloads or staging mutations occur.
Write-Output $actual
