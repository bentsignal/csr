param([Parameter(Mandatory)][string]$OutputDir)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$manifest=Join-Path $repo 'mods/skate/Cargo.toml'
# The creator embeds this exact module. Keep its private Cargo workspace apart
# from the CS workspace's profile cache; neither build reads any game data.
$target=Join-Path $repo 'mods/skate/target'
& rustup target add wasm32-unknown-unknown
if($LASTEXITCODE -ne 0){throw 'Installing the WebAssembly Rust target failed'}
& cargo build --manifest-path $manifest --target-dir $target --target wasm32-unknown-unknown --release --locked --lib
if($LASTEXITCODE -ne 0){throw 'Building the Skate movement module failed'}
& cargo build --manifest-path $manifest --target-dir $target --release --locked --features authoring --bin prepare-skate-pack
if($LASTEXITCODE -ne 0){throw 'Building the Skate pack creator failed'}
New-Item -ItemType Directory -Force -Path $OutputDir | Out-Null
Copy-Item -LiteralPath (Join-Path $target 'release/prepare-skate-pack.exe') -Destination (Join-Path $OutputDir 'Skate-Pack-Creator.exe')
Copy-Item -LiteralPath (Join-Path $repo 'mods/skate/LICENSE') -Destination (Join-Path $OutputDir 'Skate-GPL-3.0.txt')
Copy-Item -LiteralPath (Join-Path $repo 'mods/skate/NOTICE') -Destination (Join-Path $OutputDir 'Skate-NOTICE.txt')
