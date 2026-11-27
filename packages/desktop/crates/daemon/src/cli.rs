//! The `zoes` command, and `anywhere` from before the rename, on the user's PATH: links to the pocketd in the app,
//! made once behind macOS's admin prompt.

use std::path::{Path, PathBuf};
use std::process::Command;

const LINKS: [&str; 2] = ["/usr/local/bin/zoes", "/usr/local/bin/anywhere"];

/// The pocketd in the bundle whose binary is `exe`, `Zoes.app/Contents/MacOS/pocket-desktop`.
fn bundled_pocketd(exe: &Path) -> Option<PathBuf> {
    Some(exe.parent()?.parent()?.join("Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd"))
}

fn pocketd() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    bundled_pocketd(&exe).filter(|p| p.is_file()).ok_or_else(|| "This build has no pocketd in its bundle".to_string())
}

/// Whether both links point at this app's pocketd. Reads the disk; keep it off the UI thread.
pub fn installed() -> bool {
    pocketd().is_ok_and(|p| links_to(&LINKS, &p))
}

fn links_to(links: &[&str], pocketd: &Path) -> bool {
    links.iter().all(|link| std::fs::read_link(link).is_ok_and(|target| target == pocketd))
}

/// AppleScript that makes the links as root, behind one password prompt.
fn link_script(pocketd: &Path) -> String {
    let path = pocketd.display().to_string().replace('\\', "\\\\").replace('"', "\\\"");
    let links: String = LINKS.iter().map(|link| format!(" && ln -sfn \" & quoted form of \"{path}\" & \" {link}")).collect();
    format!("do shell script \"mkdir -p /usr/local/bin{links}\" with administrator privileges")
}

fn run(cmd: &mut Command) -> Result<(), String> {
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// Makes the links and installs the skill. Blocks on the password prompt; keep it off the UI thread.
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
    fn the_bundled_pocketd_sits_in_the_helper_app_whatever_the_app_folder_is_called() {
        for app in ["/Applications/Anywhere.app", "/Applications/Zoes.app"] {
            let got = bundled_pocketd(&Path::new(app).join("Contents/MacOS/pocket-desktop"));
            assert_eq!(got, Some(Path::new(app).join("Contents/Library/Helpers/AnywhereDaemon.app/Contents/MacOS/pocketd")));
        }
    }

    #[test]
    fn the_link_script_links_zoes_and_anywhere_behind_one_prompt() {
        assert_eq!(
            link_script(Path::new("/A.app/pocketd")),
            "do shell script \"mkdir -p /usr/local/bin && ln -sfn \" & quoted form of \"/A.app/pocketd\" & \" /usr/local/bin/zoes && ln -sfn \" & quoted form of \"/A.app/pocketd\" & \" /usr/local/bin/anywhere\" with administrator privileges"
        );
    }

    #[test]
    fn the_link_script_keeps_a_quote_in_the_path_inside_its_string() {
        assert!(link_script(Path::new("/Apps/A\"B.app/pocketd")).contains("quoted form of \"/Apps/A\\\"B.app/pocketd\""));
    }

    #[cfg(unix)]
    #[test]
    fn the_command_counts_as_installed_only_once_both_names_link_to_this_pocketd() {
        let dir = std::env::temp_dir().join(format!("cli-links-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (zoes, anywhere, pocketd) = (dir.join("zoes"), dir.join("anywhere"), dir.join("pocketd"));
        let links = [zoes.to_str().unwrap(), anywhere.to_str().unwrap()];
        std::os::unix::fs::symlink(&pocketd, &anywhere).unwrap();
        let only_anywhere = super::links_to(&links, &pocketd);
        std::os::unix::fs::symlink(PathBuf::from("/elsewhere/pocketd"), &zoes).unwrap();
        let zoes_elsewhere = super::links_to(&links, &pocketd);
        std::fs::remove_file(&zoes).unwrap();
        std::os::unix::fs::symlink(&pocketd, &zoes).unwrap();
        let both = super::links_to(&links, &pocketd);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!((only_anywhere, zoes_elsewhere, both), (false, false, true));
    }
}
