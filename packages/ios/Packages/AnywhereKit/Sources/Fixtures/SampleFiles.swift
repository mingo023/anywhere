import Foundation
import Wire

extension Sample {
    /// A tree's folders as fs.list sends them, folders first, with ignored folders, dotfiles, a link and git letters.
    public static let listings: [String: [FsEntry]] = {
        let at = now - 3_600_000
        func dir(_ name: String, git: String? = nil, ignored: Bool? = nil) -> FsEntry {
            FsEntry(name: name, dir: true, size: 192, mtime: at, ignored: ignored, git: git)
        }
        func file(_ name: String, _ size: Int, git: String? = nil, ignored: Bool? = nil, link: Bool? = nil) -> FsEntry {
            FsEntry(name: name, dir: false, link: link, size: size, mtime: at, ignored: ignored, git: git)
        }
        return [
            "": [
                dir(".github"), dir("docs", git: "M"), dir("node_modules", ignored: true), dir("packages", git: "M"), dir("scripts"),
                dir("target", ignored: true), file(".env", 120, ignored: true), file(".gitignore", 310), file("CHANGELOG.md", 18_204, git: "M"),
                file("CONTEXT.md", 6_402), file("LICENSE", 1_071), file("README.md", 4_310, git: "M"), file("package.json", 812),
                file("readme", 9, link: true),
            ],
            "docs": [dir("adr"), dir("mobile", git: "A"), file("logo.png", 853), file("icon.svg", 402)],
            "docs/mobile": [file("plan.md", 9_811, git: "A"), file("protocol.md", 14_022, git: "A")],
            "packages": [dir("desktop", git: "M"), dir("ios", git: "?"), dir("pocketd", git: "M")],
            "packages/pocketd": [dir("cmd"), dir("internal", git: "M"), file("go.mod", 740), file("go.sum", 9_120)],
            "packages/pocketd/internal": [
                dir("fsview", git: "?"), dir("hostop"), dir("proto", git: "M"), dir("wsserver", git: "M"),
            ],
            "packages/pocketd/internal/fsview": [
                file("letters.go", 2_410, git: "?"), file("read.go", 3_120, git: "?"), file("search.go", 3_904, git: "?"),
                file("service.go", 5_221, git: "?"), file("tree.go", 2_270, git: "?"),
            ],
        ]
    }()

    public static let fileTree = "/Users/mingo/dev/anywhere"

    public static let rustSource = """
        use std::path::Path;

        /// The letter a folder at `dir` takes from the changed `files` inside it: theirs when they agree, else M.
        pub fn folder_status(files: &[FileStat], root: &str, dir: &str) -> Option<char> {
            let rel = dir.strip_prefix(root)?.trim_start_matches('/');
            let mut inside = files
                .iter()
                .filter(|f| f.path.strip_prefix(rel).is_some_and(|r| r.starts_with('/')))
                .map(|f| f.status);
            let first = inside.next()?;
            Some(if inside.all(|s| s == first) { first } else { 'M' })
        }

        #[cfg(test)]
        mod tests {
            use super::*;

            #[test]
            fn a_folder_takes_the_letter_its_changes_share_or_else_m() {
                let files = [stat("new/a.rs", 'A'), stat("src/a.rs", 'A'), stat("src/b.rs", 'D')];
                assert_eq!(folder_status(&files, "/r", "/r/new"), Some('A'));
                assert_eq!(folder_status(&files, "/r", "/r/src"), Some('M'));
            }
        }

        """

    public static let markdownSource = """
        # iOS app build plan

        Read with `docs/adr/0007-native-ios-app.md` (decisions) and **protocol.md** (the wire contract).

        ## Toolchain

        - No Xcode. Swift 6.2 from swiftly.
        - Build one module with your own scratch dir.
        - [x] Files slice: tree, preview, search, attachments

        > [!NOTE]
        > Judge speed in release: our own code is ~10× slower in debug.

        | Measurement | Before | Now |
        |---|---|---|
        | Changes list, 1137 files | 67ms | 3ms |

        ```swift
        let found = FuzzyIndex(paths).search("expl", limit: 50)
        ```

        """

    public static let svgSource = """
        <svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">
          <rect width="64" height="64" rx="14" fill="#0A84FF"/>
          <path d="M18 34l10 10 18-22" stroke="#fff" stroke-width="6" fill="none" stroke-linecap="round"/>
        </svg>

        """

