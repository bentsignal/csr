//! Per-user `.csrpack` registration. Tests use a private HKCU subtree, never
//! the real extension or UserChoice. Microsoft registration/uninstall rules:
//! https://learn.microsoft.com/en-us/windows/win32/shell/fa-file-types
use std::path::Path;

const PROG_ID: &str = "CSGORewrite.Pack.1";
const CLASSES: &str = r"Software\Classes";

fn open_command(exe: &Path) -> Result<String, String> {
    let exe = exe
        .to_str()
        .ok_or("file association path must be Unicode")?;
    if !Path::new(exe).is_absolute() || exe.contains(['\0', '"', '\r', '\n']) {
        return Err("invalid file association executable path".into());
    }
    Ok(format!("\"{exe}\" --open-pack \"%1\""))
}

fn script(root: &str, exe: &Path, remove: bool) -> Result<String, String> {
    let command = open_command(exe)?;
    let mut script = format!(
        "$ErrorActionPreference='Stop'; $root=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey({}); \
         $id={}; $command={}; ",
        crate::dialog::ps_quote(root),
        crate::dialog::ps_quote(PROG_ID),
        crate::dialog::ps_quote(&command)
    );
    if remove {
        script += r#"
            $own=$root.OpenSubKey($id+'\shell\open\command');
            $owned=$false; if($own){$owned=($own.GetValue('') -eq $command);$own.Dispose()};
            if($owned){
                $root.DeleteSubKeyTree($id,$false);
                $list=$root.OpenSubKey('.csrpack\OpenWithProgids',$true);
                if($list){$list.DeleteValue($id,$false);$list.Dispose()};
            };
            $root.Dispose();
        "#;
        // Windows ignores a default ProgID once unregistered. Do not erase
        // extension ownership/UserChoice; another application may own them.
    } else {
        script += &format!(
            "$icon={}; ",
            crate::dialog::ps_quote(&format!("\"{}\",0", exe.display()))
        );
        script += r#"
            $type=$root.CreateSubKey($id);
            $type.SetValue('','CSGO Rewrite content pack',[Microsoft.Win32.RegistryValueKind]::String);
            $type.SetValue('FriendlyTypeName','CSGO Rewrite content pack',[Microsoft.Win32.RegistryValueKind]::String);
            $type.Dispose();
            $key=$root.CreateSubKey($id+'\DefaultIcon');$key.SetValue('',$icon);$key.Dispose();
            $key=$root.CreateSubKey($id+'\shell\open\command');$key.SetValue('',$command);$key.Dispose();
            $ext=$root.CreateSubKey('.csrpack');
            $current=$ext.GetValue('');
            if([string]::IsNullOrEmpty($current) -or $current -eq $id){$ext.SetValue('',$id)};
            $ext.Dispose();
            $list=$root.CreateSubKey('.csrpack\OpenWithProgids');$list.SetValue($id,'');$list.Dispose();
            $root.Dispose();
        "#;
    }
    Ok(script)
}

#[cfg(windows)]
fn run(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let output = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .creation_flags(0x0800_0000)
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(not(windows))]
fn run(_script: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn refresh_shell() {
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn SHChangeNotify(
            event: i32,
            flags: u32,
            first: *const std::ffi::c_void,
            second: *const std::ffi::c_void,
        );
    }
    // SAFETY: association change takes no item pointers (SHCNF_IDLIST).
    unsafe {
        SHChangeNotify(0x0800_0000, 0, std::ptr::null(), std::ptr::null());
    }
}
#[cfg(not(windows))]
fn refresh_shell() {}

pub fn register() {
    let result = crate::update::launch_exe()
        .ok_or("installed executable not found".into())
        .and_then(|exe| script(CLASSES, &exe, false))
        .and_then(|s| run(&s));
    match result {
        Ok(()) => refresh_shell(),
        Err(e) => eprintln!("Content pack file association: {e}"),
    }
}

pub fn remove() {
    if let Some(exe) = crate::update::launch_exe() {
        match script(CLASSES, &exe, true).and_then(|s| run(&s)) {
            Ok(()) => refresh_shell(),
            Err(e) => eprintln!("Removing content pack file association: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_and_filename_arguments_are_quoted_without_a_shell() {
        let path = std::env::temp_dir().join("O'Reilly & $Test/game.exe");
        assert_eq!(
            open_command(&path).unwrap(),
            format!("\"{}\" --open-pack \"%1\"", path.display())
        );
        assert!(open_command(Path::new("relative.exe")).is_err());
    }

    #[test]
    #[cfg(windows)]
    fn registration_preserves_other_handlers_and_uninstall_checks_command_ownership() {
        let root = format!(
            r"Software\CSGORewrite\AssociationTests\{}",
            std::process::id()
        );
        let old = std::env::temp_dir().join("Old CSGO/game.exe");
        let new = std::env::temp_dir().join("New CSGO/game.exe");
        // An exclusive new subtree is essential: do not delete earlier state.
        run(&format!("$r=[Microsoft.Win32.Registry]::CurrentUser; if($r.OpenSubKey({})){{throw 'test key already exists'}}; $k=$r.CreateSubKey({}); $k.Dispose()", crate::dialog::ps_quote(&root), crate::dialog::ps_quote(&root))).unwrap();
        let result = std::panic::catch_unwind(|| {
            run(&format!("$r=[Microsoft.Win32.Registry]::CurrentUser.CreateSubKey({}); $e=$r.CreateSubKey('.csrpack'); $e.SetValue('','Other.Application'); $e.SetValue('OtherData','preserved'); $e.Dispose(); $r.Dispose()", crate::dialog::ps_quote(&root))).unwrap();
            run(&script(&root, &old, false).unwrap()).unwrap();
            run(&script(&root, &new, false).unwrap()).unwrap();
            // Old installed copies cannot unregister a newer copy's handler.
            run(&script(&root, &old, true).unwrap()).unwrap();
            run(&format!("$r=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey({}); $e=$r.OpenSubKey('.csrpack'); if($e.GetValue('') -ne 'Other.Application' -or $e.GetValue('OtherData') -ne 'preserved'){{throw 'other association changed'}}; $p=$r.OpenSubKey({}); if(!$p -or $p.GetValue('') -ne {}){{throw 'new handler removed'}}; $p.Dispose(); $e.Dispose(); $r.Dispose()", crate::dialog::ps_quote(&root), crate::dialog::ps_quote(&format!(r"{PROG_ID}\shell\open\command")), crate::dialog::ps_quote(&open_command(&new).unwrap()))).unwrap();
            run(&script(&root, &new, true).unwrap()).unwrap();
            run(&format!("$r=[Microsoft.Win32.Registry]::CurrentUser.OpenSubKey({}); if($r.OpenSubKey({})){{throw 'owned handler remains'}}; $e=$r.OpenSubKey('.csrpack'); if($e.GetValue('') -ne 'Other.Application' -or $e.GetValue('OtherData') -ne 'preserved'){{throw 'other data removed'}}; $e.Dispose(); $r.Dispose()", crate::dialog::ps_quote(&root), crate::dialog::ps_quote(PROG_ID))).unwrap();
        });
        run(&format!(
            "[Microsoft.Win32.Registry]::CurrentUser.DeleteSubKeyTree({},$false)",
            crate::dialog::ps_quote(&root)
        ))
        .unwrap();
        if let Err(e) = result {
            std::panic::resume_unwind(e);
        }
    }
}
