//! The `anywhere` command on the user's PATH: a link to the pocketd in the app, made once behind macOS's admin prompt.

use std::path::{Path, PathBuf};
use std::process::Command;

const LINK: &str = "/usr/local/bin/anywhere";

/// The pocketd in the bundle whose binary is `exe`, `Anywhere.app/Contents/MacOS/pocket-desktop`.
fn bundled_pocketd(exe: &Path) -> Option<PathBuf> {
    Some(exe.parent()?.parent()?.join("Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd"))
}

fn pocketd() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    bundled_pocketd(&exe).filter(|p| p.is_file()).ok_or_else(|| "This build has no pocketd in its bundle".to_string())
}

/// Whether the link points at this app's pocketd. Reads the disk; keep it off the UI thread.
pub fn installed() -> bool {
    pocketd().is_ok_and(|p| std::fs::read_link(LINK).is_ok_and(|target| target == p))
}

/// AppleScript that makes the link as root.
fn link_script(pocketd: &Path) -> String {
    let path = pocketd.display().to_string().replace('\\', "\\\\").replace('"', "\\\"");
    format!("do shell script \"mkdir -p /usr/local/bin && ln -sfn \" & quoted form of \"{path}\" & \" {LINK}\" with administrator privileges")
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// Makes the link and installs the skill. Blocks on the password prompt; keep it off the UI thread.
pub fn install() -> Result<(), String> {
    let pocketd = pocketd()?;
    run(crate::command("osascript").args(["-e", &link_script(&pocketd)]))?;
    run(crate::command(&pocketd).args(["skill", "install"]))
}

#[cfg(test)]
mod tests {
    use super::{bundled_pocketd, link_script};
    use std::path::{Path, PathBuf};

    #[test]
    fn the_bundled_pocketd_sits_in_the_helper_app() {
        let got = bundled_pocketd(Path::new("/Applications/Anywhere.app/Contents/MacOS/pocket-desktop"));
        assert_eq!(got, Some(PathBuf::from("/Applications/Anywhere.app/Contents/Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd")));
    }

    #[test]
    fn the_link_script_keeps_a_quote_in_the_path_inside_its_string() {
        assert!(link_script(Path::new("/Apps/A\"B.app/pocketd")).contains("quoted form of \"/Apps/A\\\"B.app/pocketd\""));
    }
}
