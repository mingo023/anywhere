import Client
import Domain
import Fixtures
import Foundation
import Testing
import Wire
@testable import Stores

private func entry(_ name: String, dir: Bool = false, ignored: Bool? = nil, git: String? = nil) -> FsEntry {
    FsEntry(name: name, dir: dir, size: 1, mtime: 1, ignored: ignored, git: git)
}

private func paths(_ rows: [FileRow]) -> [String] { rows.map(\.path) }

@Suite("File tree")
struct FileTreeTests {
    @Test("the tree nests what an open folder holds and hides what a folded one holds")
    func nests() {
        var tree = FileTree(listings: [
            "": [entry("open", dir: true), entry("shut", dir: true), entry("a.rs")],
            "open": [entry("b.rs")],
            "shut": [entry("c.rs")],
        ])
        tree.toggle("open")
        let rows = tree.rows(FileFilter())
        #expect(paths(rows) == ["open", "open/b.rs", "shut", "a.rs"])
        #expect(rows.map(\.depth) == [0, 1, 0, 0])
        #expect(rows.map(\.open) == [true, false, false, false])
    }

    @Test("folding a folder keeps the open folders inside it, and reopening shows them open")
    func foldKeepsOpenSubfolders() {
        var tree = FileTree(listings: ["": [entry("a", dir: true)], "a": [entry("b", dir: true)], "a/b": [entry("c.rs")]])
        let opened = [tree.toggle("a"), tree.toggle("a/b"), tree.toggle("a")]
        #expect(opened == [true, true, false])
        #expect(paths(tree.rows(FileFilter())) == ["a"])
        #expect(tree.shown == [""])
        tree.toggle("a")
        #expect(paths(tree.rows(FileFilter())) == ["a", "a/b", "a/b/c.rs"])
        #expect(tree.shown == ["", "a", "a/b"])
    }

    @Test("revealing a folder opens it and every folder holding it")
    func reveal() {
        var tree = FileTree()
        tree.reveal("a/b/c")
        tree.reveal("")
        #expect(tree.open == ["a", "a/b", "a/b/c"])
        #expect(tree.shown == ["", "a", "a/b", "a/b/c"])
    }

    @Test("listing a folder as it already is changes nothing")
    func sameListing() {
        var tree = FileTree()
        let changed = [tree.listed("", [entry("a.rs")]), tree.listed("", [entry("a.rs")]), tree.listed("", [])]
        #expect(changed == [true, false, true])
    }

    @Test("a folder folded while its listing loads stays folded, and its listing waits for the next open")
    func toggledDuringLoad() {
        var tree = FileTree(listings: ["": [entry("a", dir: true)]])
        tree.toggle("a")
        #expect(tree.rows(FileFilter()).first?.loading == true)
        tree.toggle("a")
        tree.listed("a", [entry("x.rs")])
        #expect(paths(tree.rows(FileFilter())) == ["a"])
        tree.toggle("a")
        #expect(paths(tree.rows(FileFilter())) == ["a", "a/x.rs"])
    }

    @Test("ignored files hide unless asked, dotfiles show unless asked, as the desktop's defaults")
    func filters() {
        let tree = FileTree(listings: ["": [entry("target", dir: true, ignored: true), entry(".env", ignored: true), entry(".gitignore"), entry("a.rs")]])
        #expect(paths(tree.rows(FileFilter())) == [".gitignore", "a.rs"])
        #expect(paths(tree.rows(FileFilter(hideIgnored: false))) == ["target", ".env", ".gitignore", "a.rs"])
        #expect(paths(tree.rows(FileFilter(hideIgnored: false, hideDotfiles: true))) == ["target", "a.rs"])
    }

    @Test("a folder that is gone takes everything listed and open inside it")
    func gone() {
        var tree = FileTree(listings: ["": [entry("a", dir: true)], "a": [entry("b", dir: true)], "a/b": [], "ab": []], open: ["a", "a/b", "ab"])
        tree.gone("a")
        #expect(tree.listings.keys.sorted() == ["", "ab"])
        #expect(tree.open == ["ab"])
    }

