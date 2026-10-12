# Session bookkeeping for the fidelity harness. No game files are changed
# here except newly-created, known session byproducts, after the game exits.
# Steam-userdata preservation restores only an exact pre-launch snapshot.

function Assert-NoLiveLegacyGame {
    if (Get-Process csgo -ErrorAction SilentlyContinue) { throw 'A Legacy process is still running; userdata restoration is refused.' }
}

function Get-FidelitySteamRoot {
    $path = (Get-ItemProperty -LiteralPath 'HKCU:\Software\Valve\Steam' -Name SteamPath -ErrorAction Stop).SteamPath
    if (-not $path) { throw 'Steam path is unavailable; cannot preserve native player preferences.' }
    [IO.Path]::GetFullPath($path).TrimEnd('\')
}

function Assert-NoReparsePath([string]$Path) {
    $absolute = [IO.Path]::GetFullPath($Path)
    $current = [IO.Path]::GetPathRoot($absolute)
    foreach ($part in $absolute.Substring($current.Length).Split('\')) {
        if (-not $part) { continue }
        $current = Join-Path $current $part
        if (Test-Path -LiteralPath $current) {
            if ((Get-Item -LiteralPath $current -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) {
                throw 'A userdata path is a reparse point; preservation is refused.'
            }
        }
    }
}

function Get-FidelityUserdataTargets([string]$SteamRoot, [uint32]$ActiveUser) {
    if ($ActiveUser -eq 0) { throw 'Steam has no active account; cannot snapshot native player preferences.' }
    $steam = [IO.Path]::GetFullPath($SteamRoot).TrimEnd('\')
    $root = [IO.Path]::GetFullPath((Join-Path $steam ('userdata\' + $ActiveUser)))
    Assert-NoReparsePath $root
    # Legacy app 4465480 still uses app 730's userdata on some installs. These
    # are Legacy filenames only; cs2_* files are neither read nor modified.
    foreach ($app in @('730', '4465480')) {
        foreach ($name in @('config.cfg', 'config.cfg.bak', 'video.txt', 'video.txt.bak', 'videodefaults.txt', 'trustedlaunch.cfg')) {
            $relative = $app + '\local\cfg\' + $name
            $path = [IO.Path]::GetFullPath((Join-Path $root $relative))
            if (-not $path.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Userdata target escapes its account folder.' }
            Assert-NoReparsePath $path
            [pscustomobject]@{ Relative = $relative; Path = $path }
        }
    }
}

function Get-FidelityBytesHash([byte[]]$Bytes) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { [BitConverter]::ToString($sha.ComputeHash($Bytes)).Replace('-', '') } finally { $sha.Dispose() }
}

function New-FidelityUserdataSnapshot([string]$SteamRoot = '', [uint32]$ActiveUser = 0) {
    Assert-NoLiveLegacyGame
    if (-not $SteamRoot) { $SteamRoot = Get-FidelitySteamRoot }
    if (-not $ActiveUser) {
        $ActiveUser = (Get-ItemProperty -LiteralPath 'HKCU:\Software\Valve\Steam\ActiveProcess' -Name ActiveUser -ErrorAction Stop).ActiveUser
    }
    $targets = @(Get-FidelityUserdataTargets $SteamRoot $ActiveUser)
    $entries = foreach ($target in $targets) {
        $exists = Test-Path -LiteralPath $target.Path
        $data = ''; $hash = ''
        if ($exists) {
            $file = Get-Item -LiteralPath $target.Path -Force
            if ($file.PSIsContainer -or $file.Length -gt 1048576) { throw 'Userdata target is not a bounded preference file.' }
            [byte[]]$bytes = [IO.File]::ReadAllBytes($target.Path)
            if ($bytes.Length -gt 1048576) { throw 'Userdata file grew beyond the snapshot limit.' }
            $data = [Convert]::ToBase64String($bytes); $hash = Get-FidelityBytesHash $bytes
        }
        [pscustomobject]@{ Relative = $target.Relative; Existed = [bool]$exists; Data = $data; Hash = $hash }
    }
    [pscustomobject]@{ Schema = 1; SteamRoot = [IO.Path]::GetFullPath($SteamRoot).TrimEnd('\'); ActiveUser = $ActiveUser; Entries = @($entries) }
}

function Restore-FidelityUserdataSnapshot($Snapshot, [string]$SteamRoot = '') {
    # Older sessions had no userdata snapshot. Never reconstruct their settings.
    if (-not $Snapshot) { return 0 }
    Assert-NoLiveLegacyGame
    if (-not $SteamRoot) { $SteamRoot = Get-FidelitySteamRoot }
    $root = [IO.Path]::GetFullPath($SteamRoot).TrimEnd('\')
    if ($Snapshot.Schema -ne 1 -or -not [string]::Equals($root, $Snapshot.SteamRoot, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'Userdata snapshot does not match this Steam installation.'
    }
    $targets = @(Get-FidelityUserdataTargets $root $Snapshot.ActiveUser)
    if (@($Snapshot.Entries).Count -ne $targets.Count) { throw 'Userdata snapshot target set is incomplete.' }
    # Validate the complete fixed target set and every byte/hash before writes.
    $ready = foreach ($target in $targets) {
        $matches = @($Snapshot.Entries | Where-Object { $_.Relative -ceq $target.Relative })
        if ($matches.Count -ne 1) { throw 'Userdata snapshot contains an unknown or duplicate target.' }
        $entry = $matches[0]
        if ($entry.Existed -isnot [bool] -or $entry.Data.Length -gt 1398104) { throw 'Invalid userdata snapshot record.' }
        [byte[]]$bytes = [Convert]::FromBase64String($entry.Data)
        if ($entry.Existed) {
            if ($bytes.Length -gt 1048576 -or (Get-FidelityBytesHash $bytes) -cne $entry.Hash) { throw 'Userdata snapshot hash is invalid.' }
        } elseif ($bytes.Length -ne 0 -or $entry.Hash -ne '') { throw 'Nonexistent userdata target carries unexpected bytes.' }
        if ((Test-Path -LiteralPath $target.Path) -and (Get-Item -LiteralPath $target.Path -Force).PSIsContainer) { throw 'A preference file was replaced by a directory.' }
        [pscustomobject]@{ Path = $target.Path; Existed = $entry.Existed; Bytes = $bytes }
    }
    $restored = 0
    foreach ($target in $ready) {
        Assert-NoLiveLegacyGame
        Assert-NoReparsePath $target.Path
        if ($target.Existed) {
            New-Item -ItemType Directory -Path (Split-Path $target.Path -Parent) -Force | Out-Null
            Assert-NoReparsePath $target.Path
            [IO.File]::WriteAllBytes($target.Path, $target.Bytes)
        } elseif (Test-Path -LiteralPath $target.Path) {
            Remove-Item -LiteralPath $target.Path -Force -Confirm:$false
        }
        $restored++
    }
    $restored
}

function Get-GameByproducts([string]$Root) {
    $rootPath = [IO.Path]::GetFullPath($Root).TrimEnd('\')
    if (-not (Test-Path -LiteralPath $rootPath)) { return }
    $rootItem = Get-Item -LiteralPath $rootPath -Force
    if ($rootItem.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Game folder is a reparse point.' }
    foreach ($item in Get-ChildItem -LiteralPath $rootPath -Filter 'backup_round*.txt' -File -Force) {
        if (-not ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { $item }
    }
    # Walk explicitly: never follow a junction into another folder.
    foreach ($relative in @('maps\graphs', 'maps\workshop')) {
        $path = $rootPath
        $safe = $true
        foreach ($part in $relative.Split('\')) {
            $path = Join-Path $path $part
            if (-not (Test-Path -LiteralPath $path)) { $safe = $false; break }
            if ((Get-Item -LiteralPath $path -Force).Attributes -band [IO.FileAttributes]::ReparsePoint) { $safe = $false; break }
        }
        if (-not $safe) { continue }
        if ($relative -eq 'maps\graphs') {
            Get-ChildItem -LiteralPath $path -Filter '*.ain' -File -Force | Where-Object { -not ($_.Attributes -band [IO.FileAttributes]::ReparsePoint) }
        } else {
            $pending = [Collections.Generic.Stack[string]]::new()
            $pending.Push($path)
            while ($pending.Count) {
                $folder = $pending.Pop()
                Get-Item -LiteralPath $folder -Force
                foreach ($item in Get-ChildItem -LiteralPath $folder -Force) {
                    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { continue }
                    if ($item.PSIsContainer) { $pending.Push($item.FullName) } else { $item }
                }
            }
        }
    }
}

function Remove-SessionByproducts($Session) {
    $root = [IO.Path]::GetFullPath($Session.Root).TrimEnd('\')
    $baseline = [Collections.Generic.HashSet[string]]::new([StringComparer]::OrdinalIgnoreCase)
    foreach ($path in $Session.Before) { [void]$baseline.Add($path) }
    $removed = 0
    foreach ($item in @(Get-GameByproducts $root | Sort-Object { $_.FullName.Length } -Descending)) {
        $absolute = [IO.Path]::GetFullPath($item.FullName)
        if (-not $absolute.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) { throw "Cleanup path escapes game folder: $absolute" }
        if ($baseline.Contains($absolute)) { continue }
        # No recursive deletion. New directories are removed only if empty;
        # pre-existing files and directories survive regardless of timestamp.
        if ($item.PSIsContainer -and @(Get-ChildItem -LiteralPath $absolute -Force).Count) { continue }
        Remove-Item -LiteralPath $absolute -Force -Confirm:$false
        $removed++
    }
    return $removed
}

function Get-OwnedGameProcess($Session) {
    if (-not $Session -or -not $Session.ProcessId) { return }
    $process = Get-Process -Id $Session.ProcessId -ErrorAction SilentlyContinue
    if ($process -and $process.ProcessName -eq 'csgo' -and $process.StartTime.ToUniversalTime().ToString('o') -eq $Session.ProcessStart) { $process }
}
