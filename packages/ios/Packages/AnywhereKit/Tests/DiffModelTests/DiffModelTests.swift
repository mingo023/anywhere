import Fixtures
import Testing
import Wire
@testable import DiffModel

@Suite("Diff rows")
struct DiffRowsTests {
    @Test("git.diff rows keep their kinds and line numbers, keyed by position")
    func rows() {
        let lines = DiffRows.lines([
            DiffRow(k: .hunk, t: "@@ -1,2 +1,2 @@"), DiffRow(k: .context, o: 1, n: 1, t: "a"), DiffRow(k: .deleted, o: 2, t: "b"),
            DiffRow(k: .added, n: 2, t: "c", cut: true),
        ])
        #expect(lines.map(\.kind) == [.hunk, .context, .deleted, .added])
        #expect(lines.map(\.id) == [0, 1, 2, 3])
        #expect(lines[2].old == 2 && lines[2].new == nil && lines[3].new == 2 && lines[3].cut)
    }

    @Test("word ranges move from UTF-8 bytes to UTF-16 offsets")
    func words() {
        #expect(DiffRows.utf16("let b = 1;", utf8: [[4, 5]]) == [4..<5])
        #expect(DiffRows.utf16("é = 2", utf8: [[5, 6]]) == [4..<5])
        #expect(DiffRows.utf16("a 😀 b", utf8: [[2, 6]]) == [2..<4])
    }

    @Test("a range outside the text or inside a character is dropped")
    func badWords() {
        #expect(DiffRows.utf16("é", utf8: [[1, 2]]).isEmpty)
        #expect(DiffRows.utf16("ab", utf8: [[1, 9], [2, 1]]).isEmpty)
    }

    @Test("split pairs deletions with additions")
    func split() {
        let lines = DiffRows.lines([
            DiffRow(k: .hunk, t: "@@ -45,4 +45,5 @@"), DiffRow(k: .context, o: 45, n: 45, t: "ctx"), DiffRow(k: .deleted, o: 46, t: "old"),
            DiffRow(k: .added, n: 46, t: "new1"), DiffRow(k: .added, n: 47, t: "new2"), DiffRow(k: .context, o: 47, n: 48, t: "ctx2"),
        ])
        let rows = DiffRows.split(lines).map { ($0.left.map { lines[$0].text }, $0.right.map { lines[$0].text }) }
        #expect(rows.count == 5)
        #expect(rows[2] == ("old", "new1"))
        #expect(rows[3] == (nil, "new2"))
    }

    @Test("each side's text is its own lines in order, and every row knows its line in the side it is coloured from")
    func sides() {
        let lines = DiffRows.lines([
            DiffRow(k: .hunk, t: "@@ -1,3 +1,3 @@"), DiffRow(k: .context, o: 1, n: 1, t: "a"), DiffRow(k: .deleted, o: 2, t: "\"b"),
            DiffRow(k: .added, n: 2, t: "c"), DiffRow(k: .context, o: 3, n: 3, t: "d"),
        ])
        let s = DiffRows.sides(lines)
        #expect(s.old == ["a", "\"b", "d"])
        #expect(s.new == ["a", "c", "d"])
        #expect(s.at == [nil, 0, 1, 1, 2])
    }

    @Test("a document says when it has nothing to draw, and why")
    func states() {
        let empty = GitDiff(tree: "/t", path: "a", language: "text", added: 0, removed: 0, lines: [])
        #expect(DiffDocument(empty).state == .unchanged)
        var binary = empty
        binary.binary = true
        #expect(DiffDocument(binary).state == .binary)
        binary.image = true
        #expect(DiffDocument(binary).state == .image)
        var large = empty
        large.tooLarge = true
        #expect(DiffDocument(large).state == .tooLarge)
        let rows = DiffDocument(Sample.diff(lines: 200))
        #expect(rows.state == .rows && rows.gutterDigits >= 3)
    }
}

