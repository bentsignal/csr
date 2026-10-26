param([Parameter(Mandatory)][string]$StateFile, [string]$Preview = '', [string]$IconFile = '')
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
[System.Windows.Forms.Application]::EnableVisualStyles()

$bg = [Drawing.ColorTranslator]::FromHtml('#11161d')
$card = [Drawing.ColorTranslator]::FromHtml('#1a222c')
$ink = [Drawing.ColorTranslator]::FromHtml('#edf2f7')
$muted = [Drawing.ColorTranslator]::FromHtml('#9aaaba')
$accent = [Drawing.ColorTranslator]::FromHtml('#62d9b0')
$script:rendering = $false
$script:packSignature = ''

function Send-Action([string]$Action, [string]$Value = '') {
    [Console]::Out.WriteLine((@{action=$Action; value=$Value} | ConvertTo-Json -Compress))
    [Console]::Out.Flush()
}
function Label($Parent, $Text, $X, $Y, $W, $H, $Size = 10, $Color = $ink) {
    $c = New-Object Windows.Forms.Label
    $c.Text=$Text; $c.UseMnemonic=$false; $c.Location=New-Object Drawing.Point($X,$Y); $c.Size=New-Object Drawing.Size($W,$H)
    $c.Font=New-Object Drawing.Font('Segoe UI',$Size); $c.ForeColor=$Color
    $Parent.Controls.Add($c); return $c
}
function Button($Parent, $Text, $X, $Y, $W, $H) {
    $c=New-Object Windows.Forms.Button
    $c.Text=$Text; $c.Location=New-Object Drawing.Point($X,$Y); $c.Size=New-Object Drawing.Size($W,$H)
    $c.FlatStyle='Flat'; $c.FlatAppearance.BorderColor=[Drawing.ColorTranslator]::FromHtml('#344555')
    $c.UseMnemonic=$false
    $c.BackColor=$card; $c.ForeColor=$ink; $c.Cursor='Hand'; $c.Font=New-Object Drawing.Font('Segoe UI',10)
    $Parent.Controls.Add($c); return $c
}
function Read-State {
    # Share reads/writes/deletion with Rust. A partial JSON read is retried on
    # the next timer tick; file contents are data and are never evaluated.
    $file=[IO.File]::Open($StateFile,'Open','Read',([IO.FileShare]::ReadWrite -bor [IO.FileShare]::Delete))
    $reader=New-Object IO.StreamReader($file,[Text.Encoding]::UTF8)
    try { return ($reader.ReadToEnd() | ConvertFrom-Json) } finally { $reader.Dispose() }
}

$form=New-Object Windows.Forms.Form
$form.Text='CSGO Rewrite Launcher'; $form.StartPosition='CenterScreen'
$form.ClientSize=New-Object Drawing.Size(980,680); $form.MinimumSize=New-Object Drawing.Size(996,719)
$form.BackColor=$bg; $form.ForeColor=$ink; $form.Font=New-Object Drawing.Font('Segoe UI',10)
$form.AutoScaleMode='Dpi'
if($IconFile){try{$form.Icon=[Drawing.Icon]::ExtractAssociatedIcon($IconFile)}catch{}}

$brand=Label $form 'CSGO REWRITE' 32 23 420 34 20
$tagline=Label $form 'Original Counter-Strike. Rebuilt in Rust.' 34 64 570 24 10 $muted
$trackLabel=Label $form 'RELEASE TRACK' 687 25 225 20 9 $muted
$track=New-Object Windows.Forms.ComboBox
$track.Location=New-Object Drawing.Point(688,49); $track.Size=New-Object Drawing.Size(258,30)
$track.DropDownStyle='DropDownList'; $track.FlatStyle='Flat'; $track.BackColor=$card; $track.ForeColor=$ink
[void]$track.Items.Add('Stable - recommended'); [void]$track.Items.Add('Dev - latest tested changes')
$form.Controls.Add($track)
$track.Add_SelectedIndexChanged({ if(-not $script:rendering){Send-Action 'track' $(if($track.SelectedIndex -eq 1){'dev'}else{'stable'})} })

$overviewTab=Button $form 'Overview' 32 112 118 35
$packsTab=Button $form 'Content packs' 160 112 150 35
$version=Label $form '' 500 121 446 24 10 $muted
$version.TextAlign='MiddleRight'; $version.Anchor='Top,Right'