    @Test("an open folder that couldn't be listed says why instead of loading, until it is asked again or listed")
    func folderFailure() {
        var tree = FileTree(listings: ["": [entry("a", dir: true)]], open: ["a"])
        let changed = [tree.failed("a", "Studio didn't answer in time"), tree.failed("a", "Studio didn't answer in time")]
        #expect(changed == [true, false])
        let row = tree.rows(FileFilter())[0]
        #expect(row.failure == "Studio didn't answer in time")
        #expect(!row.loading)
        let retried = tree.retrying("a")
        #expect(retried)
        #expect(tree.rows(FileFilter())[0].loading)
        tree.failed("a", "Studio didn't answer in time")
        tree.listed("a", [])
        #expect(tree.failures.isEmpty)
        #expect(tree.rows(FileFilter())[0].failure == nil)
    }
}

@Suite("Files store")
@MainActor
struct FilesStoreTests {
    @Test("a listing reply lands in its tree's model, and one for a tree no one opened is dropped")
    func listings() {
        let store = FilesStore()
        let model = store.tree(Sample.fileTree)
        #expect(store.tree(Sample.fileTree) === model)
        #expect(!model.loaded)
        #expect(store.apply(.fsList(FsList(tree: Sample.fileTree, dir: "", entries: Sample.listings[""]!))))
        #expect(model.loaded)
        #expect(model.rows.count == Sample.listings[""]!.filter { $0.ignored != true }.count)
        #expect(!store.apply(.fsList(FsList(tree: Sample.fileTree, dir: "", entries: Sample.listings[""]!))))
        #expect(!store.apply(.fsList(FsList(tree: "/elsewhere", dir: "", entries: []))))
    }

    @Test("changing the filter redraws every tree")
    func filter() {
        let store = FilesStore()
        let model = store.tree(Sample.fileTree)
        store.apply(.fsList(FsList(tree: Sample.fileTree, dir: "", entries: Sample.listings[""]!)))
        let shown = model.rows.count
        store.filter.hideIgnored = false
        #expect(model.rows.count == Sample.listings[""]!.count)
        #expect(model.rows.count > shown)
    }

    @Test("a failed root says why; a folder the Mac refused folds and forgets what it held, one lost to the link keeps it")
    func failures() {
        let model = FileTreeModel(tree: "/t")
        model.listed("", [entry("a", dir: true)])
        model.listed("a", [entry("b.rs")])
        _ = model.toggle("a")
        model.asked("a")
        model.failed("a", "Not connected to Studio", refused: false)
        #expect(model.rows.map(\.path) == ["a", "a/b.rs"])
        #expect(model.hasFailures)
        model.failed("a", "That folder is gone", refused: true)
        #expect(model.rows.map(\.path) == ["a"])
        #expect(model.waiting.isEmpty)
        model.failed("", "Not connected to Studio", refused: false)
        #expect(model.failure == "Not connected to Studio")
        model.listed("", [entry("a", dir: true)])
        #expect(model.failure == nil)
    }

    @Test("a folder whose listing was cancelled keeps waiting, so a reconnect lists it")
    func cancelledListing() {
        let model = FileTreeModel(tree: "/t")
        model.asked("a")
        model.failed("a", nil, refused: false)
        #expect(model.waiting == ["a"])
        #expect(!model.hasFailures)
    }

    @Test("a preview changes only when the file did, and a failed reload keeps what showed with a notice over it")
    func previews() {
        let store = FilesStore()
        let preview = store.preview(tree: Sample.fileTree, path: "src/explorer.rs")
        #expect(store.preview(tree: Sample.fileTree, path: "src/explorer.rs") === preview)
        #expect(preview.phase == .loading)
        let read = Sample.fileRead("src/explorer.rs")
        #expect(store.apply(.fsRead(read)))
        #expect(!store.apply(.fsRead(read)))
        preview.failed("Not connected to Studio")
        #expect(preview.phase == .loaded(read))
        #expect(preview.notice == "Not connected to Studio")
        var edited = read
        edited.mtime += 1
        edited.text = "fn main() {}\n"
        #expect(store.apply(.fsRead(edited)))
        #expect(preview.phase == .loaded(edited))
        #expect(preview.notice == nil)
    }

