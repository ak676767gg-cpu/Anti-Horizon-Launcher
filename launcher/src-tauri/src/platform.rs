// What differs between Windows, macOS and Linux: where files live, how folders and links open, the Recycle Bin or
// Trash, this computer's memory, and closing a game.
use std::path::{Path, PathBuf};
use std::process::Command;

/// The user's home folder.
pub fn home() -> PathBuf {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Where apps keep their own files: %APPDATA% on Windows, ~/Library/Application Support on a Mac, and
/// $XDG_DATA_HOME (normally ~/.local/share) on Linux.
pub fn data_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home().join(".local").join("share"))
    }
}

/// The Downloads folder.
pub fn downloads() -> PathBuf {
    if cfg!(target_os = "linux") {
        // Linux desktops can rename it (Téléchargements, Descargas); xdg-user-dir knows what it is called here.
        if let Ok(out) = Command::new("xdg-user-dir").arg("DOWNLOAD").output() {
            let dir = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim());
            if out.status.success() && dir.is_absolute() && dir != home() {
                return dir;
            }
        }
    }
    home().join("Downloads")
}

/// The command that opens a file, folder or web link with whatever the system uses for it.
fn opener() -> Command {
    if cfg!(windows) {
        Command::new("explorer")
    } else if cfg!(target_os = "macos") {
        Command::new("open")
    } else {
        Command::new("xdg-open")
    }
}

/// Opens a folder in Explorer, Finder or the Linux file manager.
pub fn open(path: &Path) {
    let _ = opener().arg(path).spawn();
}

/// Opens a web link in the default browser.
pub fn open_link(url: &str) -> std::io::Result<()> {
    opener().arg(url).spawn().map(|_| ())
}

/// Shows a file selected in its folder.
pub fn reveal(file: &Path) {
    let _ = if cfg!(windows) {
        Command::new("explorer").arg(format!("/select,{}", file.display())).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(file).spawn()
    } else {
        // Linux file managers have no one way to select a file, so open the folder it is in.
        opener().arg(file.parent().unwrap_or(file)).spawn()
    };
}

/// This computer's memory in MB, or 0 if the system won't say.
pub fn memory_mb() -> u64 {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        // SAFETY: a zeroed MEMORYSTATUSEX with its length set is what GlobalMemoryStatusEx expects to fill.
        let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut status) } != 0 {
            return status.ullTotalPhys / (1024 * 1024);
        }
        0
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("sysctl")
            .args(["-n", "hw.memsize"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<u64>().ok())
            .map_or(0, |bytes| bytes / (1024 * 1024))
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        // /proc/meminfo: "MemTotal:       16318412 kB"
        std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|m| m.lines().find_map(|l| l.strip_prefix("MemTotal:").and_then(|v| v.trim().trim_end_matches("kB").trim().parse::<u64>().ok())))
            .map_or(0, |kb| kb / 1024)
    }
}

/// Into the Recycle Bin (Windows) or the Trash (macOS and Linux) rather than gone for good: an instance holds the
/// player's worlds.
#[cfg(windows)]
pub fn to_bin(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        SHFileOperationW, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, FO_DELETE, SHFILEOPSTRUCTW,
    };
    let mut from: Vec<u16> = path.as_os_str().encode_wide().collect();
    from.extend([0, 0]);
    // SAFETY: an all-zero SHFILEOPSTRUCTW is valid; `from` is double-null terminated and outlives the call.
    let code = unsafe {
        let mut op: SHFILEOPSTRUCTW = std::mem::zeroed();
        op.wFunc = FO_DELETE as _;
        op.pFrom = from.as_ptr();
        op.fFlags = (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_NOERRORUI | FOF_SILENT) as _;
        let code = SHFileOperationW(&mut op);
        if op.fAnyOperationsAborted != 0 { -1 } else { code }
    };
    if code == 0 {
        Ok(())
    } else {
        Err(format!("Windows couldn't move it to the Recycle Bin (code {code})."))
    }
}

#[cfg(not(windows))]
pub fn to_bin(path: &Path) -> Result<(), String> {
    #[allow(unused_mut)]
    let mut bin = trash::TrashContext::default();
    // Straight through the system, not by asking Finder, which would make macOS ask the player for permission first.
    #[cfg(target_os = "macos")]
    {
        use trash::macos::{DeleteMethod, TrashContextExtMacos};
        bin.set_delete_method(DeleteMethod::NsFileManager);
    }
    bin.delete(path).map_err(|err| format!("Couldn't move it to the Trash ({err})."))
}

/// Ends a game process and whatever it started.
pub fn kill(pid: u32) -> bool {
    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("taskkill");
        c.args(["/PID", &pid.to_string(), "/T", "/F"]);
        c
    } else {
        let mut c = Command::new("kill");
        c.args(["-KILL", &pid.to_string()]);
        c
    };
    crate::mc::no_window(&mut cmd);
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

/// The local time of day as (hours, minutes, seconds).
pub fn local_time() -> (u32, u32, u32) {
    #[cfg(windows)]
    {
        // SAFETY: GetLocalTime only writes the SYSTEMTIME it is given.
        let t = unsafe {
            let mut t: windows_sys::Win32::Foundation::SYSTEMTIME = std::mem::zeroed();
            windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut t);
            t
        };
        (t.wHour as u32, t.wMinute as u32, t.wSecond as u32)
    }
    #[cfg(unix)]
    {
        // SAFETY: localtime_r only writes the tm it is given; time(NULL) just returns the time.
        let t = unsafe {
            let now = libc::time(std::ptr::null_mut());
            let mut t: libc::tm = std::mem::zeroed();
            libc::localtime_r(&now, &mut t);
            t
        };
        (t.tm_hour as u32, t.tm_min as u32, t.tm_sec as u32)
    }
    #[cfg(not(any(windows, unix)))]
    {
        let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        ((s / 3600 % 24) as u32, (s / 60 % 60) as u32, (s % 60) as u32)
    }
}
