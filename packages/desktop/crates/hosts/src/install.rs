//! Puts pocketd on a host over ssh as a user service, or finds the one already running there.

use crate::Host;
use crate::link::Paths;
use crate::ssh::{self, Failure};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BOOTSTRAP: &str = include_str!("bootstrap.sh");
const RELEASES: &str = "https://github.com/mingo023/anywhere/releases/download";
/// Where an uploaded archive waits, in the app's folder on the host.
const UPLOAD: &str = "upload.tar.gz";

/// A line of the install checklist, in the order bootstrap.sh reports them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Detect,
    Download,
    Verify,
    Unpack,
    Link,
    Service,
    Start,
}

impl Step {
    pub const ALL: [Step; 7] = [Step::Detect, Step::Download, Step::Verify, Step::Unpack, Step::Link, Step::Service, Step::Start];

    pub fn label(self) -> &'static str {
        match self {
            Step::Detect => "Check the host",
            Step::Download => "Download pocketd",
            Step::Verify => "Verify the download",
            Step::Unpack => "Unpack pocketd",
            Step::Link => "Install pocketd",
            Step::Service => "Set up the service",
            Step::Start => "Start pocketd",
        }
    }

    fn named(name: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|s| format!("{s:?}").eq_ignore_ascii_case(name))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Progress {
    Step(Step),
    Warn(String),
    /// A pocketd already runs there; it's reused as it is.
    Running,
    Done,
    Error(String),
}

/// What an `ANYWHERE …` line from bootstrap.sh reports; `None` for its other output.
pub fn parse(line: &str) -> Option<Progress> {
    let rest = line.trim_end().strip_prefix("ANYWHERE ")?;
    let (word, text) = rest.split_once(' ').unwrap_or((rest, ""));
    match word {
        "step" => Step::named(text).map(Progress::Step),
        "warn" => Some(Progress::Warn(text.into())),
        "error" => Some(Progress::Error(text.into())),
        "running" if text.is_empty() => Some(Progress::Running),
        "done" if text.is_empty() => Some(Progress::Done),
        _ => None,
    }
}

/// What a host runs, named as pocketd's archives name it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Platform {
    Unix { os: &'static str, arch: &'static str },
    Windows,
}

/// The platform `uname -s` then `uname -m` printed, past any login noise.
pub fn parse_uname(stdout: &str) -> Result<Platform, String> {
    let mut lines = stdout.lines().map(str::trim).filter(|l| !l.is_empty());
    let (Some(machine), Some(system)) = (lines.next_back(), lines.next_back()) else { return Err("uname printed nothing".into()) };
    if ["MINGW", "MSYS", "CYGWIN"].iter().any(|p| system.starts_with(p)) {
        return Ok(Platform::Windows);
    }
    let os = match system {
        "Darwin" => "darwin",
        "Linux" => "linux",
        _ => return Err(format!("Anywhere doesn't run on {system}")),
    };
    let arch = match machine {
        "arm64" | "aarch64" => "arm64",
        "x86_64" | "amd64" => "amd64",
        _ => return Err(format!("Anywhere doesn't run on {machine}")),
    };
    Ok(Platform::Unix { os, arch })
}

#[derive(Clone, Debug, PartialEq)]
pub enum Error {
    Ssh(Failure),
    Failed(String),
}

impl Error {
    pub fn message(&self, name: &str) -> String {
        match self {
            Error::Ssh(f) => f.message(name),
            Error::Failed(text) => text.clone(),
        }
    }
}

/// What the host runs. Windows' shells have no `uname`, so a failed one asks `ver`.
pub fn probe(host: &Host, paths: &Paths) -> Result<Platform, Error> {
    let out = ssh(host, paths, "uname -s; uname -m", Stdio::null())?;
    if out.status.success() {
        return parse_uname(&String::from_utf8_lossy(&out.stdout)).map_err(Error::Failed);
    }
    let ver = ssh(host, paths, "cmd /c ver", Stdio::null())?;
    if String::from_utf8_lossy(&ver.stdout).contains("Microsoft Windows") {
        return Ok(Platform::Windows);
    }
    Err(Error::Failed(format!("Couldn't tell what {} runs: {}", host.name, last_line(&out.stderr))))
}

