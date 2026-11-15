use agents::factory::{Profile, ProfileFile};
use base64::Engine as _;
use std::collections::BTreeSet;
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

/// The soul and the instructions together, in bytes. pocketd refuses more.
pub const MAX_TEXT: usize = 64 * 1024;
/// A whole profile, in bytes. pocketd refuses more.
pub const MAX_PROFILE: usize = 2 * 1024 * 1024;

const SOUL: &str = "SOUL.md";
const INSTRUCTIONS: &str = "INSTRUCTIONS.md";
const AGENTS: &str = "AGENTS.md";
const SKILLS: &str = "skills";

/// One file the import takes.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: String,
    pub size: usize,
    /// Not markdown, so the agent can run it.
    pub runs: bool,
}

/// What an import would put into a profile. Nothing is saved until the user takes it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preview {
    /// The folder's or the archive's name.
    pub from: String,
    /// `soul`, `instructions`, `files` and `skills` are filled. The rest is empty.
    pub profile: Profile,
    pub entries: Vec<Entry>,
    /// How many files sit outside the layout. The import leaves them out.
    pub left_out: usize,
}

impl Preview {
    pub fn size(&self) -> usize {
        self.entries.iter().map(|e| e.size).sum()
    }
}

/// A file's path in the layout, and its bytes.
type Loaded = (String, Vec<u8>);

/// Reads a folder laid out as `SOUL.md`, `INSTRUCTIONS.md` or `AGENTS.md`, and `skills/<name>/…`.
/// It reads the disk, so call it off the UI thread.
pub fn read(path: &Path) -> Result<Preview, String> {
    let from = name_of(path);
    if path.is_dir() {
        let (files, left_out) = folder(path)?;
        return build(from, files, left_out);
    }
    if path.is_file() && path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let (files, left_out) = archive(path)?;
        return build(from, files, left_out);
    }
    Err(format!("{from} is not a folder or a .zip"))
}

fn name_of(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn too_big() -> String {
    "These files are more than 2 MB. A profile holds 2 MB at most".into()
}

/// Files macOS and git add, which are no part of a profile.
fn ignored(name: &str) -> bool {
    matches!(name, ".DS_Store" | "__MACOSX" | ".git")
}

/// A folder's entries by name.
fn children(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let entries = fs::read_dir(dir).map_err(|e| format!("Can't read {}: {e}", name_of(dir)))?;
    let mut out: Vec<(String, PathBuf)> = entries.filter_map(Result::ok).map(|e| (e.file_name().to_string_lossy().into_owned(), e.path())).filter(|(name, _)| !ignored(name)).collect();
    out.sort();
    Ok(out)
}

/// Whether `path` is a folder, and its size. A link is refused, because it can point outside the folder.
fn real(path: &Path) -> Result<(bool, usize), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("Can't read {}: {e}", name_of(path)))?;
    if meta.file_type().is_symlink() {
        return Err(format!("{} is a link. Import needs the file itself", name_of(path)));
    }
    Ok((meta.is_dir(), meta.len() as usize))
}

/// Reads one file unless it takes the import over the limit. The size on disk is checked first, so a huge file is never read.
fn load(path: &Path, size: usize, total: &mut usize) -> Result<Vec<u8>, String> {
    *total += size;
    if *total > MAX_PROFILE {
        return Err(too_big());
    }
    fs::read(path).map_err(|e| format!("Can't read {}: {e}", name_of(path)))
}

/// The layout's files in `root`, and how many of its other entries are left out. Only `skills` is walked.
fn folder(root: &Path) -> Result<(Vec<Loaded>, usize), String> {
    let (mut files, mut left_out, mut total) = (Vec::new(), 0, 0);
    for (name, path) in children(root)? {
        let (dir, size) = real(&path)?;
        match name.as_str() {
            SKILLS if dir => walk(&path, SKILLS, &mut files, &mut total)?,
            SOUL | INSTRUCTIONS | AGENTS if !dir => files.push((name, load(&path, size, &mut total)?)),
            _ => left_out += 1,
        }
    }
    Ok((files, left_out))
}

