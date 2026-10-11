<#
.SYNOPSIS
  Fidelity harness: matched screenshots and sound logs from the real CS:GO
  Legacy and from CSGO Rewrite, plus difference metrics. See docs/FIDELITY.md.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File tools\fidelity\fidelity.ps1 all
  powershell -ExecutionPolicy Bypass -File tools\fidelity\fidelity.ps1 ours -Only ct_garage
  powershell -ExecutionPolicy Bypass -File tools\fidelity\fidelity.ps1 sounds

.NOTES
  Commands: real (capture the real game), ours (render ours), compare,
  all (real + ours + compare), sounds (real-game sound log probes), quit.
  Output goes to -Out (default %USERPROFILE%\csr-fidelity), never into the
  repository: captures contain Valve art and must not be committed.
#>
param(
    [Parameter(Position = 0)][ValidateSet('real', 'ours', 'compare', 'all', 'sounds', 'quit')][string]$Command = 'all',
    [string]$Out = (Join-Path $env:USERPROFILE 'csr-fidelity'),
    [string[]]$Only = @(),
    [int]$Width = 1600,
    [int]$Height = 900,
    [int]$Port = 27999,
    [string]$Game = $(if ($env:CSR_GAME_DIR) { Split-Path $env:CSR_GAME_DIR -Parent } else { 'C:\Program Files (x86)\Steam\steamapps\common\csgo legacy' }),
    [string]$Csr = '',
    [switch]$KeepGame
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$repo = Split-Path (Split-Path $here -Parent) -Parent
Add-Type -TypeDefinition (Get-Content -Raw (Join-Path $here 'FidelityKit.cs')) -ReferencedAssemblies System.Drawing

# CS:GO's standing view offset: `getpos` prints origin + this.
$EyeHeight = 64.062561
$shots = Join-Path $Out 'captures'
New-Item -ItemType Directory -Force $shots | Out-Null
if ((Resolve-Path $Out).Path.StartsWith((Resolve-Path $repo).Path, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Output folder $Out is inside the repository; captures contain Valve art. Use a folder outside it."
}

function Get-Viewpoints([switch]$Resolved) {
    $file = Join-Path $here 'viewpoints.txt'
    $r = Join-Path $Out 'viewpoints-resolved.txt'
    if ($Resolved -and (Test-Path $r)) { $file = $r }
    Get-Content $file | Where-Object { $_ -and -not $_.StartsWith('#') } | ForEach-Object {
        $f = -split $_
        [pscustomobject]@{ Name = $f[0]; Map = $f[1]; X = [double]$f[2]; Y = [double]$f[3]; Z = [double]$f[4]; Pitch = [double]$f[5]; Yaw = [double]$f[6] }
    } | Where-Object { $Only.Count -eq 0 -or $Only -contains $_.Name }
}

$SessionFile = Join-Path $Out 'game-session-start.txt'

function Connect-Game([string]$map) {
    $running = Get-Process csgo -ErrorAction SilentlyContinue
    if (-not $running) {
        # Remember when the session began, so files the game creates in its
        # own folder (round backups, the Workshop map sync) can be removed.
        (Get-Date).ToString('o') | Set-Content $SessionFile
        $exe = Join-Path $Game 'csgo.exe'
        if (-not (Test-Path $exe)) { throw "csgo.exe not found under $Game" }
        Write-Host "Starting CS:GO Legacy ($map)..."
        # -insecure: no VAC session; -netconport: console over TCP for this script.
        Start-Process -FilePath $exe -ArgumentList "-insecure -novid -nojoy -windowed -noborder -w $Width -h $Height -netconport $Port +map $map" | Out-Null
    }
    $con = [CsrFidelity.NetCon]::Connect($Port, 120000)
    $deadline = (Get-Date).AddSeconds(150)
    while ((Get-Date) -lt $deadline) {
        $t = $con.Now
        $con.Send('status')
        Start-Sleep 2
        $s = $con.Since($t) -join "`n"
        if ($s -match "map\s+:\s+$map" -and $s -match '#\s+\d+\s+\d+\s+"[^"]+"\s+\S+\s+\S+\s+\d+\s+\d+\s+active') { break }
        if ($s -match '#\s+\d+\s+\d+\s+"' -and $s -notmatch "map\s+:\s+$map") { $con.Send("map $map") ; Start-Sleep 10 }
    }
    # A quiet, frozen, unobstructed scene: no bots, no HUD or viewmodel,
    # rounds that never end. Cheats are needed for setpos/noclip.
    foreach ($c in @('sv_cheats 1', 'sv_max_allowed_developer 1', 'jointeam 3 1', 'bot_quota 0', 'bot_kick', 'mp_warmup_end',
            'mp_freezetime 0', 'mp_backup_round_auto 0', 'mp_roundtime 60', 'mp_roundtime_defuse 60', 'mp_ignore_round_win_conditions 1',
            'cl_drawhud 0', 'r_drawviewmodel 0', 'cl_showpos 0', 'cl_showfps 0', 'net_graph 0', 'fps_max 120',
            'developer 0', 'con_drawnotify 0', 'con_notifytime 0')) {
        $con.Send($c)
    }
    Start-Sleep 3
    return $con
}

function Stop-Game($con) {
    if ($KeepGame) { return }
    try { $con.Send('quit') } catch {}
    Start-Sleep 3
    Get-Process csgo -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep 2
    Remove-GameLeftovers
}

# The install is read-only for us. A session makes the game write round
# backups, a nav graph cache and a copy of every subscribed Workshop map
# (gigabytes) into csgo\; delete whatever it created since the session began.
function Remove-GameLeftovers {
    if (-not (Test-Path $SessionFile)) { return }
    $since = [datetime]::Parse((Get-Content $SessionFile))
    $root = Join-Path $Game 'csgo'
    # Only the known session byproducts, so a Steam update that lands
    # mid-session is never touched.
    $created = @(
        Get-ChildItem (Join-Path $root 'maps\workshop') -Recurse -Force -ErrorAction SilentlyContinue
        Get-Item (Join-Path $root 'maps\workshop') -Force -ErrorAction SilentlyContinue
        Get-ChildItem $root -Filter 'backup_round*.txt' -Force -ErrorAction SilentlyContinue
        Get-ChildItem (Join-Path $root 'maps\graphs') -Filter '*.ain' -Force -ErrorAction SilentlyContinue
    ) | Where-Object { $_.CreationTime -gt $since } | Sort-Object { $_.FullName.Length } -Descending
    foreach ($item in $created) {
        Remove-Item -LiteralPath $item.FullName -Recurse -Force -Confirm:$false -ErrorAction SilentlyContinue
    }
    if ($created.Count) { Write-Host "Removed $($created.Count) files the game created during the session." }
    Remove-Item $SessionFile -ErrorAction SilentlyContinue
}

function Invoke-Real {
    $views = @(Get-Viewpoints)
    $con = Connect-Game $views[0].Map
    $con.Send('noclip 1')
    $hwnd = (Get-Process csgo).MainWindowHandle
    $resolved = @()
    foreach ($v in $views) {
        $con.Send(('setpos {0} {1} {2}' -f $v.X, $v.Y, ($v.Z - $EyeHeight)))
        $con.Send(('setang {0} {1} 0' -f $v.Pitch, $v.Yaw))
        Start-Sleep -Milliseconds 1500
        # Read back the eye the engine really used (it may nudge the
        # player out of solids); ours renders from the same point.
        $t = $con.Now
        $con.Send('getpos')
        Start-Sleep -Milliseconds 300
        $eye = $con.Since($t) | Where-Object { $_ -match 'setpos (\S+) (\S+) (\S+);setang (\S+) (\S+)' } | Select-Object -Last 1
        if ($eye -match 'setpos (\S+) (\S+) (\S+);setang (\S+) (\S+)') {
            $resolved += ('{0} {1} {2:F2} {3:F2} {4:F2} {5:F2} {6:F2}' -f $v.Name, $v.Map, [double]$Matches[1], [double]$Matches[2], [double]$Matches[3], [double]$Matches[4], [double]$Matches[5])
        }
        $path = Join-Path $shots "$($v.Name)_real.png"
        [CsrFidelity.Window]::Capture($hwnd, $path)
        Write-Host "real  $($v.Name)"
    }
    $resolved | Set-Content (Join-Path $Out 'viewpoints-resolved.txt')
    Stop-Game $con
}

function Invoke-Ours {
    $exe = if ($Csr) { $Csr } else { Join-Path $repo 'target\release\csr.exe' }
    if (-not (Test-Path $exe)) { throw "Build first: cargo build --release -p launcher ($exe not found)" }
    $env:CSR_NO_HUD = '1'
    $env:CSR_NO_SELF = '1'
    foreach ($v in Get-Viewpoints -Resolved) {
        $path = Join-Path $shots "$($v.Name)_ours.png"
        $arguments = "play $($v.Map) --bots 0 --size $Width $Height --camera $($v.X) $($v.Y) $($v.Z) $($v.Pitch) $($v.Yaw) --screenshot `"$path`""
        $log = Join-Path $Out 'csr-last.log'
        $p = Start-Process -FilePath $exe -ArgumentList $arguments -WorkingDirectory $repo -Wait -PassThru -NoNewWindow -RedirectStandardError $log -RedirectStandardOutput "$log.out"
        Write-Host "ours  $($v.Name) (exit $($p.ExitCode))"
    }
}

function Invoke-Compare {
    $rows = foreach ($v in Get-Viewpoints) {
        $a = Join-Path $shots "$($v.Name)_real.png"
        $b = Join-Path $shots "$($v.Name)_ours.png"
        if (-not ((Test-Path $a) -and (Test-Path $b))) { continue }
        [CsrFidelity.Compare]::DiffImage($a, $b, (Join-Path $shots "$($v.Name)_diff.png"))
        "$($v.Name)`t" + [CsrFidelity.Compare]::Metrics($a, $b)
    }
    $stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
    $rows | Set-Content (Join-Path $Out "metrics-$stamp.tsv")
    $rows | ForEach-Object { Write-Host $_ }
    $vals = $rows | ForEach-Object { if ($_ -match 'mae=([\d.]+) ssim=([\d.]+)') { [pscustomobject]@{ Mae = [double]$Matches[1]; Ssim = [double]$Matches[2] } } }
    if ($vals) {
        Write-Host ('mean  mae={0:F1} ssim={1:F3}  ({2} pairs, metrics-{3}.tsv)' -f ($vals | Measure-Object Mae -Average).Average, ($vals | Measure-Object Ssim -Average).Average, @($vals).Count, $stamp)
    }
}

function Invoke-Sounds {
    $con = Connect-Game 'de_dust2'
    $con.Send('developer 1')
    $con.Send('noclip 0')
    $logs = Join-Path $Out 'sounds'
    New-Item -ItemType Directory -Force $logs | Out-Null
    $scenarios = [ordered]@{}
    $name = $null
    foreach ($line in Get-Content (Join-Path $here 'soundprobes.txt')) {
        $l = $line.Trim()
        if (-not $l -or $l.StartsWith('#')) { continue }
        if ($l -match '^\[(.+)\]$') { $name = $Matches[1]; $scenarios[$name] = @(); continue }
        $scenarios[$name] += $l
    }
    foreach ($s in $scenarios.Keys) {
        if ($Only.Count -and $Only -notcontains $s) { continue }
        $con.Send('snd_showstart 0')
        Start-Sleep -Milliseconds 300
        $t = $con.Now
        $con.Send('snd_showstart 2')
        $con.Send("echo ==== $s")
        foreach ($step in $scenarios[$s]) {
            if ($step -match '^wait (\d+)$') { Start-Sleep -Milliseconds ([int]$Matches[1]) }
            else { $con.Send($step); $con.Send("echo @@ $step") }
        }
        $con.Send('snd_showstart 0')
        Start-Sleep -Milliseconds 500
        $con.Since($t) | Set-Content (Join-Path $logs "$s.log")
        Write-Host "sound $s"
    }
    $con.Send('developer 0')
    Stop-Game $con
}

switch ($Command) {
    'real' { Invoke-Real }
    'ours' { Invoke-Ours }
    'compare' { Invoke-Compare }
    'all' { Invoke-Real; Invoke-Ours; Invoke-Compare }
    'sounds' { Invoke-Sounds }
    'quit' { try { $c = [CsrFidelity.NetCon]::Connect($Port, 2000); Stop-Game $c } catch { Get-Process csgo -ErrorAction SilentlyContinue | Stop-Process -Force } }
}
