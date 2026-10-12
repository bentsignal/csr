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

# All userdata fixtures are private synthetic bytes under this test's own
# directory; never inspect or restore the runner's real Steam account.
$steam = Join-Path $fixture 'steam'
$cfg = Join-Path $steam 'userdata\42\730\local\cfg'
$otherCfg = Join-Path $steam 'userdata\42\4465480\local\cfg'
New-Item -ItemType Directory -Path $cfg, $otherCfg -Force | Out-Null
$config = Join-Path $cfg 'config.cfg'
$video = Join-Path $otherCfg 'video.txt'
[byte[]]$original = @(0, 255, 13, 10, 128, 65)
[IO.File]::WriteAllBytes($config, $original)
[IO.File]::WriteAllBytes($video, [byte[]]@())
Set-Content -LiteralPath (Join-Path $cfg 'unrelated.txt') 'unrelated profile'
function Expect-Refusal([scriptblock]$Action) {
    $failed = $false
    try { & $Action | Out-Null } catch { $failed = $true }
    if (-not $failed) { throw 'Expected userdata operation to fail closed.' }
}
function Changed-Config {
    [IO.File]::WriteAllBytes($config, [byte[]]@(1, 2, 3))
}
function Assert-ConfigNotRestored {
    if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($config)) -ne 'AQID') { throw 'Invalid snapshot mutated a valid earlier preference target.' }
}
$snapshot = New-FidelityUserdataSnapshot -SteamRoot $steam -ActiveUser 42
# Exercise the exact serialized shape retained by the real harness.
$snapshot = $snapshot | ConvertTo-Json -Depth 6 | ConvertFrom-Json
Changed-Config
Remove-Item -LiteralPath $video
$created = Join-Path $cfg 'config.cfg.bak'
Set-Content -LiteralPath $created 'native-created backup'
if ((Restore-FidelityUserdataSnapshot $snapshot -SteamRoot $steam) -ne 12) { throw 'Expected all twelve fixed targets restored.' }
if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($config)) -ne [Convert]::ToBase64String($original)) { throw 'Binary/encoding preference bytes were not restored exactly.' }
if (-not (Test-Path -LiteralPath $video) -or (Get-Item -LiteralPath $video).Length -ne 0) { throw 'Original empty video file was not recreated.' }
if (Test-Path -LiteralPath $created) { throw 'New known backup survived original-nonexistence restoration.' }
if ((Get-Content -LiteralPath (Join-Path $cfg 'unrelated.txt')) -ne 'unrelated profile') { throw 'Unrelated profile file changed.' }

# A late corrupt entry rejects the whole snapshot before the first write.
Changed-Config
$bad = $snapshot | ConvertTo-Json -Depth 6 | ConvertFrom-Json
($bad.Entries | Where-Object Relative -eq '4465480\local\cfg\video.txt').Hash = 'corrupt'
Expect-Refusal { Restore-FidelityUserdataSnapshot $bad -SteamRoot $steam }
Assert-ConfigNotRestored
$bad = $snapshot | ConvertTo-Json -Depth 6 | ConvertFrom-Json
$bad.Entries[1].Relative = '..\outside\keep.bsp'
Expect-Refusal { Restore-FidelityUserdataSnapshot $bad -SteamRoot $steam }
Assert-ConfigNotRestored
Expect-Refusal { Restore-FidelityUserdataSnapshot $snapshot -SteamRoot (Join-Path $fixture 'outside') }
Assert-ConfigNotRestored

# Fake process guard proves a live owned OR unrelated Legacy process blocks
# restoration before any file mutation. No native process is launched.
$originalGuard = ${function:Assert-NoLiveLegacyGame}
try {
    function Assert-NoLiveLegacyGame { throw 'Synthetic live Legacy process.' }
    Expect-Refusal { Restore-FidelityUserdataSnapshot $snapshot -SteamRoot $steam }
    Expect-Refusal { New-FidelityUserdataSnapshot -SteamRoot $steam -ActiveUser 42 }
    Assert-ConfigNotRestored
} finally { Set-Item Function:Assert-NoLiveLegacyGame $originalGuard }

# A junction inserted after capture is rejected before bytes can escape the
# approved Steam account folder. Both moved paths belong to this fixture.
$savedCfg = Join-Path $fixture 'saved-cfg'
foreach ($path in @($cfg, $savedCfg)) {
    if (-not [IO.Path]::GetFullPath($path).StartsWith([IO.Path]::GetFullPath($fixture) + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Fixture move escaped its root.' }
}
Move-Item -LiteralPath $cfg -Destination $savedCfg
New-Item -ItemType Junction -Path $cfg -Target (Join-Path $fixture 'outside') | Out-Null
try {
    Expect-Refusal { Restore-FidelityUserdataSnapshot $snapshot -SteamRoot $steam }
    Expect-Refusal { New-FidelityUserdataSnapshot -SteamRoot $steam -ActiveUser 42 }
    if ((Get-Content -LiteralPath (Join-Path $fixture 'outside\keep.bsp')) -ne 'outside') { throw 'Junction target changed.' }
} finally {
    [IO.Directory]::Delete($cfg)
    Move-Item -LiteralPath $savedCfg -Destination $cfg
}
Restore-FidelityUserdataSnapshot $snapshot -SteamRoot $steam | Out-Null
if ((Restore-FidelityUserdataSnapshot $null -SteamRoot $steam) -ne 0) { throw 'Old session without a snapshot must not reconstruct preferences.' }
# Remove the junction itself first, then the test's own known temporary tree.
[IO.Directory]::Delete((Join-Path $root 'maps\workshop\link'))
$absolute = [IO.Path]::GetFullPath($fixture)
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
if (-not $absolute.StartsWith($tempRoot + '\csr-session-test-', [StringComparison]::OrdinalIgnoreCase)) { throw 'Test cleanup escaped its temporary fixture' }
Remove-Item -LiteralPath $absolute -Recurse -Force
Write-Host 'Session cleanup tests passed: existing files, unrelated files and junction targets preserved.'
Write-Host 'Userdata preservation passed: exact binary/empty bytes, original nonexistence, serialized snapshots, atomic validation, live-process guard and junction refusal.'