@Suite("Edit previews")
struct EditPreviewTests {
    @Test("diff lines map onto git lines")
    func mapsOntoGitLines() {
        let diff = FileDiff(
            lines: [
                DiffLine(number: 4, kind: .context, text: "fn a() {"), DiffLine(number: 5, kind: .del, text: "    old();"),
                DiffLine(number: 6, kind: .del, text: "    gone();"), DiffLine(number: 5, kind: .add, text: "    new();"),
                DiffLine(number: 6, kind: .context, text: "}"), DiffLine(number: 20, kind: .context, text: "fn b() {"),
                DiffLine(number: 21, kind: .add, text: "    more();"),
            ], additions: 2, deletions: 2)
        let got = EditPreview.lines(diff).map { "\($0.kind) \($0.old.map(String.init) ?? "-") \($0.new.map(String.init) ?? "-") \($0.text)" }
        #expect(
            got == [
                "hunk - - @@ -4 +4 @@", "context 4 4 fn a() {", "deleted 5 -     old();", "deleted 6 -     gone();", "added - 5     new();",
                "context 7 6 }", "hunk - - @@ -21 +20 @@", "context 21 20 fn b() {", "added - 21     more();",
            ])
    }
}

private func s(_ shas: String...) -> [String] { shas }
private func shas(_ lanes: [Lane]) -> [String] { lanes.map(\.sha) }

@Suite("Graph lanes")
struct LanesTests {
    @Test("a commit takes the first lane waiting for it")
    func firstLane() {
        var lanes = Lanes()
        _ = lanes.push("c", parents: s("b"))
        let row = lanes.push("b", parents: s("a"))
        #expect(row.column == 0 && shas(row.input) == ["b"] && shas(row.output) == ["a"])
        #expect(row.output[0].color == row.input[0].color)
    }

    @Test("a merge opens a lane for its second parent")
    func merge() {
        var lanes = Lanes()
        let row = lanes.push("m", parents: s("a", "b"))
        #expect(shas(row.output) == ["a", "b"])
        #expect(row.output[0].color != row.output[1].color)
    }

    @Test("two lanes waiting for one commit converge on it")
    func converge() {
        var lanes = Lanes()
        _ = lanes.push("m", parents: s("a", "b"))
        let b = lanes.push("b", parents: s("a"))
        #expect(b.column == 1 && shas(b.output) == ["a", "a"])
        let a = lanes.push("a", parents: s("z"))
        #expect(a.column == 0 && shas(a.output) == ["z"])
    }

    @Test("lanes right of a closed lane shift left")
    func shift() {
        var lanes = Lanes()
        _ = lanes.push("t", parents: s("a", "b"))
        _ = lanes.push("u", parents: s("a"))
        _ = lanes.push("v", parents: s("c"))
        #expect(shas(lanes.push("a", parents: s("z")).output) == ["z", "b", "c"])
    }

    @Test("a root commit keeps the other lanes open")
    func root() {
        var lanes = Lanes()
        _ = lanes.push("t", parents: s("a", "b"))
        #expect(shas(lanes.push("a", parents: []).output) == ["b"])
    }

    @Test("a lane takes the upstream colour below the upstream tip")
    func upstream() {
        let label: (String) -> LaneColor? = { $0 == "h" ? .head : $0 == "u" ? .upstream : nil }
        var lanes = Lanes()
        _ = lanes.push("h", parents: s("u"), label: label)
        let row = lanes.push("u", parents: s("p"), label: label)
        #expect(row.input[0].color == .head)
        #expect(row.output == [Lane(sha: "p", color: .upstream)])
        #expect(row.color == .upstream)
    }

    @Test("a row's strokes pass other lanes by, end waiting lanes in the dot and leave it for each parent")
    func edges() {
        var lanes = Lanes()
        _ = lanes.push("t", parents: s("a", "b"))
        _ = lanes.push("u", parents: s("a"))
        let row = lanes.push("a", parents: s("z", "y"))
        let e = GraphEdges(row, sha: "a", parents: 2)
        #expect(shas(row.input) == ["a", "b", "a"] && shas(row.output) == ["z", "b", "y"])
        #expect(e.into.map(\.column) == [0, 2])
        #expect(e.through.map { [$0.from, $0.to] } == [[1, 1]])
        #expect(e.out.map(\.column) == [0, 2])
        let root = GraphEdges(lanes.push("z", parents: []), sha: "z", parents: 0)
        #expect(root.into.map(\.column) == [0] && root.out.isEmpty && root.through.map { [$0.from, $0.to] } == [[1, 0], [2, 1]])
    }

    @Test("a layout continues the lanes of the page before it")
    func pages() {
        let commits = Sample.log(commits: 120).commits
        var whole = GraphLayout(tips: LogTips(head: commits[0].sha))
        whole.append(commits)
        var paged = GraphLayout(tips: LogTips(head: commits[0].sha))
        paged.append(Array(commits[..<50]))
        paged.append(Array(commits[50...]))
        #expect(paged.rows == whole.rows)
        #expect(whole.width >= 2)
    }
}

