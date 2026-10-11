# Session bookkeeping for the fidelity harness. No game files are changed
# here except newly-created, known session byproducts, after the game exits.

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
