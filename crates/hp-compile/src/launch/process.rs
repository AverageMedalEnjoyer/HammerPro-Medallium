use std::path::{Path, PathBuf};

pub const GAME_EXE_NAMES: &[&str] = &["gmod.exe", "hl2.exe"];

pub const ENGINE_WINDOW_CLASS: &str = "Valve001";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameProcess {
    pub pid: u32,
    pub exe: PathBuf,
    pub window: Option<isize>,
}

impl GameProcess {
    pub fn is_ready(&self) -> bool {
        self.window.is_some()
    }
}

fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy().replace('\\', "/").to_lowercase();
    s.trim_end_matches('/').to_owned()
}

pub fn is_game_exe_of(exe: &Path, root: &Path) -> bool {
    let name_ok = exe
        .file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| GAME_EXE_NAMES.iter().any(|g| g.eq_ignore_ascii_case(n)));
    let root = path_key(root);
    if !name_ok || root.is_empty() {
        return false;
    }
    let exe = path_key(exe);
    exe.strip_prefix(&root).is_some_and(|rest| rest.starts_with('/'))
}

pub fn hijack_command_line(exe: &Path, args: &[String]) -> String {
    let mut out = crate::plan::quote_arg(&exe.to_string_lossy());
    for a in args {
        out.push(' ');
        out.push_str(&crate::plan::quote_arg(a));
    }
    out
}

pub fn find_game(root: &Path) -> Option<GameProcess> {
    let mut games = game_processes(root);
    games.sort_by_key(|g| !g.is_ready());
    games.into_iter().next()
}

pub fn game_processes(root: &Path) -> Vec<GameProcess> {
    imp::processes()
        .into_iter()
        .filter(|(_, exe)| is_game_exe_of(exe, root))
        .map(|(pid, exe)| GameProcess { pid, exe, window: imp::engine_window(pid) })
        .collect()
}

pub fn send_command_line(game: &GameProcess, command_line: &str) -> Result<(), String> {
    let window = game.window.ok_or_else(|| "the game window does not exist yet".to_owned())?;
    imp::send_copydata(window, command_line)
}

#[cfg(windows)]
mod imp {
    use std::path::PathBuf;

    use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM};
    use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowThreadProcessId, SMTO_ABORTIFHUNG, SMTO_NORMAL, SendMessageTimeoutW,
        WM_COPYDATA,
    };

    use super::{ENGINE_WINDOW_CLASS, GAME_EXE_NAMES};

    fn wide_to_string(buf: &[u16]) -> String {
        let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..end])
    }

    fn image_path(pid: u32) -> Option<PathBuf> {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 4096];
        let mut len = u32::try_from(buf.len()).unwrap_or(0);
        let ok = unsafe { QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len) };
        unsafe { CloseHandle(handle) };
        if ok == 0 {
            return None;
        }
        let len = usize::try_from(len).ok()?.min(buf.len());
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
    }

    pub(super) fn processes() -> Vec<(u32, PathBuf)> {
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot.is_null() || snapshot == INVALID_HANDLE_VALUE {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut entry = PROCESSENTRY32W {
            dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>()).unwrap_or(0),
            ..PROCESSENTRY32W::default()
        };
        let mut more = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
        let mut guard = 0;
        while more && guard < 65_536 {
            guard += 1;
            let name = wide_to_string(&entry.szExeFile);
            if GAME_EXE_NAMES.iter().any(|g| g.eq_ignore_ascii_case(&name))
                && let Some(path) = image_path(entry.th32ProcessID)
            {
                out.push((entry.th32ProcessID, path));
            }
            more = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
        }
        unsafe { CloseHandle(snapshot) };
        out
    }

    struct Search {
        pid: u32,
        found: HWND,
    }

    unsafe extern "system" fn visit(window: HWND, param: LPARAM) -> windows_sys::core::BOOL {
        let search = unsafe { &mut *(param as *mut Search) };
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        if pid != search.pid {
            return 1;
        }
        let mut class = [0u16; 64];
        let n = unsafe { GetClassNameW(window, class.as_mut_ptr(), 64) };
        let n = usize::try_from(n).unwrap_or(0).min(class.len());
        if String::from_utf16_lossy(&class[..n]) == ENGINE_WINDOW_CLASS {
            search.found = window;
            return 0;
        }
        1
    }

    pub(super) fn engine_window(pid: u32) -> Option<isize> {
        let mut search = Search { pid, found: std::ptr::null_mut() };
        unsafe { EnumWindows(Some(visit), &mut search as *mut Search as LPARAM) };
        (!search.found.is_null()).then_some(search.found as isize)
    }

    pub(super) fn send_copydata(window: isize, text: &str) -> Result<(), String> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        let data = COPYDATASTRUCT {
            dwData: 0,
            cbData: u32::try_from(bytes.len()).map_err(|_| "command line too long".to_owned())?,
            lpData: bytes.as_mut_ptr().cast(),
        };
        let mut result = 0usize;
        let sent = unsafe {
            SendMessageTimeoutW(
                window as HWND,
                WM_COPYDATA,
                0,
                &data as *const COPYDATASTRUCT as LPARAM,
                SMTO_NORMAL | SMTO_ABORTIFHUNG,
                2000,
                &mut result,
            )
        };
        if sent == 0 { Err("the game did not answer".to_owned()) } else { Ok(()) }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::path::PathBuf;

    pub(super) fn processes() -> Vec<(u32, PathBuf)> {
        Vec::new()
    }

    pub(super) fn engine_window(_pid: u32) -> Option<isize> {
        None
    }

    pub(super) fn send_copydata(_window: isize, _text: &str) -> Result<(), String> {
        Err("loading a map in a running game is only supported on Windows".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_executables_are_matched_inside_their_install_only() {
        let root = Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod");
        assert!(is_game_exe_of(Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\bin\win64\gmod.exe"), root));
        assert!(is_game_exe_of(Path::new("d:/steamlibrary/steamapps/common/garrysmod/GMOD.EXE"), root));
        assert!(is_game_exe_of(Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\hl2.exe"), root));
        assert!(!is_game_exe_of(Path::new(r"C:\Games\GarrysMod\gmod.exe"), root));
        assert!(!is_game_exe_of(Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod2\gmod.exe"), root));
        assert!(!is_game_exe_of(Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\bin\vbsp.exe"), root));
        assert!(!is_game_exe_of(Path::new("gmod.exe"), Path::new("")));
        assert!(is_game_exe_of(Path::new("C:/g/gmod.exe"), Path::new("C:/g/")));
    }

    #[test]
    fn the_hijack_command_line_quotes_like_the_launcher() {
        let exe = Path::new(r"C:\Program Files (x86)\Steam\steamapps\common\GarrysMod\gmod.exe");
        let args = vec!["-game".to_owned(), "garrysmod".to_owned(), "+map".to_owned(), "gm_test".to_owned()];
        assert_eq!(
            hijack_command_line(exe, &args),
            "\"C:\\Program Files (x86)\\Steam\\steamapps\\common\\GarrysMod\\gmod.exe\" -game garrysmod +map gm_test"
        );
    }

    #[test]
    fn no_game_is_found_in_a_folder_without_one() {
        let dir = std::env::temp_dir().join(format!("hp_compile_no_game_{}", std::process::id()));
        assert!(find_game(&dir).is_none());
        assert!(game_processes(&dir).is_empty());
        let ghost = GameProcess { pid: 0, exe: dir.join("gmod.exe"), window: None };
        assert!(!ghost.is_ready());
        assert!(send_command_line(&ghost, "+map x").is_err(), "no window, nothing sent");
    }
}