private func file(_ path: String) -> GitFile { GitFile(path: path, status: .modified, staged: false, unstaged: true) }
private func staged(_ path: String) -> GitFile { GitFile(path: path, status: .modified, staged: true, unstaged: false) }

private func rows(_ files: [GitFile], tree: Bool, folded: Set<String> = []) -> [String] {
    ChangeTree.rows(files, tree: tree, folded: folded).map { row in
        switch row {
        case .header(let s, let paths): "\(s.rawValue): \(paths.joined(separator: " "))"
        case .folder(let s, _, let label, let depth, _): "\(String(repeating: "  ", count: depth + 1))\(s.rawValue):\(label)/"
        case .file(let s, let f, let depth): "\(String(repeating: "  ", count: depth + 1))\(s.rawValue):\(f.path)"
        }
    }
}

private func labels(_ files: [GitFile], folded: Set<String> = []) -> [String] {
    var out: [ChangeRow] = []
    ChangeTree.tree(files[...], base: "", depth: 0, section: .changes, folded: folded, into: &out)
    return out.map { row in
        switch row {
        case .folder(_, _, let label, let depth, _): "\(String(repeating: "  ", count: depth))\(label)/"
        case .file(_, let f, let depth): "\(String(repeating: "  ", count: depth))\(f.path.split(separator: "/").last!)"
        case .header: ""
        }
    }
}

@Suite("Change tree")
struct ChangeTreeTests {
    @Test("change rows list staged files then the rest, each under its header")
    func sections() {
        var both = file("both.rs")
        both.staged = true
        let untracked = GitFile(path: "new.rs", status: .untracked, staged: false, unstaged: true)
        #expect(
            rows([file("a.rs"), staged("b.rs"), both, untracked], tree: false) == [
                "staged: b.rs both.rs", "  staged:b.rs", "  staged:both.rs", "changes: a.rs both.rs new.rs", "  changes:a.rs", "  changes:both.rs",
                "  changes:new.rs",
            ])
    }

    @Test("change rows skip empty sections and keep a folded section's header with all its paths")
    func foldedSection() {
        let files = [file("a.rs"), file("src/b.rs")]
        #expect(rows(files, tree: false, folded: ["changes"]) == ["changes: a.rs src/b.rs"])
        #expect(rows(files, tree: true, folded: ["changes"]) == ["changes: a.rs src/b.rs"])
    }

    @Test("change rows fold a folder only in its own section")
    func foldedFolder() {
        #expect(
            rows([file("src/a.rs"), staged("src/b.rs")], tree: true, folded: ["changes:src/"]) == [
                "staged: src/b.rs", "  staged:src/", "    staged:src/b.rs", "changes: src/a.rs", "  changes:src/",
            ])
    }

    @Test("toggling a fold twice opens it again")
    func toggle() {
        var folded: Set<String> = ["staged"]
        ChangeTree.toggleFold(&folded, "changes:src/")
        #expect(folded == ["staged", "changes:src/"])
        ChangeTree.toggleFold(&folded, "changes:src/")
        #expect(folded == ["staged"])
    }

    @Test("tree lists folders first and merges single-folder chains")
    func chains() {
        let files = [file("README.md"), file("src/hooks/use-a.ts"), file("src/hooks/use-b.ts"), file("src/app/main.ts"), file("docs/x/y/z.md")]
        #expect(labels(files) == ["docs/x/y/", "  z.md", "src/", "  app/", "    main.ts", "  hooks/", "    use-a.ts", "    use-b.ts", "README.md"])
    }

    @Test("tree hides what a folded folder holds")
    func hides() {
        #expect(labels([file("src/a.rs"), file("src/b/c.rs"), file("d.rs")], folded: ["changes:src/"]) == ["src/", "d.rs"])
    }

    @Test("row ids are stable and unique across sections")
    func ids() {
        let all = ChangeTree.rows(Sample.gitStatus(files: 400).files, tree: true, folded: [])
        #expect(Set(all.map(\.id)).count == all.count)
    }

    @Test("staging leaves each picked file wholly staged or wholly unstaged")
    func flip() {
        var both = file("both.rs")
        both.staged = true
        var files = [file("a.rs"), both, file("c.rs")]
        ChangeTree.flipStaged(&files, paths: ["a.rs", "both.rs"], staged: true)
        #expect(files == [staged("a.rs"), staged("both.rs"), file("c.rs")])
        files = [staged("a.rs"), both]
        ChangeTree.flipStaged(&files, paths: ["a.rs", "both.rs"], staged: false)
        #expect(files == [file("a.rs"), file("both.rs")])
    }
}

