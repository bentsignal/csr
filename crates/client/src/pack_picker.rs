//! One owned native picker job. Selection does not install or trust the pack.
//! The menu polls this job and passes a selected path to its install worker.

use std::path::PathBuf;
#[cfg(windows)]
use std::{
    io::{Read, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
};

const MAX_RESPONSE: usize = 128 * 1024;

// Fixed code only: paths travel back as raw UTF-16LE hex, never as shell code.
// Wait for our acknowledgement before opening UI: EOF on an early owner exit
// prevents a child from opening a dialog before it joins the owned job object.
const PICKER_SCRIPT: &str = r#"
$ErrorActionPreference='Stop';
function Hex([string]$text) {
    $chars=$text.ToCharArray(); $bytes=New-Object byte[] ($chars.Length*2);
    [Buffer]::BlockCopy($chars,0,$bytes,0,$bytes.Length);
    return [BitConverter]::ToString($bytes).Replace('-','');
}
try {
    if([Console]::ReadLine() -ne 'OPEN'){exit 0}
    Add-Type -AssemblyName System.Windows.Forms;
    [System.Windows.Forms.Application]::EnableVisualStyles();
    $d=New-Object System.Windows.Forms.OpenFileDialog;
    $top=$null;
    try {
        $top=New-Object System.Windows.Forms.Form -Property @{TopMost=$true;ShowInTaskbar=$false};
        $d.Title='Install a content pack';
        $d.Filter='Content packs (*.csrpack)|*.csrpack';
        $d.Multiselect=$false; $d.CheckFileExists=$true;
        $d.CheckPathExists=$true; $d.RestoreDirectory=$true;
        if($d.ShowDialog($top) -eq [System.Windows.Forms.DialogResult]::OK) {
            [Console]::Out.Write('P:'+(Hex $d.FileName));
        } else { [Console]::Out.Write('C'); }
    } finally { if($null -ne $top){$top.Dispose()}; $d.Dispose(); }
} catch { [Console]::Out.Write('E:'+(Hex $_.Exception.Message)); }
"#;

#[derive(Debug, PartialEq, Eq)]
pub enum PickerOutcome {
    Selected(PathBuf),
    Cancelled,
    Failed(String),
}

/// Keep one job in the menu. Dropping it closes only its own child; Windows
/// also closes the non-inherited job handle if the game exits without Drop.
pub struct PackPicker {
    pending: Option<PickerOutcome>,
    finished: bool,
    #[cfg(windows)]
    child: Option<Child>,
    #[cfg(windows)]
    job: Option<owned_job::Job>,
    #[cfg(windows)]
    response: Option<mpsc::Receiver<Result<Vec<u8>, String>>>,
    #[cfg(windows)]
    bytes: Option<Vec<u8>>,
}

impl PackPicker {
    pub fn start() -> Self {
        #[cfg(windows)]
        {
            Self::start_script(PICKER_SCRIPT)
        }
        #[cfg(not(windows))]
        {
            Self {
                pending: Some(PickerOutcome::Failed(
                    "The native content-pack picker requires Windows.".into(),
                )),
                finished: false,
            }
        }
    }

    #[cfg(windows)]
    fn start_script(script: &str) -> Self {
        use std::os::windows::process::CommandExt;
        let mut this = Self {
            pending: None,
            finished: false,
            child: None,
            job: None,
            response: None,
            bytes: None,
        };
        let result = (|| -> Result<(), String> {
            let job = owned_job::Job::new()
                .map_err(|e| format!("Cannot own the content-pack picker: {e}"))?;
            let mut command = Command::new("powershell.exe");
            command
                .args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script])
                .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            let child = command
                .spawn()
                .map_err(|e| format!("Cannot open the content-pack picker: {e}"))?;
            this.child = Some(child);
            this.job = Some(job);
            let child = this.child.as_mut().unwrap();
            this.job
                .as_ref()
                .unwrap()
                .assign(child)
                .map_err(|e| format!("Cannot own the content-pack picker process: {e}"))?;
            let stdout = child.stdout.take().ok_or("Picker output is unavailable")?;
            let (send, receive) = mpsc::channel();
            std::thread::Builder::new()
                .name("pack-picker-output".into())
                .spawn(move || {
                    let _ = send.send(read_response(stdout));
                })
                .map_err(|e| format!("Cannot read the content-pack picker: {e}"))?;
            this.response = Some(receive);
            // Job assignment precedes permission to open the dialog.
            child
                .stdin
                .take()
                .ok_or("Picker input is unavailable")?
                .write_all(b"OPEN\n")
                .map_err(|e| format!("Cannot start the content-pack picker: {e}"))?;
            Ok(())
        })();
        if let Err(error) = result {
            this.stop();
            this.pending = Some(PickerOutcome::Failed(error));
        }
        this
    }

    /// Nonblocking; returns the terminal outcome exactly once.
    pub fn poll(&mut self) -> Option<PickerOutcome> {
        if self.finished {
            return None;
        }
        #[cfg(windows)]
        if self.pending.is_none() {
            if let Some(receive) = self.response.as_ref() {
                match receive.try_recv() {
                    Ok(Ok(bytes)) => {
                        self.bytes = Some(bytes);
                        self.response = None;
                    }
                    Ok(Err(error)) => {
                        self.stop();
                        self.pending = Some(PickerOutcome::Failed(error));
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.stop();
                        self.pending = Some(PickerOutcome::Failed(
                            "The content-pack picker stopped without a response.".into(),
                        ));
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            if let Some(child) = self.child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) if !status.success() => {
                        self.stop();
                        self.pending = Some(PickerOutcome::Failed(
                            "The content-pack picker exited unexpectedly.".into(),
                        ));
                    }
                    Ok(Some(_)) if self.bytes.is_some() => {
                        self.pending = Some(parse_response(&self.bytes.take().unwrap()));
                        self.stop();
                    }
                    Err(error) => {
                        self.stop();
                        self.pending = Some(PickerOutcome::Failed(format!(
                            "Cannot check the content-pack picker: {error}"
                        )));
                    }
                    _ => {}
                }
            }
        }
        let outcome = self.pending.take();
        if outcome.is_some() {
            self.finished = true;
        }
        outcome
    }

    pub fn cancel(&mut self) {
        if !self.finished && self.pending.is_none() {
            #[cfg(windows)]
            self.stop();
            self.pending = Some(PickerOutcome::Cancelled);
        }
    }

    #[cfg(windows)]
    fn stop(&mut self) {
        // Closing the owned job also terminates any descendants before wait.
        self.job.take();
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.response = None;
        self.bytes = None;
    }
}