fn walk(dir: &Path, at: &str, files: &mut Vec<Loaded>, total: &mut usize) -> Result<(), String> {
    for (name, path) in children(dir)? {
        let at = format!("{at}/{name}");
        let (dir, size) = real(&path)?;
        if dir {
            walk(&path, &at, files, total)?;
        } else {
            files.push((at, load(&path, size, total)?));
        }
    }
    Ok(())
}

/// An archive entry's name as its parts. None when the name could land outside the folder the archive stands for.
fn split(name: &str) -> Option<Vec<&str>> {
    if name.starts_with('/') || name.contains(['\\', '\0', ':']) {
        return None;
    }
    let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    (!parts.contains(&"..")).then_some(parts)
}

/// The layout's files in a `.zip`. Every name is checked before any file is read. One folder that wraps everything is dropped.
fn archive(path: &Path) -> Result<(Vec<Loaded>, usize), String> {
    let unread = |e: &dyn std::fmt::Display| format!("Can't read {}: {e}", name_of(path));
    let mut zip = zip::ZipArchive::new(fs::File::open(path).map_err(|e| unread(&e))?).map_err(|e| unread(&e))?;
    let mut kept: Vec<(usize, Vec<String>, usize)> = Vec::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| unread(&e))?;
        let name = entry.name().to_string();
        let Some(parts) = split(&name) else { return Err(format!("{name} points outside the archive")) };
        if entry.is_symlink() {
            return Err(format!("{name} is a link. Import needs the file itself"));
        }
        if entry.is_dir() || parts.is_empty() || parts.iter().any(|p| ignored(p)) {
            continue;
        }
        kept.push((i, parts.into_iter().map(String::from).collect(), entry.size() as usize));
    }
    let wrapper = kept.first().map(|(_, parts, _)| parts[0].clone()).filter(|top| top != SKILLS && kept.iter().all(|(_, parts, _)| parts.len() > 1 && parts[0] == *top));
    let skip = usize::from(wrapper.is_some());
    let (mut files, mut left_out, mut total) = (Vec::new(), 0, 0);
    for (i, parts, size) in kept {
        let at = parts[skip..].join("/");
        if !matches!(at.as_str(), SOUL | INSTRUCTIONS | AGENTS) && skill_of(&at).is_none() {
            left_out += 1;
            continue;
        }
        total += size;
        if total > MAX_PROFILE {
            return Err(too_big());
        }
        // An archive can say a file is smaller than it is, so the read stops at the limit too.
        let mut bytes = Vec::new();
        zip.by_index(i).map_err(|e| unread(&e))?.take(MAX_PROFILE as u64 + 1).read_to_end(&mut bytes).map_err(|e| unread(&e))?;
        if bytes.len() > MAX_PROFILE {
            return Err(too_big());
        }
        files.push((at, bytes));
    }
    Ok((files, left_out))
}

/// A skill on this Mac, as an import of `skills/<name>/…`. `dir` may be a link, as the entries of `~/.claude/skills` often are.
pub fn read_skill(dir: &Path) -> Result<Preview, String> {
    let name = name_of(dir);
    let real = fs::canonicalize(dir).map_err(|e| format!("Can't read {name}: {e}"))?;
    let (mut files, mut total) = (Vec::new(), 0);
    walk(&real, &format!("{SKILLS}/{name}"), &mut files, &mut total)?;
    build(name, files, 0)
}

/// The skills on this Mac by name: each folder with a `SKILL.md` under `~/.claude/skills` and `~/.agents/skills`.
pub fn installed(home: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for root in [".claude/skills", ".agents/skills"] {
        for (name, path) in children(&home.join(root)).unwrap_or_default() {
            if path.join("SKILL.md").is_file() && !out.iter().any(|(known, _)| *known == name) {
                out.push((name, path));
            }
        }
    }
    out.sort();
    out
}

/// The skill a path under `skills/<name>/` belongs to. A file right under `skills/` belongs to none.
pub(crate) fn skill_of(path: &str) -> Option<&str> {
    let (name, rest) = path.strip_prefix("skills/")?.split_once('/')?;
    (!name.is_empty() && !rest.is_empty()).then_some(name)
}