@Suite("Commit composer")
struct ComposerTests {
    @Test("nothing commits while git works")
    func busy() {
        for kind in [CommitKind.commit, .push, .amend] {
            #expect(Composer.step(files: [staged("a.rs")], kind: kind, message: "Edit a", busy: true, commitAll: true) == .skip)
        }
    }

    @Test("with nothing changed only amend runs")
    func nothing() {
        #expect(Composer.step(files: [], kind: .commit, message: "Edit a", busy: false, commitAll: true) == .skip)
        #expect(Composer.step(files: [], kind: .push, message: "Edit a", busy: false, commitAll: true) == .skip)
        #expect(Composer.step(files: [], kind: .amend, message: "", busy: false, commitAll: true) == .run(all: false, busy: "Amending…"))
    }

    @Test("a commit without a message asks for one but amend keeps the last")
    func message() {
        let files = [staged("a.rs")]
        #expect(Composer.step(files: files, kind: .commit, message: "", busy: false, commitAll: true) == .askMessage)
        #expect(Composer.step(files: files, kind: .push, message: "", busy: false, commitAll: true) == .askMessage)
        #expect(Composer.step(files: files, kind: .amend, message: "", busy: false, commitAll: true) == .run(all: false, busy: "Amending…"))
    }

    @Test("a commit takes every change when none is staged and only the staged otherwise")
    func all() {
        let loose = [file("a.rs"), GitFile(path: "new.rs", status: .untracked, staged: false, unstaged: true)]
        #expect(Composer.step(files: loose, kind: .commit, message: "Edit", busy: false, commitAll: true) == .run(all: true, busy: "Committing…"))
        #expect(Composer.step(files: loose, kind: .push, message: "Edit", busy: false, commitAll: true) == .run(all: true, busy: "Committing…"))
        #expect(Composer.step(files: loose, kind: .amend, message: "Edit", busy: false, commitAll: true) == .run(all: false, busy: "Amending…"))
        #expect(
            Composer.step(files: [file("a.rs"), staged("b.rs")], kind: .commit, message: "Edit", busy: false, commitAll: true)
                == .run(all: false, busy: "Committing…"))
    }

    @Test("the commit button says Commit All when nothing is staged, and what git does while busy")
    func label() {
        #expect(Composer.label(files: [file("a.rs")], busy: nil, commitAll: true) == "Commit All")
        #expect(Composer.label(files: [file("a.rs"), staged("b.rs")], busy: nil, commitAll: true) == "Commit")
        #expect(Composer.label(files: [], busy: nil, commitAll: true) == "Commit")
        #expect(Composer.label(files: [file("a.rs")], busy: "Pushing…", commitAll: true) == "Pushing…")
    }

    @Test("commit is ready with changes, a message and git idle")
    func ready() {
        let files = [file("a.rs")]
        #expect(Composer.ready(files: files, message: "Edit a", busy: false, commitAll: true))
        #expect(!Composer.ready(files: files, message: " \n", busy: false, commitAll: true))
        #expect(!Composer.ready(files: [], message: "Edit a", busy: false, commitAll: true))
        #expect(!Composer.ready(files: files, message: "Edit a", busy: true, commitAll: true))
    }

    @Test("staged-only asks to stage first instead of committing everything")
    func stagedOnly() {
        let loose = [file("a.rs")]
        #expect(Composer.step(files: loose, kind: .commit, message: "Edit", busy: false, commitAll: false) == .stageFirst)
        #expect(Composer.step(files: loose, kind: .amend, message: "", busy: false, commitAll: false) == .run(all: false, busy: "Amending…"))
        #expect(Composer.step(files: [staged("a.rs")], kind: .commit, message: "Edit", busy: false, commitAll: false) == .run(all: false, busy: "Committing…"))
        #expect(Composer.label(files: loose, busy: nil, commitAll: false) == "Commit")
        #expect(!Composer.ready(files: loose, message: "Edit", busy: false, commitAll: false))
        #expect(Composer.ready(files: [staged("a.rs")], message: "Edit", busy: false, commitAll: false))
    }
}
