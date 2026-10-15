param(
    [string]$Script=(Join-Path $PSScriptRoot 'test-session.ps1'),
    [ValidateRange(1,1800)][int]$WaitSeconds=600,
    [switch]$NativeSlotGranted
)
$ErrorActionPreference='Stop'
if(![IO.Path]::IsPathRooted($Script) -or !(Test-Path -LiteralPath $Script -PathType Leaf)){throw 'A real absolute script path is required'}
# The runner and desktop native observations share the same Windows login.
# The owned game helper still performs process identity and userdata guards;
# this mutex does not replace them or authorize stopping an existing game.
$slot=[Threading.Mutex]::new($false,'Local\CSGORewrite.NativeFidelity')
$held=$false
$deadline=[DateTime]::UtcNow.AddSeconds($WaitSeconds)
try {
    while(!$held -and [DateTime]::UtcNow -lt $deadline){
        try {$held=$slot.WaitOne(1000)}
        catch [Threading.AbandonedMutexException] {$held=$true}
    }
    if(!$held){throw 'Native fidelity slot deadline exceeded; no script started'}
    $global:LASTEXITCODE=0
    if($NativeSlotGranted){& $Script -NativeSlotGranted}else{& $Script}
    if(!$? -or $LASTEXITCODE -ne 0){throw 'Owned fidelity script failed; inspect its preserved receipts'}
} finally {
    if($held){$slot.ReleaseMutex()}
    $slot.Dispose()
}