fn entry(path: &str, size: usize) -> Entry {
    Entry { path: path.into(), size, runs: !path.to_lowercase().ends_with(".md") }
}

fn text(name: &str, bytes: Vec<u8>) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| format!("{name} is not text"))
}

fn file(path: String, bytes: Vec<u8>) -> ProfileFile {
    match String::from_utf8(bytes) {
        Ok(text) => ProfileFile { path, text, binary: false },
        Err(e) => ProfileFile { path, text: base64::engine::general_purpose::STANDARD.encode(e.into_bytes()), binary: true },
    }
}

/// Sorts loaded files into a profile, and refuses what pocketd would refuse.
fn build(from: String, files: Vec<Loaded>, mut left_out: usize) -> Result<Preview, String> {
    let (mut soul, mut instructions, mut agents, mut skills) = (None, None, None, Vec::new());
    for (path, bytes) in files {
        match path.as_str() {
            SOUL => soul = Some(bytes),
            INSTRUCTIONS => instructions = Some(bytes),
            AGENTS => agents = Some(bytes),
            _ if skill_of(&path).is_some() => skills.push((path, bytes)),
            _ => left_out += 1,
        }
    }
    if instructions.is_some() && agents.is_some() {
        left_out += 1;
    }
    let instructions = instructions.map(|b| (INSTRUCTIONS, b)).or(agents.map(|b| (AGENTS, b)));
    let (mut profile, mut entries) = (Profile::default(), Vec::new());
    if let Some(bytes) = soul {
        entries.push(entry(SOUL, bytes.len()));
        profile.soul = text(SOUL, bytes)?;
    }
    if let Some((name, bytes)) = instructions {
        entries.push(entry(name, bytes.len()));
        profile.instructions = text(name, bytes)?;
    }
    if profile.soul.len() + profile.instructions.len() > MAX_TEXT {
        return Err("The soul and the instructions are more than 64 KB together".into());
    }
    skills.sort();
    let names: BTreeSet<&str> = skills.iter().filter_map(|(path, _)| skill_of(path)).collect();
    for name in &names {
        if !skills.iter().any(|(path, _)| *path == format!("{SKILLS}/{name}/SKILL.md")) {
            return Err(format!("skills/{name} has no SKILL.md"));
        }
    }
    profile.info.skills = names.len() as u32;
    for (path, bytes) in skills {
        entries.push(entry(&path, bytes.len()));
        profile.files.push(file(path, bytes));
    }
    if entries.is_empty() {
        return Err(format!("{from} has no SOUL.md, INSTRUCTIONS.md, AGENTS.md or skills folder"));
    }
    if entries.iter().map(|e| e.size).sum::<usize>() > MAX_PROFILE {
        return Err(too_big());
    }
    Ok(Preview { from, profile, entries, left_out })
}

#[cfg(test)]
mod tests {
    use super::{MAX_PROFILE, MAX_TEXT, installed, read, read_skill};
    use std::fs;
    use std::path::PathBuf;