impl Drop for PackPicker {
    fn drop(&mut self) {
        #[cfg(windows)]
        self.stop();
    }
}

#[cfg(windows)]
fn read_response(mut reader: impl Read) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let count = reader
            .read(&mut chunk)
            .map_err(|e| format!("Cannot read picker output: {e}"))?;
        if count == 0 {
            return Ok(bytes);
        }
        if bytes.len() + count > MAX_RESPONSE {
            return Err("The content-pack picker response is too large.".into());
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
}

fn parse_response(bytes: &[u8]) -> PickerOutcome {
    let invalid =
        || PickerOutcome::Failed("The content-pack picker returned an invalid response.".into());
    if bytes == b"C" {
        return PickerOutcome::Cancelled;
    }
    if bytes.len() > MAX_RESPONSE
        || bytes.len() < 6
        || bytes[1] != b':'
        || !matches!(bytes[0], b'P' | b'E')
    {
        return invalid();
    }
    let hex = &bytes[2..];
    if !hex.len().is_multiple_of(4) {
        return invalid();
    }
    let mut units = Vec::with_capacity(hex.len() / 4);
    for word in hex.chunks_exact(4) {
        let mut pair = [0u8; 2];
        for (i, digits) in word.chunks_exact(2).enumerate() {
            let Some(high) = (digits[0] as char).to_digit(16) else {
                return invalid();
            };
            let Some(low) = (digits[1] as char).to_digit(16) else {
                return invalid();
            };
            pair[i] = (high * 16 + low) as u8;
        }
        units.push(u16::from_le_bytes(pair));
    }
    if units.contains(&0) {
        return invalid();
    }
    if bytes[0] == b'E' {
        return PickerOutcome::Failed(format!(
            "Cannot open the content-pack picker: {}",
            String::from_utf16_lossy(&units)
        ));
    }
    #[cfg(windows)]
    let path = {
        use std::os::windows::ffi::OsStringExt;
        PathBuf::from(std::ffi::OsString::from_wide(&units))
    };
    #[cfg(not(windows))]
    let path = match String::from_utf16(&units) {
        Ok(text) => PathBuf::from(text),
        Err(_) => return invalid(),
    };
    if !path.is_absolute() {
        return invalid();
    }
    PickerOutcome::Selected(path)
}