/// Where pocketd comes from: the release's download folder, or an archive on this Mac that's uploaded first.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    Release { version: String, url: String },
    Archive(PathBuf),
}

/// The source for `platform`: an archive in `dist` when it has one, else the release `version`.
/// A dev build has no release, so it needs the archive.
pub fn source(platform: Platform, version: &str, dist: Option<&Path>) -> Result<Source, String> {
    let Platform::Unix { os, arch } = platform else { return Err("Windows hosts: coming next".into()) };
    let name = format!("pocketd-{os}-{arch}.tar.gz");
    if let Some(path) = dist.map(|d| d.join(&name)).filter(|p| p.is_file()) {
        return Ok(Source::Archive(path));
    }
    if version == "dev" {
        let dir = dist.map_or("packages/pocketd/dist".into(), |d| d.display().to_string());
        return Err(format!("No {name} in {dir}. Build it with scripts/package-pocketd.sh dev {os}-{arch}"));
    }
    Ok(Source::Release { version: version.into(), url: format!("{RELEASES}/v{version}") })
}

/// The folder of pocketd archives this Mac can upload: `ANYWHERE_HOST_ARCHIVE_DIR`, or in a dev build the one
/// scripts/package-pocketd.sh fills.
pub fn dist_dir() -> Option<PathBuf> {
    std::env::var_os("ANYWHERE_HOST_ARCHIVE_DIR")
        .map(PathBuf::from)
        .or_else(|| (!channel::is_release()).then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../pocketd/dist")))
}

/// The service's name for the app's folder `remote_dir`. One per folder, so a test install never
/// takes over the real one.
pub fn label(remote_dir: &str) -> String {
    let label = format!("{}.host.pocketd", channel::bundle_id());
    if remote_dir == channel::home_dir_name() { label } else { format!("{label}.{}", remote_dir.trim_start_matches('.')) }
}

/// Installs pocketd on `host` and starts it, or finds it running and leaves it be.
pub fn install(host: &Host, paths: &Paths, source: &Source, on: impl FnMut(Progress)) -> Result<(), Error> {
    run(host, paths, source, false, on)
}

/// Installs `source` even over a running pocketd, which hands off to it in place, keeping its terminals.
pub fn update(host: &Host, paths: &Paths, source: &Source, on: impl FnMut(Progress)) -> Result<(), Error> {
    run(host, paths, source, true, on)
}

fn run(host: &Host, paths: &Paths, source: &Source, force: bool, mut on: impl FnMut(Progress)) -> Result<(), Error> {
    let dir = &paths.remote_dir;
    let (version, url, mut env) = match source {
        Source::Release { version, url } => (version.as_str(), url.as_str(), Vec::new()),
        Source::Archive(path) => {
            upload(host, paths, path)?;
            let mut env = vec![format!("ANYWHERE_ARCHIVE=\"$HOME/{dir}/{UPLOAD}\"")];
            env.extend(checksum(path).map(|sum| format!("ANYWHERE_SHA256={sum}")));
            (channel::version(), "", env)
        }
    };
    if force {
        env.push("ANYWHERE_FORCE=1".into());
    }
    let command = bootstrap_command(dir, &env, matches!(source, Source::Archive(_)));
    let mut child = Command::new("ssh")
        .args(args(host, paths, &command))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Failed(format!("Couldn't run ssh: {e}")))?;
    // A host that refused the login has closed the pipe; ssh's exit says why.
    let _ = child.stdin.take().unwrap().write_all(render(version, url, dir, &label(dir)).as_bytes());
    let mut stderr = child.stderr.take().unwrap();
    let errors = std::thread::spawn(move || {
        let mut text = Vec::new();
        let _ = stderr.read_to_end(&mut text);
        text
    });
    let mut end = None;
    for line in BufReader::new(child.stdout.take().unwrap()).lines().map_while(Result::ok) {
        if let Some(progress) = parse(&line) {
            if matches!(progress, Progress::Done | Progress::Running | Progress::Error(_)) {
                end = Some(progress.clone());
            }
            on(progress);
        }
    }
    let status = child.wait().map_err(|e| Error::Failed(e.to_string()))?;
    let stderr = errors.join().unwrap_or_default();
    match end {
        Some(Progress::Done | Progress::Running) => Ok(()),
        Some(Progress::Error(text)) => Err(Error::Failed(text)),
        _ if status.code() == Some(255) => Err(Error::Ssh(ssh::classify(&String::from_utf8_lossy(&stderr)))),
        _ => Err(Error::Failed(format!("The install stopped before it finished: {}", last_line(&stderr)))),
    }
}

