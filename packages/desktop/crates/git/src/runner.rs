use std::io::{self, ErrorKind, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// What a finished command left; `code` is -1 when it was killed or timed out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    pub code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Output {
    pub fn success(&self) -> bool {
        self.code == 0
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stat {
    pub dir: bool,
    pub size: u64,
    pub created: Option<SystemTime>,
}

/// Runs commands and reads files on another machine; every path is that machine's.
pub trait Exec: Send + Sync {
    /// Runs `argv` in `cwd`, or wherever the host starts commands when `cwd` is empty.
    fn exec(&self, cwd: &str, argv: &[&str], stdin: Option<&[u8]>, timeout: Duration) -> io::Result<Output>;
    /// Up to `max` bytes of the file at `path`.
    fn read(&self, path: &str, max: usize) -> io::Result<Vec<u8>>;
    /// `None` when nothing is at `path`.
    fn stat(&self, path: &str) -> io::Result<Option<Stat>>;
}

/// Where a tree's git commands run and its files are read: this Mac, or the host holding it.
#[derive(Clone)]
pub enum Runner {
    Local,
    Remote(Arc<dyn Exec>),
}

impl Runner {
    /// Locally `timeout` is ignored: git here has always run to the end.
    pub fn exec(&self, cwd: &str, argv: &[&str], stdin: Option<&[u8]>, timeout: Duration) -> io::Result<Output> {
        match self {
            Runner::Local => local_exec(cwd, argv, stdin),
            Runner::Remote(host) => host.exec(cwd, argv, stdin, timeout),
        }
    }

    pub fn read(&self, path: &str, max: usize) -> io::Result<Vec<u8>> {
        match self {
            Runner::Local => {
                let mut bytes = std::fs::read(path)?;
                bytes.truncate(max);
                Ok(bytes)
            }
            Runner::Remote(host) => host.read(path, max),
        }
    }

    pub fn stat(&self, path: &str) -> io::Result<Option<Stat>> {
        match self {
            Runner::Local => match std::fs::metadata(path) {
                Ok(m) => Ok(Some(Stat { dir: m.is_dir(), size: m.len(), created: m.created().ok() })),
                Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
                Err(e) => Err(e),
            },
            Runner::Remote(host) => host.stat(path),
        }
    }
}

fn local_exec(cwd: &str, argv: &[&str], stdin: Option<&[u8]>) -> io::Result<Output> {
    let (program, args) = argv.split_first().ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "no command"))?;
    let mut cmd = Command::new(program);
    cmd.args(args);
    if !cwd.is_empty() {
        cmd.current_dir(cwd);
    }
    let out = match stdin {
        None => cmd.output()?,
        Some(input) => {
            let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
            child.stdin.take().expect("piped").write_all(input)?;
            child.wait_with_output()?
        }
    };
    Ok(Output { code: out.status.code().unwrap_or(-1), stdout: out.stdout, stderr: out.stderr })
}

/// A host the desktop isn't linked to: everything fails, so its paths never run here.
pub struct NotConnected;

impl NotConnected {
    fn error() -> io::Error {
        io::Error::new(ErrorKind::NotConnected, "host not connected")
    }
}

impl Exec for NotConnected {
    fn exec(&self, _: &str, _: &[&str], _: Option<&[u8]>, _: Duration) -> io::Result<Output> {
        Err(Self::error())
    }

    fn read(&self, _: &str, _: usize) -> io::Result<Vec<u8>> {
        Err(Self::error())
    }

    fn stat(&self, _: &str) -> io::Result<Option<Stat>> {
        Err(Self::error())
    }
}