    /// A 160×100 PNG: a sky, a sun, a hill.
    public static let png = Data(base64Encoded: """
        iVBORw0KGgoAAAANSUhEUgAAAKAAAABkCAIAAACO1KzYAAADHElEQVR42u3cR09UURTA8Td+ENdjiS22wAwCGiyxxbKxIwQBAY0tWGKLLbbYYosttqjAMBRBEIHYgiW22L6NJs7G67wHb5g598aT/z8smLP9Ld85NxJdMRDxfv358/7+Z4T5k/l/Oo9EVwKsG3gVwLqBVwOsG3gNwLqB1wGsG7gUYN3A6wHWDVwGsG7gcoB1A1cArBt4A8C6gSsB1g1cBbBu4GqAdQPXAKwbuBZg3cB1AOsG3gSwbuDNAOsG3gKwbuCtAA9nvnb5Yy9ELQ3FroG3AZzZvHRZu5dhbY1F7oC3Axx2Xra01cuijqZCF8A7AA41L1+SlW6qzsQM68D1AA8xr1ic9HJaV7LAIvBOgAebVy7KsW6q7pa4LeBdAAfOqxY2e2I9bY1bAd4NcDDwAkngNjvAewD2n1fPF9RN1dMekwfeC7DPfOO8hGel3o6YMPA+gP2A59oC7pQG3g/wv/OaOU2exfqe5EsCHwA4DXi2XeAuUeCDABvz2hKruqn6u/PEgA8BbALPcgHcIwd8GGATeKYL4GdywEcANoGLXQD3ygEfBdgELnIB3CcHfAxgE7jQBXC/HPBxgE3gAhfAz+WATwBsAsddAL+QAz4JsAkccwH8Ug74FMAmcL4L4FdywKcBNoHzXAC/lgM+A7AJPN0F8IAc8FmATeBpLoDfyAGfAzjta9JUu1+T3op+TToPcBrwFLvA70SBLwDst9Ex2dZGx3vpjY6LAPsBT7IF/EEa+BLAAVuVE+W3Kj9a2Kq8DHDwXvQEyb3oT3b2oq8AHAw8XhL4sx3gqwAPeps0TuY26Yu126RrAA91XTg219eFX21eF14HONx98Jhc3Ad/s38ffAPg0Bf+o7O78P/u5ML/JsAZvtExKvM3On44fKPjFsDDemUnGu6VnZ/OX9m5DbDud7LuAKwb+C7AuoHvAawb+D7AuoEfAKwb+CHAuoEfAawbuAFg3cCNAOsGTgCsG7gZYN3ASYBVA4+sL/FIbwADTAATwAQwAUwAE8AAE8AEMAFMABPABDDABDABTAATwAQwAUwAA0wAE8Dkrt8f6W+sbnvPswAAAABJRU5ErkJggg==
        """, options: .ignoreUnknownCharacters)!

    public static func fileRead(_ path: String, tree: String = fileTree) -> FsRead {
        let at = now - 3_600_000
        switch path {
        case let p where p.hasSuffix(".rs"):
            return FsRead(tree: tree, path: p, kind: .text, size: rustSource.utf8.count, mtime: at, text: rustSource, language: "rust")
        case let p where p.hasSuffix(".md"):
            return FsRead(tree: tree, path: p, kind: .text, size: markdownSource.utf8.count, mtime: at, text: markdownSource, language: "markdown")
        case let p where p.hasSuffix(".svg"):
            return FsRead(tree: tree, path: p, kind: .text, size: svgSource.utf8.count, mtime: at, text: svgSource, language: "text", mime: "image/svg+xml")
        case let p where p.hasSuffix(".png"):
            return FsRead(tree: tree, path: p, kind: .image, size: png.count, mtime: at, mime: "image/png", data: png)
        case let p where p.hasSuffix(".log"):
            return FsRead(tree: tree, path: p, kind: .tooLarge, size: 48 * 1024 * 1024, mtime: at)
        case let p where p.hasSuffix(".zip"):
            return FsRead(tree: tree, path: p, kind: .binary, size: 182_044, mtime: at)
        default:
            return FsRead(tree: tree, path: path, kind: .unreadable, size: 0, mtime: 0)
        }
    }

    /// `count` repo-like paths, in a stable order, for go-to-file and its bench.
    public static func repoPaths(_ count: Int = 50_000) -> [String] {
        let dirs = [
            "packages/desktop/crates/pocket/src/chat", "packages/desktop/crates/git/src", "packages/ios/Packages/AnywhereKit/Sources/Stores",
            "packages/pocketd/internal/wsserver", "node_modules/.pnpm/react@19/node_modules/react/cjs", "docs/mobile", "scripts",
        ]
        let names = ["view", "model", "store", "row", "lanes", "status", "golden", "explorer", "preview", "markdown"]
        let exts = ["rs", "swift", "go", "json", "md", "ts", "toml"]
        return (0..<count).map { "\(dirs[$0 % dirs.count])/\($0 % 97)/\(names[$0 % names.count])_\($0).\(exts[$0 % exts.count])" }
    }

    public static let searchResults = [
        "packages/desktop/crates/pocket/src/explorer.rs", "packages/desktop/crates/pocket/src/explorer/preview.rs",
        "packages/desktop/crates/pocket/src/explorer/files.rs", "packages/pocketd/internal/fsview/search.go",
        "packages/ios/Packages/AnywhereKit/Sources/FeatureFiles/FileTreeScreen.swift", "docs/mobile/research-git.md",
    ]
}