/// Copies `archive` into the app's folder on the host.
fn upload(host: &Host, paths: &Paths, archive: &Path) -> Result<(), Error> {
    let file = std::fs::File::open(archive).map_err(|e| Error::Failed(format!("Can't read {}: {e}", archive.display())))?;
    let dir = &paths.remote_dir;
    let out = ssh(host, paths, &format!(r#"sh -c 'umask 077; mkdir -p "$HOME/{dir}" && cat > "$HOME/{dir}/{UPLOAD}"'"#), file.into())?;
    if !out.status.success() {
        return Err(Error::Failed(format!("Couldn't upload pocketd: {}", last_line(&out.stderr))));
    }
    Ok(())
}

/// The sha256 package-pocketd.sh wrote beside `archive`.
fn checksum(archive: &Path) -> Option<String> {
    let mut sums = archive.as_os_str().to_owned();
    sums.push(".sha256");
    let text = std::fs::read_to_string(sums).ok()?;
    let sum = text.split_whitespace().next()?;
    (sum.len() == 64 && sum.bytes().all(|b| b.is_ascii_hexdigit())).then(|| sum.into())
}

/// Runs bootstrap.sh from stdin with `env`, under sh whatever the login shell is.
fn bootstrap_command(dir: &str, env: &[String], uploaded: bool) -> String {
    let mut inner = env.iter().map(|e| format!("{e} ")).collect::<String>() + "sh -s";
    if uploaded {
        inner += &format!(r#"; rc=$?; rm -f "$HOME/{dir}/{UPLOAD}"; exit $rc"#);
    }
    format!("sh -c '{inner}'")
}

fn render(version: &str, url: &str, dir: &str, label: &str) -> String {
    BOOTSTRAP.replace("@@VERSION@@", version).replace("@@URL@@", url).replace("@@HOME_DIR@@", dir).replace("@@LABEL@@", label)
}

/// ssh's arguments for `command`, as the link connects. ssh adds an accepted key to the app's known_hosts,
/// but not the folder holding it.
fn args(host: &Host, paths: &Paths, command: &str) -> Vec<String> {
    let known_hosts = host.known_hosts(&paths.home);
    if let Some(dir) = known_hosts.parent() {
        let _ = std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir);
    }
    ssh::args(host, &known_hosts, &[], command)
}

/// `command`'s output on the host; `Err` only when ssh itself failed, which it says by exiting 255.
fn ssh(host: &Host, paths: &Paths, command: &str, stdin: Stdio) -> Result<Output, Error> {
    let out = Command::new("ssh").args(args(host, paths, command)).stdin(stdin).output().map_err(|e| Error::Failed(format!("Couldn't run ssh: {e}")))?;
    if out.status.code() == Some(255) {
        return Err(Error::Ssh(ssh::classify(&String::from_utf8_lossy(&out.stderr))));
    }
    Ok(out)
}

fn last_line(text: &[u8]) -> String {
    String::from_utf8_lossy(text).lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("no output").into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Target;

    #[test]
    fn bootstraps_lines_become_checklist_progress_and_the_rest_is_log() {
        assert_eq!(parse("ANYWHERE step detect"), Some(Progress::Step(Step::Detect)));
        assert_eq!(parse("ANYWHERE step service\n"), Some(Progress::Step(Step::Service)));
        assert_eq!(parse("ANYWHERE warn linger is off"), Some(Progress::Warn("linger is off".into())));
        assert_eq!(parse("ANYWHERE error checksum mismatch: got a, want b"), Some(Progress::Error("checksum mismatch: got a, want b".into())));
        assert_eq!(parse("ANYWHERE running"), Some(Progress::Running));
        assert_eq!(parse("ANYWHERE done"), Some(Progress::Done));
        assert_eq!(parse(r#"{"label":"x","installed":true}"#), None);
        assert_eq!(parse("ANYWHERE step teleport"), None);
    }

    #[test]
    fn every_step_bootstrap_reports_has_a_checklist_line() {
        for name in ["detect", "download", "verify", "unpack", "link", "service", "start"] {
            assert!(BOOTSTRAP.contains(&format!("say step {name}")), "{name}");
            assert!(Step::named(name).is_some(), "{name}");
        }
    }

    #[test]
    fn uname_names_the_archive_past_any_login_noise() {
        assert_eq!(parse_uname("Welcome!\nLinux\naarch64\n"), Ok(Platform::Unix { os: "linux", arch: "arm64" }));
        assert_eq!(parse_uname("Darwin\narm64\n"), Ok(Platform::Unix { os: "darwin", arch: "arm64" }));
        assert_eq!(parse_uname("Linux\nx86_64\n"), Ok(Platform::Unix { os: "linux", arch: "amd64" }));
        assert_eq!(parse_uname("MINGW64_NT-10.0-22631\nx86_64\n"), Ok(Platform::Windows));
        assert_eq!(parse_uname("FreeBSD\namd64\n"), Err("Anywhere doesn't run on FreeBSD".into()));
        assert_eq!(parse_uname("Linux\nriscv64\n"), Err("Anywhere doesn't run on riscv64".into()));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hosts-install-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn an_archive_on_this_mac_wins_over_the_release() {
        let dist = scratch("dist");
        std::fs::write(dist.join("pocketd-linux-arm64.tar.gz"), b"").unwrap();
        let linux = Platform::Unix { os: "linux", arch: "arm64" };
        assert_eq!(source(linux, "1.2.0", Some(&dist)), Ok(Source::Archive(dist.join("pocketd-linux-arm64.tar.gz"))));
        let mac = Platform::Unix { os: "darwin", arch: "arm64" };
        let release = Source::Release { version: "1.2.0".into(), url: "https://github.com/mingo023/anywhere/releases/download/v1.2.0".into() };
        assert_eq!(source(mac, "1.2.0", Some(&dist)), Ok(release));
        std::fs::remove_dir_all(dist).unwrap();
    }

    #[test]
    fn a_dev_build_without_an_archive_says_how_to_make_one() {
        let err = source(Platform::Unix { os: "linux", arch: "amd64" }, "dev", None).unwrap_err();
        assert!(err.contains("scripts/package-pocketd.sh dev linux-amd64"), "{err}");
        assert_eq!(source(Platform::Windows, "1.2.0", None), Err("Windows hosts: coming next".into()));
    }

    #[test]
    fn the_rendered_script_has_no_placeholder_left() {
        let script = render("1.2.0", "https://x/v1.2.0", ".coding-pocket", "dev.mingo.anywhere.host.pocketd");
        assert!(["@@VERSION@@", "@@URL@@", "@@HOME_DIR@@", "@@LABEL@@"].iter().all(|p| !script.contains(p)));
        assert!(script.contains("VERSION='1.2.0'") && script.contains(r#"DIR="$HOME/.coding-pocket""#));
    }

    #[test]
    fn only_the_apps_own_folder_gets_the_apps_service_name() {
        assert_eq!(label(channel::home_dir_name()), format!("{}.host.pocketd", channel::bundle_id()));
        assert_eq!(label(".coding-pocket-e2e"), format!("{}.host.pocketd.coding-pocket-e2e", channel::bundle_id()));
    }

    #[test]
    fn an_uploaded_archive_is_checked_then_removed_whatever_the_login_shell() {
        let cmd = bootstrap_command(".cp", &["ANYWHERE_ARCHIVE=\"$HOME/.cp/upload.tar.gz\"".into(), "ANYWHERE_FORCE=1".into()], true);
        assert_eq!(cmd, r#"sh -c 'ANYWHERE_ARCHIVE="$HOME/.cp/upload.tar.gz" ANYWHERE_FORCE=1 sh -s; rc=$?; rm -f "$HOME/.cp/upload.tar.gz"; exit $rc'"#);
        assert_eq!(bootstrap_command(".cp", &[], false), "sh -c 'sh -s'");
    }

    #[test]
    fn the_checksum_comes_from_the_file_beside_the_archive() {
        let dir = scratch("sum");
        let archive = dir.join("pocketd-linux-arm64.tar.gz");
        let sum = "ab".repeat(32);
        std::fs::write(dir.join("pocketd-linux-arm64.tar.gz.sha256"), format!("{sum}  pocketd-linux-arm64.tar.gz\n")).unwrap();
        assert_eq!(checksum(&archive), Some(sum));
        std::fs::write(dir.join("pocketd-linux-arm64.tar.gz.sha256"), "nope\n").unwrap();
        assert_eq!(checksum(&archive), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// Installs on the OrbStack VM through the loopback sshd (see scripts/test-bootstrap.sh), from the archive
    /// `scripts/package-pocketd.sh e2e linux-arm64` builds. Run with `cargo test -p hosts -- --ignored`.
    #[test]
    #[ignore]
    fn pocketd_installs_on_a_real_linux_host_then_is_found_running() {
        const DIR: &str = ".coding-pocket-e2e2";
        let host = Host::new(
            Target::Address { addr: "127.0.0.1".into(), port: 2222, user: "mingo".into(), key_file: Some("/tmp/anywhere-e2e/id_ed25519".into()) },
            &[],
        );
        let paths = Paths { home: scratch("e2e"), remote_dir: DIR.into() };
        let sh = |cmd: &str| ssh(&host, &paths, cmd, Stdio::null()).unwrap();
        let pocketd = format!(r#"POCKET_HOME="$HOME/{DIR}" "$HOME/{DIR}/bin/pocketd""#);
        let cleanup = || sh(&format!(r#"{pocketd} service uninstall --label {}; rm -rf "$HOME/{DIR}""#, label(DIR)));
        cleanup();

        let platform = probe(&host, &paths).unwrap();
        assert_eq!(platform, Platform::Unix { os: "linux", arch: "arm64" });
        let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../pocketd/dist");
        let source = source(platform, "dev", Some(&dist)).unwrap();

        let mut seen = Vec::new();
        install(&host, &paths, &source, |p| seen.push(p)).unwrap();
        assert_eq!(seen.last(), Some(&Progress::Done), "{seen:?}");
        assert!(seen.contains(&Progress::Step(Step::Verify)) && !seen.contains(&Progress::Step(Step::Download)), "{seen:?}");
        let status = sh(&format!("{pocketd} status --json"));
        assert!(String::from_utf8_lossy(&status.stdout).contains(r#""os": "linux""#));
        assert!(!sh(&format!(r#"test -e "$HOME/{DIR}/{UPLOAD}""#)).status.success(), "the upload is removed");

        let mut again = Vec::new();
        install(&host, &paths, &source, |p| again.push(p)).unwrap();
        assert_eq!(again, [Progress::Step(Step::Detect), Progress::Running]);

        let mut updated = Vec::new();
        update(&host, &paths, &source, |p| updated.push(p)).unwrap();
        assert_eq!(updated.last(), Some(&Progress::Done), "{updated:?}");
        assert!(sh(&format!("{pocketd} status")).status.success(), "pocketd still answers after the update");

        cleanup();
        assert!(!sh(&format!("systemctl --user is-active {}", label(DIR))).status.success());
        std::fs::remove_dir_all(&paths.home).unwrap();
    }
}
