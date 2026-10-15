param(
    [Parameter(Mandatory)][ValidateSet('dev','play','release')][string]$Profile,
    [Parameter(Mandatory)][string]$TargetDir,
    [switch]$DryRun
)
$ErrorActionPreference='Stop'
$repo=[IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
if(![IO.Path]::IsPathRooted($TargetDir)){throw 'TargetDir must be an explicit absolute Cargo target directory'}
$target=[IO.Path]::GetFullPath($TargetDir)
$manifest=Join-Path $repo 'Cargo.toml'
$cargo=(Get-Command cargo -ErrorAction SilentlyContinue|Select-Object -First 1 -ExpandProperty Source)
if(!$cargo){$cargo=Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'}
if(!(Test-Path -LiteralPath $cargo -PathType Leaf)){throw 'Rustup Cargo is unavailable; no cleanup performed'}
$metadata=& $cargo metadata --no-deps --locked --format-version 1 --manifest-path $manifest
if($LASTEXITCODE -ne 0){throw 'Cargo metadata failed; no cleanup performed'}
$metadata=$metadata|ConvertFrom-Json
$packages=@($metadata.packages|Where-Object {$_.id -in $metadata.workspace_members}|Select-Object -ExpandProperty name|Sort-Object -Unique)
if(!$packages.Count -or @($packages|Where-Object {$_ -notmatch '^[a-zA-Z0-9_-]+$'}).Count){throw 'Invalid workspace package set; no cleanup performed'}
$cargoArgs=@('clean','--manifest-path',$manifest,'--target-dir',$target,'--profile',$Profile)
foreach($name in $packages){$cargoArgs+=@('-p',$name)}
Write-Output ("Workspace profile handoff: {0} packages; {1}; {2}" -f $packages.Count,$Profile,$target)
if($DryRun){Write-Output ($packages -join ', ');return}
# Use only Cargo's package-scoped cleanup. Keep Bevy/Ruffle and all other
# external dependencies; never recursively remove a computed target path.
& $cargo @cargoArgs
if($LASTEXITCODE -ne 0){throw 'Workspace package cleanup failed'}
$head=& git -C $repo rev-parse HEAD
if($LASTEXITCODE -ne 0){throw 'Cannot record source identity after cleanup'}
@{worktree=$repo;head=$head;profile=$Profile;target=$target;workspace_packages=$packages;cleaned_utc=[DateTime]::UtcNow.ToString('o')}|
    ConvertTo-Json -Depth 4|Set-Content -LiteralPath (Join-Path $target ('csr-workspace-clean-'+$Profile+'.json')) -Encoding utf8