$overview=New-Object Windows.Forms.Panel; $overview.Location=New-Object Drawing.Point(32,168)
$overview.Size=New-Object Drawing.Size(914,448); $overview.Anchor='Top,Bottom,Left,Right'; $form.Controls.Add($overview)
$status=Label $overview 'Checking for updates...' 0 0 910 62 12
$status.Anchor='Top,Left,Right'
$progress=New-Object Windows.Forms.ProgressBar; $progress.Location=New-Object Drawing.Point(0,70)
$progress.Size=New-Object Drawing.Size(914,6); $progress.Anchor='Top,Left,Right'; $overview.Controls.Add($progress)
$play=Button $overview 'Play' 0 97 242 48
$play.BackColor=$accent; $play.ForeColor=$bg; $play.FlatAppearance.BorderSize=0; $play.Font=New-Object Drawing.Font('Segoe UI',14,[Drawing.FontStyle]::Bold)
$play.Add_Click({Send-Action 'play'})
$form.AcceptButton=$play
$apply=Button $overview 'Apply update' 256 97 166 48; $apply.Visible=$false
$apply.Add_Click({Send-Action 'apply'})
$refresh=Button $overview 'Refresh' 782 97 132 48; $refresh.Anchor='Top,Right'
$refresh.Add_Click({Send-Action 'refresh'})
$notesHeading=Label $overview "WHAT'S NEW" 0 175 914 25 9 $muted
$notesPanel=New-Object Windows.Forms.Panel; $notesPanel.Location=New-Object Drawing.Point(0,208)
$notesPanel.Size=New-Object Drawing.Size(914,240); $notesPanel.Anchor='Top,Bottom,Left,Right'
$notesPanel.Padding=New-Object Windows.Forms.Padding(16); $notesPanel.BackColor=$card; $overview.Controls.Add($notesPanel)
$notes=New-Object Windows.Forms.TextBox; $notes.Multiline=$true; $notes.ReadOnly=$true
$notes.BorderStyle='None'; $notes.Dock='Fill'; $notes.ScrollBars='Vertical'; $notes.BackColor=$card
$notes.ForeColor=$ink; $notes.Font=New-Object Drawing.Font('Segoe UI',10.5); $notesPanel.Controls.Add($notes)

$packs=New-Object Windows.Forms.Panel; $packs.Location=$overview.Location; $packs.Size=$overview.Size
$packs.Anchor='Top,Bottom,Left,Right'; $packs.Visible=$false; $form.Controls.Add($packs)
$packInfo=Label $packs 'Install and manage your private .csrpack files. Game updates keep these files.' 0 0 914 32 10 $muted
$grid=New-Object Windows.Forms.DataGridView; $grid.Location=New-Object Drawing.Point(0,45)
$grid.Size=New-Object Drawing.Size(914,250); $grid.Anchor='Top,Bottom,Left,Right'
$grid.BackgroundColor=$card; $grid.BorderStyle='None'; $grid.ReadOnly=$true; $grid.MultiSelect=$false
$grid.AllowUserToAddRows=$false; $grid.AllowUserToDeleteRows=$false; $grid.AllowUserToResizeRows=$false
$grid.RowHeadersVisible=$false; $grid.SelectionMode='FullRowSelect'; $grid.AutoSizeColumnsMode='Fill'
$grid.EnableHeadersVisualStyles=$false; $grid.ColumnHeadersDefaultCellStyle.BackColor=$card
$grid.ColumnHeadersDefaultCellStyle.ForeColor=$muted; $grid.ColumnHeadersHeight=34
$grid.DefaultCellStyle.BackColor=$card; $grid.DefaultCellStyle.ForeColor=$ink
$grid.DefaultCellStyle.SelectionBackColor=[Drawing.ColorTranslator]::FromHtml('#30483f')
$grid.DefaultCellStyle.SelectionForeColor=$ink; $grid.RowTemplate.Height=34
$grid.GridColor=[Drawing.ColorTranslator]::FromHtml('#293642')
[void]$grid.Columns.Add('name','Content pack'); [void]$grid.Columns.Add('version','Version')
$grid.Columns[0].FillWeight=75; $grid.Columns[1].FillWeight=25; $packs.Controls.Add($grid)
$empty=Label $packs 'No content packs installed. Choose Install pack to add one.' 20 112 865 60 11 $muted
$install=Button $packs 'Install pack...' 0 315 155 42; $install.Anchor='Bottom,Left'
$install.Add_Click({
    $picker=New-Object Windows.Forms.OpenFileDialog; $picker.Filter='CSGO Rewrite content pack (*.csrpack)|*.csrpack'
    if($picker.ShowDialog($form) -eq 'OK'){Send-Action 'install' $picker.FileName}; $picker.Dispose()
})
$remove=Button $packs 'Remove selected' 169 315 170 42; $remove.Anchor='Bottom,Left'
$remove.Add_Click({
    if($grid.SelectedRows.Count -and [Windows.Forms.MessageBox]::Show($form,'Remove this installed content pack?','Content packs','YesNo','Question') -eq 'Yes'){
        Send-Action 'remove' ([string]$grid.SelectedRows[0].Tag)
    }
})
$folder=Button $packs 'Open packs folder' 723 315 191 42; $folder.Anchor='Bottom,Right'
$folder.Add_Click({Send-Action 'packs-folder'})
$packStatus=Label $packs '' 0 374 914 74 10 $muted; $packStatus.Anchor='Bottom,Left,Right'
$cancel=Button $form 'Cancel installation' 692 631 160 30; $cancel.Anchor='Bottom,Right'; $cancel.Visible=$false
$cancel.Add_Click({Send-Action 'cancel'})
$logs=Button $form 'Open logs' 858 631 88 30; $logs.Anchor='Bottom,Right'
$logs.Add_Click({Send-Action 'logs'})
$footer=Label $form 'Settings and content packs are kept when the game updates.' 32 638 640 24 9 $muted
$footer.Anchor='Bottom,Left'
$overviewTab.Add_Click({$overview.Visible=$true; $packs.Visible=$false})
$packsTab.Add_Click({$overview.Visible=$false; $packs.Visible=$true})

