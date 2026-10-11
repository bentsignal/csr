$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'session.ps1')
$fixture = Join-Path ([IO.Path]::GetTempPath()) ('csr-session-test-' + [guid]::NewGuid())
New-Item -ItemType Directory -Path (Join-Path $fixture 'csgo\maps\workshop\old'), (Join-Path $fixture 'csgo\maps\graphs'), (Join-Path $fixture 'outside') -Force | Out-Null
$root = Join-Path $fixture 'csgo'
Set-Content (Join-Path $root 'maps\workshop\old\keep.bsp') 'existing map'
Set-Content (Join-Path $root 'backup_round_keep.txt') 'existing backup'
Set-Content (Join-Path $fixture 'outside\keep.bsp') 'outside'
$session = [pscustomobject]@{ Root = $root; Before = @(Get-GameByproducts $root | ForEach-Object FullName) }
New-Item -ItemType Directory -Path (Join-Path $root 'maps\workshop\new') | Out-Null
Set-Content (Join-Path $root 'maps\workshop\new\new.bsp') 'new map'
Set-Content (Join-Path $root 'maps\graphs\new.ain') 'new graph'
Set-Content (Join-Path $root 'backup_round_new.txt') 'new backup'
Set-Content (Join-Path $root 'unrelated.txt') 'unrelated'
# Junctions are omitted from traversal; a session must not touch their targets.
New-Item -ItemType Junction -Path (Join-Path $root 'maps\workshop\link') -Target (Join-Path $fixture 'outside') | Out-Null
$removed = Remove-SessionByproducts $session
if ($removed -ne 4) { throw "Expected four new byproducts removed, got $removed" }
foreach ($path in @('maps\workshop\old\keep.bsp', 'backup_round_keep.txt', 'unrelated.txt', 'maps\workshop\link\keep.bsp')) {
    if (-not (Test-Path -LiteralPath (Join-Path $root $path))) { throw "Existing or unrelated file was deleted: $path" }
}
if (Test-Path -LiteralPath (Join-Path $root 'maps\workshop\new')) { throw 'Empty new directory was retained' }
# Remove the junction itself first, then the test's own known temporary tree.
[IO.Directory]::Delete((Join-Path $root 'maps\workshop\link'))
$absolute = [IO.Path]::GetFullPath($fixture)
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
if (-not $absolute.StartsWith($tempRoot + '\csr-session-test-', [StringComparison]::OrdinalIgnoreCase)) { throw 'Test cleanup escaped its temporary fixture' }
Remove-Item -LiteralPath $absolute -Recurse -Force
Write-Host 'Session cleanup tests passed: existing files, unrelated files and junction targets preserved.'
