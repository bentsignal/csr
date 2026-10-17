# Only the root operator executes this helper. No focus, input, or capture APIs.
param(
    [Parameter(Mandatory)][string]$Executable,
    [Parameter(Mandatory)][string]$Sha256,
    [Parameter(Mandatory)][string]$Head,
    [Parameter(Mandatory)][string]$Manifest,
    [Parameter(Mandatory)][string]$PrivateRoot,
    [Parameter(Mandatory)][string]$OwnerData,
    [Parameter(Mandatory)][string]$SteamRoot,
    [Parameter(Mandatory)][string]$GameDir,
    [Parameter(Mandatory)][ValidateSet('de_dust2_2017','de_nuke')][string]$Map,
    [Parameter(Mandatory)][ValidateSet(0,3)][int]$Bots,
    [ValidateSet('sustained','hud-baseline','hud-no-notices','hud-no-classic','hud-render-trace')][string]$Experiment='sustained',
    [ValidateRange(120,1200)][int]$MaxSeconds,
    [ValidateRange(12,128)][int]$MinFreeGiB=12,
    [ValidateRange(0,16384)][int]$ExpectedWidth=0,
    [ValidateRange(0,16384)][int]$ExpectedHeight=0,
    [switch]$NativeSlotGranted
)
$ErrorActionPreference='Stop'
if(!$PSBoundParameters.ContainsKey('MaxSeconds')){$MaxSeconds=if($Experiment -eq 'sustained'){900}else{180}}
if(($Experiment -eq 'sustained' -and ($MaxSeconds -lt 660 -or $MaxSeconds -gt 1200)) -or
   ($Experiment -ne 'sustained' -and ($MaxSeconds -lt 120 -or $MaxSeconds -gt 300))){throw 'MaxSeconds outside experiment bounded range'}
if(($ExpectedWidth -eq 0) -ne ($ExpectedHeight -eq 0)){throw 'Specify both ExpectedWidth and ExpectedHeight, or neither'}
$resolutionArgs=@()
if($ExpectedWidth -gt 0){$resolutionArgs=@('--expected-resolution',[string]$ExpectedWidth,[string]$ExpectedHeight)}
if(!$NativeSlotGranted){throw 'Root must explicitly grant the native slot; nothing started'}
# Reentrant when called by with-native-slot.ps1; otherwise obtain the same slot
# here. The switch alone never bypasses a live slot owned by another process.
$slot=[Threading.Mutex]::new($false,'Local\CSGORewrite.NativeFidelity')
$held=$false
$previousSlotParent=$env:CSR_PERF_CASE_SLOT_PARENT_PID
try {
    try {$held=$slot.WaitOne(0)} catch [Threading.AbandonedMutexException] {$held=$true}
    if(!$held){throw 'Native fidelity slot is occupied; nothing started'}
    $env:CSR_PERF_CASE_SLOT_PARENT_PID=[string]$PID
    & python -B (Join-Path $PSScriptRoot 'performance_case.py') --execute --native-slot-granted `
        --executable $Executable --sha256 $Sha256 --head $Head --manifest $Manifest `
        --private-root $PrivateRoot --owner-data $OwnerData --steam-root $SteamRoot `
        --game-dir $GameDir --map $Map --bots $Bots --experiment $Experiment --max-seconds $MaxSeconds --min-free-gib $MinFreeGiB @resolutionArgs
    if($LASTEXITCODE -ne 0){throw "Performance case closed with failure ($LASTEXITCODE); inspect private status.json"}
} finally {
    $env:CSR_PERF_CASE_SLOT_PARENT_PID=$previousSlotParent
    if($held){$slot.ReleaseMutex()}
    $slot.Dispose()
}
