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
    [ValidateRange(660,1200)][int]$MaxSeconds=900,
    [ValidateRange(12,128)][int]$MinFreeGiB=12,
    [switch]$NativeSlotGranted
)
$ErrorActionPreference='Stop'
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
    & python (Join-Path $PSScriptRoot 'performance_case.py') --execute --native-slot-granted `
        --executable $Executable --sha256 $Sha256 --head $Head --manifest $Manifest `
        --private-root $PrivateRoot --owner-data $OwnerData --steam-root $SteamRoot `
        --game-dir $GameDir --map $Map --bots $Bots --max-seconds $MaxSeconds --min-free-gib $MinFreeGiB
    if($LASTEXITCODE -ne 0){throw "Performance case closed with failure ($LASTEXITCODE); inspect private status.json"}
} finally {
    $env:CSR_PERF_CASE_SLOT_PARENT_PID=$previousSlotParent
    if($held){$slot.ReleaseMutex()}
    $slot.Dispose()
}