    @Test("reconnecting reads a failed preview again, and errors name the Mac by its hostname")
    func reconnectRereadsFailedPreviews() {
        let store = FilesStore()
        let preview = store.preview(tree: "/t", path: "a.rs")
        preview.failed("Not connected to the Mac")
        #expect(preview.phase == .failed("Not connected to the Mac"))
        store.connected(Sample.hello)
        #expect(preview.phase == .loading)
        #expect(store.describe(DaemonError.offline) == "Not connected to \(Sample.hello.hostname)")
    }

    @Test("an agent's change to a tree reads its open previews again, all but images")
    func gitChangedRereadsPreviews() async throws {
        let store = FilesStore()
        let code = store.preview(tree: Sample.fileTree, path: "src/explorer.rs")
        let image = store.preview(tree: Sample.fileTree, path: "docs/logo.png")
        store.apply(.fsRead(Sample.fileRead("src/explorer.rs")))
        store.apply(.fsRead(Sample.fileRead("docs/logo.png")))
        store.apply(.gitChanged(GitChanged(tree: Sample.fileTree, rev: "r2")))
        // With no connection the read fails, which shows it was asked for.
        try await waitFor { code.notice != nil }
        #expect(image.notice == nil)
    }

    @Test("only the most recently opened previews stay cached")
    func previewCache() {
        let store = FilesStore()
        let first = store.preview(tree: "/t", path: "0.rs")
        for n in 1...FilesStore.previewsKept { _ = store.preview(tree: "/t", path: "\(n).rs") }
        #expect(store.preview(tree: "/t", path: "0.rs") !== first)
    }

    @Test("mentions and quotes queue in order without repeats, and each chat takes only its own")
    func additions() {
        let store = FilesStore()
        store.mention(tree: "/t", path: "a.rs", for: "a1")
        store.mention(tree: "/t", path: "b.rs", for: "a1")
        store.mention(tree: "/t", path: "a.rs", for: "a1")
        store.quote("a.rs:2-3\n```rust\nlet x = 1;\n```", tree: "/t", for: "a2")
        store.mention(tree: "/t", path: "c.rs")
        #expect(store.takeAdditions(for: "a1", in: "/t") == ["@a.rs", "@b.rs", "@c.rs"])
        #expect(store.takeAdditions(for: "a1", in: "/t") == [])
        #expect(store.takeAdditions(for: "a2", in: "/t") == ["a.rs:2-3\n```rust\nlet x = 1;\n```"])
        #expect(store.additions.isEmpty)
    }

    @Test("a chat that takes one mention at a time gets the oldest on its tree")
    func oneMentionAtATime() {
        let store = FilesStore()
        store.quote("a.rs:1\n```\nx\n```", tree: "/t", for: "a1")
        store.mention(tree: "/t", path: "a.rs", for: "a1")
        store.mention(tree: "/t", path: "b.rs", for: "a1")
        #expect(store.takeMention(in: "/elsewhere") == nil)
        #expect(store.takeMention(in: "/t") == "a.rs")
        #expect(store.takeMention(in: "/t") == "b.rs")
        #expect(store.takeMention(in: "/t") == nil)
    }

    @Test("a mention joins the draft after a space, and a quote takes lines of its own")
    func draft() {
        #expect(FilesStore.draft("", adding: ["@a.rs", "@b.rs"]) == "@a.rs @b.rs ")
        #expect(FilesStore.draft("look at", adding: ["@a.rs"]) == "look at @a.rs ")
        #expect(FilesStore.draft("why", adding: ["a.rs:1\n```\nx\n```"]) == "why\n\na.rs:1\n```\nx\n```\n")
        #expect(FilesStore.draft("", adding: ["a.rs:1\n```\nx\n```", "@b.rs"]) == "a.rs:1\n```\nx\n```\n@b.rs ")
    }

