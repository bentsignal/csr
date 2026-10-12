//! Small Windows dialogs for setup (shown through PowerShell's Windows Forms,
//! which every Windows 10/11 PC has), with console fallbacks.

use std::process::{Child, Command, Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn powershell_command(script: &str) -> Command {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-STA", "-Command"]).arg(format!(
        "[Console]::InputEncoding=[Text.Encoding]::UTF8; [Console]::OutputEncoding=[Text.Encoding]::UTF8; Add-Type -AssemblyName System.Windows.Forms; \
         Add-Type -AssemblyName System.Drawing; [System.Windows.Forms.Application]::EnableVisualStyles(); {script}"
    ));
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Runs a PowerShell snippet and returns its output.
pub fn powershell(script: &str) -> Option<String> {
    let out = powershell_command(script).stdin(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn ps_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// A plain message with an OK button.
pub fn message(title: &str, text: &str) {
    if choose(title, text, &["OK"]).is_none() {
        eprintln!("{title}: {text}");
    }
}

/// PowerShell building a dialog window `$f` with our icon, a message and
/// one button per entry of `buttons` (the first is the default). Clicking a
/// button stores its index in `$f.Tag` and closes the window.
fn form_script(title: &str, text: &str, buttons: &[&str]) -> String {
    let icon = std::env::current_exe()
        .ok()
        .map(|exe| format!("try{{$f.Icon=[System.Drawing.Icon]::ExtractAssociatedIcon({})}}catch{{}};", ps_quote(&exe.display().to_string())))
        .unwrap_or_default();
    let mut s = format!(
        "$f=New-Object System.Windows.Forms.Form; $f.Text={title}; $f.TopMost=$true; $f.StartPosition='CenterScreen'; \
         $f.FormBorderStyle='FixedDialog'; $f.MaximizeBox=$false; $f.MinimizeBox=$false; $f.ShowInTaskbar=$true; \
         $f.AutoSize=$true; $f.AutoSizeMode='GrowAndShrink'; $f.Font=New-Object System.Drawing.Font('Segoe UI',10); {icon} \
         $p=New-Object System.Windows.Forms.FlowLayoutPanel; $p.FlowDirection='TopDown'; $p.AutoSize=$true; \
         $p.Padding=New-Object System.Windows.Forms.Padding(18); \
         $l=New-Object System.Windows.Forms.Label; $l.Text={text}; $l.AutoSize=$true; \
         $l.MaximumSize=New-Object System.Drawing.Size(520,0); $l.Margin=New-Object System.Windows.Forms.Padding(0,0,0,18); \
         $p.Controls.Add($l); \
         $r=New-Object System.Windows.Forms.FlowLayoutPanel; $r.AutoSize=$true; $r.FlowDirection='LeftToRight'; \
         $f.Tag=-1; ",
        title = ps_quote(title),
        text = ps_quote(text),
    );
    for (i, b) in buttons.iter().enumerate() {
        s += &format!(
            "$b=New-Object System.Windows.Forms.Button; $b.Text={}; $b.AutoSize=$true; \
             $b.Padding=New-Object System.Windows.Forms.Padding(10,4,10,4); $b.Tag={i}; \
             $b.Add_Click({{$f.Tag=$this.Tag; $f.Close()}}); $r.Controls.Add($b); ",
            ps_quote(b)
        );
        if i == 0 {
            s += "$f.AcceptButton=$b; ";
        }
    }
    s + "$p.Controls.Add($r); $f.Controls.Add($p); "
}

/// Shows `text` with a button for each choice. Returns the index of the
/// button pressed, or `None` if the window was closed.
pub fn choose(title: &str, text: &str, buttons: &[&str]) -> Option<usize> {
    let script = form_script(title, text, buttons) + "[void]$f.ShowDialog(); [Console]::Out.Write($f.Tag)";
    match powershell(&script) {
        Some(out) => out.trim().parse::<usize>().ok().filter(|&i| i < buttons.len()),
        None => {
            // No PowerShell: ask on the console.
            eprintln!("\n{title}\n\n{text}\n");
            for (i, b) in buttons.iter().enumerate() {
                eprintln!("  {}. {b}", i + 1);
            }
            eprint!("> ");
            let mut line = String::new();
            std::io::stdin().read_line(&mut line).ok()?;
            line.trim().parse::<usize>().ok().filter(|&i| (1..=buttons.len()).contains(&i)).map(|i| i - 1)
        }
    }
}

/// A window that stays up while the caller works; the player can close it
/// with its button. Drop or [`Waiting::close`] to take it down.
pub struct Waiting(Option<Child>);

impl Waiting {
    pub fn show(title: &str, text: &str, button: &str) -> Waiting {
        let script = form_script(title, text, &[button]) + "[void]$f.ShowDialog()";
        Waiting(powershell_command(&script).stdin(Stdio::null()).stdout(Stdio::null()).spawn().ok())
    }

    /// A worker owns the actual installation; stdin carries plain status text,
    /// never executable script. A runspace reads the pipe without blocking the
    /// form's event thread. Closing the form is the caller's cancellation signal.
    pub fn progress(title: &str, text: &str) -> Waiting {
        let script = progress_script(title, text);
        Waiting(powershell_command(&script).stdin(Stdio::piped()).stdout(Stdio::null()).spawn().ok())
    }

    pub fn status(&mut self, text: &str) {
        use std::io::Write;
        if let Some(input) = self.0.as_mut().and_then(|c| c.stdin.as_mut()) {
            let _ = writeln!(input, "{}", text.replace(['\r', '\n'], " "));
        }
    }

    /// True once the player has closed the window.
    pub fn dismissed(&mut self) -> bool {
        self.0.as_mut().is_some_and(|c| c.try_wait().ok().flatten().is_some())
    }

    pub fn close(mut self) {
        if let Some(mut c) = self.0.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

fn progress_script(title: &str, text: &str) -> String {
    form_script(title, text, &["Cancel"]) + r#"
            $state=[hashtable]::Synchronized(@{Text=$l.Text});
            $reader=[powershell]::Create();
            [void]$reader.AddScript({param($state)
                while($null -ne ($line=[Console]::In.ReadLine())){$state.Text=$line}
            }).AddArgument($state);
            $async=$reader.BeginInvoke();
            $timer=New-Object System.Windows.Forms.Timer;$timer.Interval=100;
            $timer.Add_Tick({$l.Text=$state.Text});$timer.Start();
            [void]$f.ShowDialog();$timer.Stop();[Environment]::Exit(0);
        "#
}

impl Drop for Waiting {
    fn drop(&mut self) {
        if let Some(mut c) = self.0.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

/// A Windows notification that does not interrupt the game (it waits in
/// the notification centre during full-screen play).
pub fn notify(title: &str, text: &str) {
    let script = format!(
        "$n=New-Object System.Windows.Forms.NotifyIcon; \
         try{{$n.Icon=[System.Drawing.Icon]::ExtractAssociatedIcon({exe})}}catch{{$n.Icon=[System.Drawing.SystemIcons]::Information}}; \
         $n.Visible=$true; $n.ShowBalloonTip(10000,{title},{text},'Info'); Start-Sleep -Seconds 12; $n.Dispose()",
        exe = ps_quote(&std::env::current_exe().map(|e| e.display().to_string()).unwrap_or_default()),
        title = ps_quote(title),
        text = ps_quote(text),
    );
    eprintln!("{title}: {text}");
    let _ = powershell_command(&script).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
}

/// Opens a URL (`steam://` links included) with the system handler.
pub fn open_url(url: &str) {
    let mut cmd = Command::new("cmd");
    cmd.args(["/C", "start", "", url]);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    if cmd.spawn().is_err() {
        eprintln!("Open this link to continue: {url}");
    }
}

/// Hides the console window when it belongs to this process alone (the game
/// was started from a shortcut, not from a terminal).
pub fn hide_own_console() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetConsoleWindow() -> isize;
            fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
        }
        #[link(name = "user32")]
        unsafe extern "system" {
            fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        }
        // SAFETY: plain Win32 calls with a valid buffer.
        unsafe {
            let mut pids = [0u32; 4];
            let window = GetConsoleWindow();
            if window != 0 && GetConsoleProcessList(pids.as_mut_ptr(), pids.len() as u32) == 1 {
                ShowWindow(window, 0);
            }
        }
    }
}

/// Draws the setup dialogs to PNGs for review:
/// `CSR_DIALOG_PREVIEW=<folder> cargo test -p launcher render_dialogs -- --ignored`.
#[cfg(test)]
mod preview {
    #[test]
    #[ignore = "visible private UI proof; set CSR_DIALOG_PREVIEW outside Git"]
    fn render_pack_progress() {
        use std::io::Write;
        use std::process::Stdio;
        use std::time::{Duration, Instant};
        let out = std::env::var("CSR_DIALOG_PREVIEW").unwrap();
        let path = std::path::Path::new(&out).join("pack-progress.png");
        // Exercise the real pipe reader and real form. The timer closes only
        // after the status delivered as plain stdin text reaches the label.
        let capture = format!(r#"
            $capture=New-Object System.Windows.Forms.Timer;$capture.Interval=100;
            $capture.Add_Tick({{
                if($l.Text -eq 'Installing the pack… 43%'){{
                    $capture.Stop();$bmp=New-Object System.Drawing.Bitmap($f.Width,$f.Height);
                    $f.DrawToBitmap($bmp,(New-Object System.Drawing.Rectangle(0,0,$f.Width,$f.Height)));
                    $bmp.Save({});$bmp.Dispose();$f.Close();
                }}
            }});$capture.Start();[void]$f.ShowDialog()
        "#, super::ps_quote(&path.display().to_string()));
        let script = super::progress_script("Install content pack", "Checking the pack…")
            .replace("[void]$f.ShowDialog()", &capture);
        let mut child = super::powershell_command(&script).stdin(Stdio::piped()).spawn().unwrap();
        // Match the production protocol's UTF-8 status line.
        writeln!(child.stdin.as_mut().unwrap(), "Installing the pack… 43%").unwrap();
        let deadline = Instant::now() + Duration::from_secs(15);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() { break Some(status); }
            if Instant::now() >= deadline { child.kill().unwrap(); child.wait().unwrap(); break None; }
            std::thread::sleep(Duration::from_millis(20));
        };
        assert!(status.is_some_and(|s| s.success()), "progress reader/form failed or did not close");
        assert!(path.exists(), "status never reached the form");
    }
    #[test]
    #[ignore]
    fn render_dialogs() {
        let out = std::env::var("CSR_DIALOG_PREVIEW").unwrap();
        let shots: [(&str, &str, &[&str]); 3] = [
            ("missing", "CSGO Rewrite plays using the game files of CS:GO Legacy, which is free on Steam. In your Steam library it is called \"Counter-Strike: Global Offensive\".\n\nIt is not installed on this PC yet.", &["Install it with Steam", "It is installed somewhere else", "Quit"]),
            ("steam-running", "Add CSGO Rewrite to your Steam library?\n\nIt shows up with your other games, with its own artwork, so you can start it from Steam.\n\nSteam is open and has to restart to show it. CSGO Rewrite closes Steam, adds the game and opens Steam again. If you are in a Steam call or chat, pick \"Not now\" and you will be asked again next time.", &["Add and restart Steam", "Not now", "No thanks"]),
            ("waiting", "Steam is opening the installer for CS:GO Legacy. Click Install there.\n\nCSGO Rewrite starts by itself as soon as the download has finished, so you can leave this window open. It is a big download and can take a while.", &["Quit"]),
        ];
        for (name, text, buttons) in shots {
            let path = format!("{out}\\{name}.png");
            let script = super::form_script("CSGO Rewrite", text, buttons)
                + &format!("$f.Show(); [System.Windows.Forms.Application]::DoEvents(); $bmp=New-Object System.Drawing.Bitmap($f.Width,$f.Height); $f.DrawToBitmap($bmp,(New-Object System.Drawing.Rectangle(0,0,$f.Width,$f.Height))); $bmp.Save({}); $f.Close()", super::ps_quote(&path));
            super::powershell(&script).unwrap();
        }
    }
}
