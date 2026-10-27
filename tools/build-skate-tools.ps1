param([Parameter(Mandatory)][string]$OutputDir)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest=Join-Path $repo 'mods/skate/Cargo.toml'
# Keep independent workspaces in separate caches. CI checkout cleaning must
# not discard the provider's Bevy/Rapier dependencies before every build.
$target=Join-Path $repo 'mods/skate/target'
$runtimeTarget=Join-Path $repo 'mods/skate-runtime/target'
if($env:CARGO_TARGET_DIR){
    $providerCache=Join-Path ([IO.Path]::GetFullPath($env:CARGO_TARGET_DIR)) 'skate-providers'
    $target=Join-Path $providerCache 'creator'
    $runtimeTarget=Join-Path $providerCache 'runtime'
}
function Prepare-ProviderCheckout([string]$Manifest,[string]$Target,[string[]]$Packages){
    New-Item -ItemType Directory -Force -Path $Target | Out-Null
    $receipt=Join-Path $Target 'csr-provider-checkout.txt'
    $previous=if(Test-Path -LiteralPath $receipt){(Get-Content -LiteralPath $receipt -Raw).Trim()}else{''}
    if($previous -ne $repo){
        # Like the main shared target handoff, clean local code for this
        # profile while retaining all third-party dependencies.
        $cleanArgs=@('clean','--manifest-path',$Manifest,'--target-dir',$Target,'--profile','release')
        foreach($package in $Packages){$cleanArgs+=@('-p',$package)}
        & cargo @cleanArgs | Out-Host
        if($LASTEXITCODE -ne 0){throw 'Cleaning a provider checkout handoff failed'}
    }
    return $receipt
}
$skateBuildFlags=$env:RUSTFLAGS
try {
    # Like the game installer, the creator must work without separately
    # installing the MSVC runtime on a friend's machine.
    $env:RUSTFLAGS='-C target-feature=+crt-static'
    $creatorReceipt=Prepare-ProviderCheckout $manifest $target @('csr-skate','mechanics','mechanics-api','mechanics-link','movement','collision','mdl','bsp','kv','packs','content','vfs','vpk')
    & cargo build --manifest-path $manifest --target-dir $target --release --locked --features authoring --bin prepare-skate-pack
    if($LASTEXITCODE -ne 0){throw 'Building the Skate pack creator failed'}
    [IO.File]::WriteAllText($creatorReceipt,$repo)
    $runtimeManifest=Join-Path $repo 'mods/skate-runtime/Cargo.toml'
    $runtimeReceipt=Prepare-ProviderCheckout $runtimeManifest $runtimeTarget @('csr-skate-runtime','mechanics-link')
    & cargo build --manifest-path $runtimeManifest --target-dir $runtimeTarget --release --locked --bin csr-skate-runtime
    if($LASTEXITCODE -ne 0){throw 'Building the full Skate runtime failed'}
    [IO.File]::WriteAllText($runtimeReceipt,$repo)
} finally {
    if($null -eq $skateBuildFlags){Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue}else{$env:RUSTFLAGS=$skateBuildFlags}
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
Copy-Item -LiteralPath (Join-Path $target 'release/prepare-skate-pack.exe') -Destination (Join-Path $OutputDir 'Skate-Pack-Creator.exe')
Copy-Item -LiteralPath (Join-Path $runtimeTarget 'release/csr-skate-runtime.exe') -Destination (Join-Path $OutputDir 'Skate-Runtime.exe')
Copy-Item -LiteralPath (Join-Path $repo 'mods/skate/LICENSE') -Destination (Join-Path $OutputDir 'Skate-GPL-3.0.txt')
Copy-Item -LiteralPath (Join-Path $repo 'mods/skate-runtime/NOTICE') -Destination (Join-Path $OutputDir 'Skate-NOTICE.txt')
$sourceRevision=(& git -C $repo rev-parse HEAD).Trim()
@"
Corresponding GPL source for Skate-Runtime.exe and Skate-Pack-Creator.exe:
https://github.com/bentsignal/csr/tree/$sourceRevision/mods/skate-runtime
https://github.com/bentsignal/csr/tree/$sourceRevision/mods/skate
Build scripts and dependency locks are in the same source revision.
Build: tools/build-skate-tools.ps1 -OutputDir C:/private/skate-tools
Upstream source and attribution: Skate-NOTICE.txt; license: Skate-GPL-3.0.txt.
Original game assets are not part of these programs or their public source.
"@ | Set-Content -LiteralPath (Join-Path $OutputDir 'Skate-SOURCE.txt') -Encoding utf8
