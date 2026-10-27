$ErrorActionPreference='Stop'
[Console]::OutputEncoding=New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Windows.Forms
[System.Windows.Forms.Application]::EnableVisualStyles()
$game=New-Object System.Windows.Forms.OpenFileDialog
$pack=New-Object System.Windows.Forms.SaveFileDialog
try {
    $game.Title='Choose your prepared Skate 3 Rust Engine'
    $game.Filter='Skate 3 Rust Engine (skate3rust.exe)|skate3rust.exe'
    $game.CheckFileExists=$true
    $game.RestoreDirectory=$true
    if($game.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK){exit 0}
    $pack.Title='Save your private Skate content pack'
    $pack.Filter='CSGO Rewrite content pack (*.csrpack)|*.csrpack'
    $pack.FileName='Skate-3.csrpack'
    $pack.RestoreDirectory=$true
    if($pack.ShowDialog() -ne [System.Windows.Forms.DialogResult]::OK){exit 0}
    [Console]::Out.Write((ConvertTo-Json -InputObject @($game.FileName,$pack.FileName) -Compress))
} finally {$game.Dispose();$pack.Dispose()}