$timer=New-Object Windows.Forms.Timer; $timer.Interval=150
$script:previewTicks=0
$timer.Add_Tick({
    try {
        $s=Read-State; $script:rendering=$true
        $track.SelectedIndex=$(if($s.track -eq 'dev'){1}else{0})
        $track.Enabled=(-not $s.busy -and -not $s.ready)
        $status.Text=[string]$s.status
        $version.Text="v$($s.version)  /  $($s.build)"
        $play.Enabled=(-not $s.installing -and -not $s.running)
        $play.Text=$(if($s.running){'Game is running'}elseif($s.ready){'Update & play'}elseif($s.busy){'Play installed'}else{'Play'})
        $apply.Visible=[bool]$s.ready; $apply.Enabled=(-not $s.busy -and -not $s.running)
        $refresh.Enabled=(-not $s.busy -and -not $s.ready)
        if($s.progress -lt 0){$progress.Style='Marquee'}else{$progress.Style='Continuous'; $progress.Value=[Math]::Min(100,[Math]::Max(0,[int]$s.progress))}
        $progress.Visible=[bool]$s.busy
        $nextNotes=([string]$s.notes).Replace("`r`n","`n").Replace("`n","`r`n")
        if($notes.Text -ne $nextNotes){$notes.Text=$nextNotes}
        $install.Enabled=(-not $s.busy -and -not $s.running)
        $remove.Enabled=($install.Enabled -and @($s.packs).Count -gt 0)
        $cancel.Visible=[bool]$s.installing
        $packStatus.Text=([string]$s.status + $(if($s.pack_errors){"`r`n"+[string]$s.pack_errors}else{''}))
        $signature=ConvertTo-Json -InputObject @($s.packs) -Compress
        if($signature -ne $script:packSignature){
            $grid.Rows.Clear()
            foreach($p in @($s.packs)){
                $index=$grid.Rows.Add([string]$p.name,[string]$p.version); $grid.Rows[$index].Tag=[string]$p.id
            }
            $script:packSignature=$signature
        }
        $empty.Visible=(@($s.packs).Count -eq 0)
        if($Preview){
            $script:previewTicks++
            if($script:previewTicks -ge 12){
                $bitmap=New-Object Drawing.Bitmap($form.Width,$form.Height)
                try{$form.DrawToBitmap($bitmap,(New-Object Drawing.Rectangle(0,0,$form.Width,$form.Height))); $bitmap.Save($Preview)}finally{$bitmap.Dispose()}
                $form.Close()
            }
        }
    } catch {
        # State may be between writes; keep the last complete view and retry.
        [Console]::Error.WriteLine($_.Exception.Message)
    } finally { $script:rendering=$false }
})
$timer.Start()
try {[void]$form.ShowDialog()} finally {$timer.Stop(); $timer.Dispose(); $form.Dispose()}
