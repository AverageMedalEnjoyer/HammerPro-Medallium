use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MAX_OUTPUT: u64 = 1024 * 1024;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CliOutput {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl CliOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0)
    }

    pub fn text(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

pub fn run(program: &Path, args: &[&str], timeout: Duration) -> io::Result<CliOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = cmd.spawn()?;
    let out = child.stdout.take().map(read_bounded);
    let err = child.stderr.take().map(read_bounded);
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("{} did not finish within {} s", program.display(), timeout.as_secs()),
            ));
        }
        std::thread::sleep(Duration::from_millis(30));
    };
    let join = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
        h.and_then(|h| h.join().ok()).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    };
    Ok(CliOutput { code: status.code(), stdout: join(out), stderr: join(err) })
}

fn read_bounded<R: Read + Send + 'static>(stream: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut limited = stream.take(MAX_OUTPUT);
        let _ = limited.read_to_end(&mut buf);
        let mut rest = limited.into_inner();
        let _ = io::copy(&mut rest, &mut io::sink());
        buf
    })
}
