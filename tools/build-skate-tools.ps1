param([Parameter(Mandatory)][string]$OutputDir)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest=Join-Path $repo 'mods/skate/Cargo.toml'
# The creator embeds this exact module. Keep its private Cargo workspace apart
# from the CS workspace's profile cache; neither build reads any game data.
$target=Join-Path $repo 'mods/skate/target'
$skateBuildFlags=$env:RUSTFLAGS
try {
    # Like the game installer, the creator must work without separately
    # installing the MSVC runtime on a friend's machine.
    $env:RUSTFLAGS='-C target-feature=+crt-static'
    & cargo build --manifest-path $manifest --target-dir $target --release --locked --features authoring --bin prepare-skate-pack
    if($LASTEXITCODE -ne 0){throw 'Building the Skate pack creator failed'}
    & cargo build --manifest-path (Join-Path $repo 'mods/skate-runtime/Cargo.toml') --target-dir (Join-Path $repo 'mods/skate-runtime/target') --release --locked --bin csr-skate-runtime
    if($LASTEXITCODE -ne 0){throw 'Building the full Skate runtime failed'}
} finally {
    if($null -eq $skateBuildFlags){Remove-Item Env:RUSTFLAGS -ErrorAction SilentlyContinue}else{$env:RUSTFLAGS=$skateBuildFlags}
}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
Copy-Item -LiteralPath (Join-Path $target 'release/prepare-skate-pack.exe') -Destination (Join-Path $OutputDir 'Skate-Pack-Creator.exe')
Copy-Item -LiteralPath (Join-Path $repo 'mods/skate-runtime/target/release/csr-skate-runtime.exe') -Destination (Join-Path $OutputDir 'Skate-Runtime.exe')
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