    @Test("requests without a connection fail as offline, and errors name the Mac")
    func offline() async {
        let store = FilesStore()
        await #expect(throws: DaemonError.offline) { try await store.search(tree: "/t", query: "a") }
        await #expect(throws: AttachmentError.unsupportedType) { try await store.upload(Data([1]), name: "notes.txt") }
        #expect(store.describe(DaemonError.server(ServerError(message: "That path is outside the project", code: .outsideTree))) == "That path is outside the project")
        store.mac = "Studio"
        #expect(store.describe(DaemonError.timedOut) == "Studio didn't answer in time")
        #expect(store.describe(AttachmentError.empty) == "That image is empty")
        #expect(store.describe(DaemonError.cancelled) == nil)
    }
}

@Suite("Attachments")
struct AttachmentsTests {
    @Test("only the image types the desktop's composer takes, up to 20 MB, go up")
    func check() throws {
        for name in ["a.png", "b.JPG", "c.jpeg", "d.gif", "e.webp", "f.heic"] { try Attachments.check(Data([1]), name: name) }
        #expect(throws: AttachmentError.unsupportedType) { try Attachments.check(Data([1]), name: "a.svg") }
        #expect(throws: AttachmentError.unsupportedType) { try Attachments.check(Data([1]), name: "png") }
        #expect(throws: AttachmentError.empty) { try Attachments.check(Data(), name: "a.png") }
        #expect(throws: AttachmentError.tooLarge) { try Attachments.check(Data(count: Limits.attachTotal + 1), name: "a.png") }
    }

    @Test("an image goes in 512 KiB chunks numbered from 0, and only the last says so")
    func chunks() {
        let data = Data((0..<(Limits.attachChunk * 2 + 10)).map { UInt8($0 % 251) })
        #expect(Attachments.chunkCount(data.count) == 3)
        let parts = (0..<3).map { Attachments.chunk($0, of: data, uploadId: "u1", name: "a.png") }
        #expect(parts.map(\.data.count) == [Limits.attachChunk, Limits.attachChunk, 10])
        #expect(parts.map(\.last) == [false, false, true])
        #expect(parts.map(\.index) == [0, 1, 2])
        #expect(parts.reduce(Data()) { $0 + $1.data } == data)
        #expect(Attachments.chunk(0, of: Data([1]), uploadId: "u", name: "a.png").last)
    }

    @Test("an upload id is one pocketd takes")
    func ids() {
        let id = Attachments.newId()
        #expect(id.count <= 64 && id.allSatisfy { $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "-" || $0 == "_") })
    }
}

@Suite("Go to file")
@MainActor
struct FileSearchTests {
    @Test("results narrow at once as the query grows, then the Mac's answer replaces them")
    func narrows() async throws {
        let index = FuzzyIndex(Sample.repoPaths(2_000))
        let search = FileSearch(tree: "/t", delay: .milliseconds(5)) { query in index.search(query, limit: 50) }
        search.query = "expl"
        try await waitFor { !search.searching }
        let found = search.results
        #expect(!found.isEmpty)
        search.query = "explrs"
        #expect(search.results == FuzzyIndex(found).search("explrs", limit: found.count))
        try await waitFor { !search.searching }
        #expect(search.results == index.search("explrs", limit: 50))
    }

    @Test("only the answer to the latest query lands")
    func latestWins() {
        let search = FileSearch(tree: "/t") { _ in [] }
        search.query = "a"
        search.query = "ab"
        search.answer(1, .success(["late.rs"]))
        #expect(search.results.isEmpty)
        search.answer(2, .success(["ab.rs"]))
        #expect(search.results == ["ab.rs"])
        #expect(!search.searching)
    }

    @Test("a failed search says why, over what the query narrowed to")
    func failure() {
        let search = FileSearch(tree: "/t", mac: "Studio") { _ in [] }
        search.query = "a"
        search.answer(1, .success(["a.rs"]))
        search.query = "ab"
        search.answer(2, .failure(DaemonError.timedOut))
        #expect(search.failure == "Studio didn't answer in time")
        #expect(search.results == [])
    }
}

@MainActor
private func waitFor(_ done: @MainActor () -> Bool) async throws {
    for _ in 0..<200 where !done() { try await Task.sleep(for: .milliseconds(5)) }
    #expect(done())
}