#[cfg(windows)]
mod owned_job {
    use std::{
        ffi::c_void,
        io,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::Child,
    };
    // Public Win32 jobapi2/winnt ABI. No external assets or runtime helpers.
    // https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_extended_limit_information
    #[repr(C)]
    #[derive(Default)]
    struct BasicLimits {
        process_time: i64,
        job_time: i64,
        flags: u32,
        min_working: usize,
        max_working: usize,
        active: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct ExtendedLimits {
        basic: BasicLimits,
        io: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process: usize,
        peak_job: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            job: *mut c_void,
            class: i32,
            info: *const c_void,
            size: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    }
    pub(super) struct Job(OwnedHandle);
    impl Job {
        pub(super) fn new() -> io::Result<Self> {
            // Null security attributes make the unnamed handle non-inheritable.
            let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
            let mut limits = ExtendedLimits::default();
            limits.basic.flags = 0x2000; // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            let okay = unsafe {
                SetInformationJobObject(
                    handle.as_raw_handle(),
                    9,
                    (&limits as *const ExtendedLimits).cast(),
                    std::mem::size_of::<ExtendedLimits>() as u32,
                )
            };
            if okay == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(Self(handle))
        }
        pub(super) fn assign(&self, child: &Child) -> io::Result<()> {
            let okay =
                unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), child.as_raw_handle()) };
            if okay == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn response(tag: u8, text: &str) -> Vec<u8> {
        let mut output = vec![tag, b':'];
        for unit in text.encode_utf16() {
            for byte in unit.to_le_bytes() {
                output.extend_from_slice(format!("{byte:02X}").as_bytes());
            }
        }
        output
    }
    #[test]
    fn literal_unicode_selection_and_cancel_error_protocol() {
        let text = if cfg!(windows) {
            "C:\\用户\\friend's & $pack % 😀.csrpack"
        } else {
            "/用户/friend's & $pack % 😀.csrpack"
        };
        assert_eq!(
            parse_response(&response(b'P', text)),
            PickerOutcome::Selected(PathBuf::from(text))
        );
        assert_eq!(parse_response(b"C"), PickerOutcome::Cancelled);
        assert_eq!(
            parse_response(&response(b'E', "错误")),
            PickerOutcome::Failed("Cannot open the content-pack picker: 错误".into())
        );
    }
    #[test]
    fn malformed_relative_nul_and_oversized_are_errors() {
        for bytes in [
            b"".to_vec(),
            b"C\n".to_vec(),
            b"P:ZZZZ".to_vec(),
            b"P:410".to_vec(),
            response(b'P', "relative.csrpack"),
            response(b'P', "C:\\bad\0.csrpack"),
            vec![b'P'; MAX_RESPONSE + 1],
        ] {
            assert!(matches!(parse_response(&bytes), PickerOutcome::Failed(_)));
        }
    }
    #[test]
    fn fixed_script_is_single_file_and_waits_for_ownership() {
        assert!(PICKER_SCRIPT.contains("ReadLine() -ne 'OPEN'"));
        assert!(PICKER_SCRIPT.contains("Content packs (*.csrpack)|*.csrpack"));
        assert!(PICKER_SCRIPT.contains("$d.Multiselect=$false"));
        assert!(PICKER_SCRIPT.contains("$d.RestoreDirectory=$true"));
        assert!(PICKER_SCRIPT.contains("TopMost=$true;ShowInTaskbar=$false"));
        assert!(PICKER_SCRIPT.contains("$top.Dispose()"));
        assert!(PICKER_SCRIPT.contains("[Buffer]::BlockCopy"));
        assert!(!PICKER_SCRIPT.contains("Invoke-Expression"));
    }
    #[cfg(windows)]
    fn wait(job: &mut PackPicker) -> PickerOutcome {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            if let Some(result) = job.poll() {
                return result;
            }
            assert!(std::time::Instant::now() < end);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    #[cfg(windows)]
    #[test]
    fn owned_hidden_helper_delivers_one_result_and_cancels() {
        // No dialog or real player data touched.
        let mut job = PackPicker::start_script(
            "if([Console]::ReadLine() -eq 'OPEN'){[Console]::Out.Write('C')}",
        );
        assert_eq!(wait(&mut job), PickerOutcome::Cancelled);
        assert_eq!(job.poll(), None);
        let mut job = PackPicker::start_script(
            "if([Console]::ReadLine() -eq 'OPEN'){Start-Sleep -Seconds 60}",
        );
        assert!(job.child.is_some(), "{:?}", job.pending);
        job.cancel();
        assert!(job.child.is_none());
        assert_eq!(job.poll(), Some(PickerOutcome::Cancelled));
        assert_eq!(job.poll(), None);
    }
    #[cfg(windows)]
    #[test]
    fn real_script_protocol_without_opening_a_dialog() {
        let cancel = PICKER_SCRIPT.replace(
            "$d.ShowDialog($top)",
            "[System.Windows.Forms.DialogResult]::Cancel",
        );
        assert_eq!(
            wait(&mut PackPicker::start_script(&cancel)),
            PickerOutcome::Cancelled
        );
        let path = "C:\\用户\\friend's & $pack % 😀.csrpack";
        let select = PICKER_SCRIPT
            .replace(
                "$d.ShowDialog($top)",
                "[System.Windows.Forms.DialogResult]::OK",
            )
            .replace("$d.FileName", &format!("'{}'", path.replace('\'', "''")));
        assert_eq!(
            wait(&mut PackPicker::start_script(&select)),
            PickerOutcome::Selected(PathBuf::from(path))
        );
    }
    #[cfg(windows)]
    #[test]
    fn oversized_child_output_fails_and_closes_the_owned_process() {
        let mut job = PackPicker::start_script(
            "if([Console]::ReadLine() -eq 'OPEN'){[Console]::Out.Write('X'*131073);Start-Sleep -Seconds 60}",
        );
        assert!(
            matches!(wait(&mut job), PickerOutcome::Failed(message) if message.contains("too large"))
        );
        assert!(job.child.is_none());
        assert_eq!(job.poll(), None);
        let mut job = PackPicker::start_script("if([Console]::ReadLine() -eq 'OPEN'){exit 7}");
        assert!(matches!(wait(&mut job), PickerOutcome::Failed(_)));
        assert!(job.child.is_none());
    }
    #[cfg(windows)]
    #[test]
    fn picker_drop_kills_only_its_owned_helper() {
        let mut job = PackPicker::start_script(
            "if([Console]::ReadLine() -eq 'OPEN'){Start-Sleep -Seconds 60}",
        );
        let mut child = job.child.take().expect("owned helper spawned");
        assert!(child.try_wait().unwrap().is_none());
        drop(job);
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            assert!(
                std::time::Instant::now() < end,
                "owned helper survived picker drop"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    #[cfg(windows)]
    #[test]
    fn closing_job_handle_kills_owned_helper_even_without_picker_drop() {
        let mut job = PackPicker::start_script(
            "if([Console]::ReadLine() -eq 'OPEN'){Start-Sleep -Seconds 60}",
        );
        let mut child = job.child.take().expect("owned helper spawned");
        assert!(child.try_wait().unwrap().is_none());
        job.job.take(); // Same kernel action as process exit; intentionally bypass stop.
        let end = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while child.try_wait().unwrap().is_none() {
            assert!(
                std::time::Instant::now() < end,
                "owned helper survived job close"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    #[cfg(windows)]
    #[test]
    fn bounded_reader_and_raw_utf16_surrogate_path() {
        assert!(read_response(std::io::Cursor::new(vec![b'X'; MAX_RESPONSE + 1])).is_err());
        assert_eq!(read_response(std::io::Cursor::new(b"C")).unwrap(), b"C");
        use std::os::windows::ffi::OsStrExt;
        let mut bytes = response(b'P', "C:\\");
        bytes.extend_from_slice(b"00D8");
        let PickerOutcome::Selected(path) = parse_response(&bytes) else {
            panic!("surrogate was lost")
        };
        assert_eq!(path.as_os_str().encode_wide().last(), Some(0xd800));
    }
}
