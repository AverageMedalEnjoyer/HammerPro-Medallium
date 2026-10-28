use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use hp_discord::frame::{self, Frame, Opcode};
use hp_discord::{Activity, Options, Session, SystemConnector, Wanted};

fn read_frame(stream: &mut impl Read) -> Frame {
    let mut header = [0u8; frame::HEADER];
    stream.read_exact(&mut header).unwrap();
    let len = u32::from_le_bytes([header[4], header[5], header[6], header[7]]) as usize;
    let mut bytes = header.to_vec();
    bytes.resize(frame::HEADER + len, 0);
    stream.read_exact(&mut bytes[frame::HEADER..]).unwrap();
    frame::decode(&bytes).unwrap().unwrap().0
}

fn serve(mut stream: impl Read + Write) -> Vec<Frame> {
    let hs = read_frame(&mut stream);
    stream.write_all(&frame::encode(Opcode::Frame, "{\"cmd\":\"DISPATCH\",\"evt\":\"READY\"}")).unwrap();
    let mut got = vec![hs];
    loop {
        let f = read_frame(&mut stream);
        let done = f.op == Opcode::Close;
        got.push(f);
        if done {
            return got;
        }
    }
}

#[cfg(windows)]
fn server(path: &str) -> std::thread::JoinHandle<Vec<Frame>> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
    use windows_sys::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
    };
    let name: Vec<u16> = std::ffi::OsStr::new(path).encode_wide().chain(Some(0)).collect();
    let handle = unsafe {
        CreateNamedPipeW(
            name.as_ptr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            0,
            std::ptr::null(),
        )
    };
    assert!(handle != INVALID_HANDLE_VALUE, "{}", std::io::Error::last_os_error());
    let raw = handle as usize;
    std::thread::spawn(move || {
        let handle = raw as windows_sys::Win32::Foundation::HANDLE;
        unsafe {
            ConnectNamedPipe(handle, std::ptr::null_mut());
            serve(std::fs::File::from_raw_handle(handle as _))
        }
    })
}

#[cfg(unix)]
fn server(path: &str) -> std::thread::JoinHandle<Vec<Frame>> {
    let _ = std::fs::remove_file(path);
    let listener = std::os::unix::net::UnixListener::bind(path).unwrap();
    std::thread::spawn(move || serve(listener.accept().unwrap().0))
}

#[cfg(windows)]
fn test_path() -> String {
    format!(r"\\.\pipe\hp-discord-test-{}", std::process::id())
}

#[cfg(unix)]
fn test_path() -> String {
    std::env::temp_dir().join(format!("hp-discord-test-{}", std::process::id())).to_string_lossy().into_owned()
}

#[test]
fn real_transport_round_trip() {
    let path = test_path();
    let srv = server(&path);
    let opts = Options { handshake_timeout: Duration::from_secs(5), ..Options::default() };
    let missing = PathBuf::from(format!("{path}-missing"));
    let mut s = Session::new(Box::new(SystemConnector::with_paths(vec![missing, PathBuf::from(&path)])), opts);
    let a = Activity { details: "Editing gm_city.vmf".into(), state: "Block".into(), ..Activity::default() };
    s.set(Wanted { client_id: "1234567890123456789".into(), activity: Some(a) });
    s.step(Instant::now());
    assert!(s.is_connected());
    s.close();
    let got = srv.join().unwrap();
    #[cfg(unix)]
    let _ = std::fs::remove_file(&path);
    let ops: Vec<Opcode> = got.iter().map(|f| f.op).collect();
    assert_eq!(ops, [Opcode::Handshake, Opcode::Frame, Opcode::Frame, Opcode::Close]);
    assert!(got[0].payload.contains("1234567890123456789"));
    assert!(got[1].payload.contains("Editing gm_city.vmf"));
    assert!(!got[2].payload.contains("activity"), "{}", got[2].payload);
}