    /// A fresh folder under the temp dir that holds `files`.
    fn folder(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let root = std::env::temp_dir().join(format!("pocket-import-{}-{name}", std::process::id()));
        _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        for (path, text) in files {
            let path = root.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        root
    }

    #[test]
    fn a_folder_gives_the_soul_the_instructions_and_each_skills_files() {
        let root = folder("whole", &[("SOUL.md", "You are careful."), ("INSTRUCTIONS.md", "Test first."), ("skills/review/SKILL.md", "# Review"), ("skills/review/notes/style.md", "Short.")]);
        let p = read(&root).unwrap();
        assert_eq!((p.profile.soul.as_str(), p.profile.instructions.as_str(), p.profile.info.skills, p.left_out), ("You are careful.", "Test first.", 1, 0));
        let files: Vec<_> = p.profile.files.iter().map(|f| (f.path.as_str(), f.text.as_str(), f.binary)).collect();
        assert_eq!(files, [("skills/review/SKILL.md", "# Review", false), ("skills/review/notes/style.md", "Short.", false)]);
        let entries: Vec<_> = p.entries.iter().map(|e| (e.path.as_str(), e.size)).collect();
        assert_eq!(entries, [("SOUL.md", 16), ("INSTRUCTIONS.md", 11), ("skills/review/SKILL.md", 8), ("skills/review/notes/style.md", 6)]);
        assert_eq!(p.size(), 41);
        assert!(p.from.ends_with("-whole"));
    }

    #[test]
    fn agents_md_is_read_as_the_instructions_unless_instructions_md_is_there() {
        let p = read(&folder("agents", &[("AGENTS.md", "From agents.")])).unwrap();
        assert_eq!((p.profile.instructions.as_str(), p.entries[0].path.as_str(), p.left_out), ("From agents.", "AGENTS.md", 0));
        let p = read(&folder("both", &[("AGENTS.md", "From agents."), ("INSTRUCTIONS.md", "Mine.")])).unwrap();
        assert_eq!((p.profile.instructions.as_str(), p.entries.len(), p.left_out), ("Mine.", 1, 1));
    }

    #[test]
    fn files_that_are_not_markdown_are_the_ones_the_agent_can_run() {
        let root = folder("runs", &[("skills/deploy/SKILL.md", "# Deploy"), ("skills/deploy/run.sh", "#!/bin/sh\n"), ("skills/deploy/README.MD", "Read me")]);
        let p = read(&root).unwrap();
        assert_eq!(p.entries.iter().filter(|e| e.runs).map(|e| e.path.as_str()).collect::<Vec<_>>(), ["skills/deploy/run.sh"]);
    }

    #[test]
    fn a_file_that_is_not_text_is_carried_as_base64() {
        let root = folder("binary", &[("skills/logo/SKILL.md", "# Logo")]);
        fs::write(root.join("skills/logo/logo.png"), [0xff, 0xd8, 0x00]).unwrap();
        let p = read(&root).unwrap();
        let png = p.profile.files.iter().find(|f| f.path.ends_with("logo.png")).unwrap();
        assert_eq!((png.text.as_str(), png.binary), ("/9gA", true));
    }

    #[test]
    fn files_outside_the_layout_are_left_out_and_counted() {
        let root = folder("extra", &[("SOUL.md", "Me."), ("README.md", "Hello"), ("src/main.rs", "fn main() {}"), ("skills/loose.md", "No skill"), (".DS_Store", "x")]);
        let p = read(&root).unwrap();
        assert_eq!((p.entries.len(), p.left_out), (1, 3));
    }

    #[test]
    fn a_soul_and_instructions_over_64_kb_together_are_refused() {
        let half = "x".repeat(MAX_TEXT / 2 + 1);
        let err = read(&folder("text", &[("SOUL.md", half.as_str()), ("INSTRUCTIONS.md", half.as_str())])).unwrap_err();
        assert_eq!(err, "The soul and the instructions are more than 64 KB together");
    }

    #[test]
    fn a_folder_over_2_mb_is_refused() {
        let big = "x".repeat(MAX_PROFILE / 2 + 1);
        let err = read(&folder("big", &[("skills/a/SKILL.md", big.as_str()), ("skills/a/data.txt", big.as_str())])).unwrap_err();
        assert_eq!(err, "These files are more than 2 MB. A profile holds 2 MB at most");
    }

    #[test]
    fn a_skill_without_skill_md_is_refused() {
        let err = read(&folder("noskill", &[("skills/review/notes.md", "x")])).unwrap_err();
        assert_eq!(err, "skills/review has no SKILL.md");
    }

    #[test]
    fn a_folder_with_nothing_to_import_is_refused() {
        let root = folder("empty", &[("README.md", "Hello")]);
        assert!(read(&root).unwrap_err().ends_with("has no SOUL.md, INSTRUCTIONS.md, AGENTS.md or skills folder"));
        assert!(read(&root.join("README.md")).unwrap_err().ends_with("is not a folder or a .zip"));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_inside_the_folder_is_refused() {
        let root = folder("link", &[("skills/a/SKILL.md", "# A")]);
        std::os::unix::fs::symlink("/etc/hosts", root.join("skills/a/hosts")).unwrap();
        assert_eq!(read(&root).unwrap_err(), "hosts is a link. Import needs the file itself");
    }

    /// A zip under the temp dir that holds `files`.
    fn zipped(name: &str, files: &[(&str, &str)]) -> PathBuf {
        use std::io::Write as _;
        let path = std::env::temp_dir().join(format!("pocket-import-{}-{name}.zip", std::process::id()));
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (name, text) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    #[test]
    fn a_zip_reads_as_the_folder_it_holds() {
        let files = [("SOUL.md", "You are careful."), ("AGENTS.md", "Test first."), ("skills/review/SKILL.md", "# Review"), ("skills/review/run.sh", "#!/bin/sh\n")];
        let from_folder = read(&folder("same", &files)).unwrap();
        let flat = read(&zipped("flat", &files)).unwrap();
        assert_eq!((&flat.profile, &flat.entries, flat.left_out), (&from_folder.profile, &from_folder.entries, 0));
        let wrapped: Vec<(String, &str)> = files.iter().map(|(path, text)| (format!("go-backend/{path}"), *text)).collect();
        let mut entries: Vec<(&str, &str)> = wrapped.iter().map(|(path, text)| (path.as_str(), *text)).collect();
        entries.push(("__MACOSX/go-backend/._SOUL.md", "x"));
        let from_zip = read(&zipped("same", &entries)).unwrap();
        assert_eq!((&from_zip.profile, &from_zip.entries, from_zip.left_out), (&from_folder.profile, &from_folder.entries, 0));
        assert!(from_zip.from.ends_with("-same.zip"));
    }

    #[test]
    fn an_entry_that_points_outside_the_archive_is_refused() {
        for name in ["../evil.md", "skills/a/../../evil.md", "/etc/evil.md"] {
            let err = read(&zipped("outside", &[("SOUL.md", "Me."), (name, "x")])).unwrap_err();
            assert_eq!(err, format!("{name} points outside the archive"));
        }
    }

    #[test]
    fn a_link_in_an_archive_is_refused() {
        let path = std::env::temp_dir().join(format!("pocket-import-{}-link.zip", std::process::id()));
        let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("skills/a/SKILL.md", options).unwrap();
        zip.add_symlink("skills/a/hosts", "/etc/hosts", options).unwrap();
        zip.finish().unwrap();
        assert_eq!(read(&path).unwrap_err(), "skills/a/hosts is a link. Import needs the file itself");
    }

    #[test]
    fn an_archive_whose_files_are_over_2_mb_is_refused() {
        let big = "x".repeat(MAX_PROFILE / 2 + 1);
        let err = read(&zipped("big", &[("skills/a/SKILL.md", big.as_str()), ("skills/a/data.txt", big.as_str())])).unwrap_err();
        assert_eq!(err, "These files are more than 2 MB. A profile holds 2 MB at most");
    }

    #[cfg(unix)]
    #[test]
    fn a_skill_on_this_mac_is_listed_once_and_read_through_its_link() {
        let home = folder("home", &[(".agents/skills/review/SKILL.md", "# Review"), (".agents/skills/review/run.sh", "x"), (".claude/skills/mine/SKILL.md", "# Mine"), (".claude/skills/notes.md", "x")]);
        std::os::unix::fs::symlink(home.join(".agents/skills/review"), home.join(".claude/skills/review")).unwrap();
        let found = installed(&home);
        assert_eq!(found.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(), ["mine", "review"]);
        let p = read_skill(&found[1].1).unwrap();
        assert_eq!(p.entries.iter().map(|e| (e.path.as_str(), e.runs)).collect::<Vec<_>>(), [("skills/review/SKILL.md", false), ("skills/review/run.sh", true)]);
        assert_eq!((p.from.as_str(), p.profile.info.skills), ("review", 1));
    }
}

