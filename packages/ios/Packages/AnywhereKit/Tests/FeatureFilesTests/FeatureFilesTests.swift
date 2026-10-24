import Fixtures
import Foundation
import Stores
import Testing
import Wire
@testable import FeatureFiles

private func read(_ kind: FileKind, size: Int = 4, text: String? = nil, language: String? = nil, mime: String? = nil, data: Data? = nil) -> FsRead {
    FsRead(tree: "/r", path: "x", kind: kind, size: size, mtime: 1, text: text, language: language, mime: mime, data: data)
}

@Suite("File preview")
struct PreviewBodyTests {
    @Test("markdown text renders unless its source is asked for; code shows as code")
    func markdown() {
        let md = read(.text, text: "# hi", language: "markdown")
        #expect(PreviewBody.of(md, path: "README.md", source: false) == .markdown("# hi"))
        #expect(PreviewBody.of(md, path: "README.md", source: true) == .code("# hi", language: "markdown"))
        #expect(PreviewBody.of(read(.text, text: "fn a() {}", language: "rust"), path: "a.rs", source: false) == .code("fn a() {}", language: "rust"))
        #expect(PreviewBody.hasSource(md, path: "README.md"))
        #expect(!PreviewBody.hasSource(read(.text, text: "x", language: "rust"), path: "a.rs"))
    }

    @Test("files without text show the image or say why not, and nothing while loading")
    func notes() {
        #expect(PreviewBody.of(read(.binary, size: 4), path: "x.md", source: false) == .note("Binary file · 4 B", systemImage: "doc.zipper"))
        #expect(
            PreviewBody.of(read(.tooLarge, size: 3 * 1024 * 1024), path: "big.log", source: false)
                == .note("Too large to preview · 3.0 MB", systemImage: "doc.badge.ellipsis"))
        #expect(PreviewBody.of(read(.unreadable), path: "gone.rs", source: false) == .note("This file can't be shown.", systemImage: "eye.slash"))
        #expect(PreviewBody.of(read(.image, mime: "image/png", data: Sample.png), path: "logo.png", source: false) == .image(Sample.png))
        #expect(PreviewBody.of(.loading, path: "a.rs", source: false) == .loading)
        #expect(PreviewBody.of(.failed("Your Mac is offline"), path: "a.rs", source: false) == .note("Your Mac is offline", systemImage: "exclamationmark.triangle"))
    }

    @Test("an SVG draws unless its source is asked for")
    func svg() {
        let svg = read(.text, text: "<svg/>", language: "text", mime: "image/svg+xml")
        #expect(PreviewBody.of(svg, path: "icon.svg", source: false) == .svg("<svg/>"))
        #expect(PreviewBody.of(svg, path: "icon.svg", source: true) == .code("<svg/>", language: "text"))
        #expect(PreviewBody.hasSource(svg, path: "icon.svg"))
    }

    @Test("a cut file says how much of it shows, and the subtitle names its language and size")
    func lines() {
        var cut = read(.text, size: 3 * 1024 * 1024, text: String(repeating: "a", count: 512 * 1024), language: "text")
        cut.truncated = true
        #expect(PreviewBody.truncation(cut) == "Showing the first 512.0 KB of 3.0 MB")
        #expect(PreviewBody.truncation(read(.text, text: "a")) == nil)
        #expect(PreviewBody.subtitle(read(.text, size: 2048, text: ""), path: "a.rs") == "Rust · 2.0 KB")
        #expect(PreviewBody.subtitle(read(.image, size: 612), path: "a.png") == "612 B")
        #expect(PreviewBody.subtitle(read(.unreadable), path: "a.rs") == nil)
    }

    @Test("the SVG page loads nothing but itself and follows the appearance")
    func svgPage() {
        let page = SVGView.page("<svg/>")
        #expect(page.contains("default-src 'none'"))
        #expect(page.contains("color-scheme: light dark"))
        #expect(page.contains("<body><svg/></body>"))
    }
}

@Suite("File tree rows")
struct FileRowTests {
    @Test("git letters read as the Explorer's words and colours")
    func letters() {
        #expect(GitLetter.word("A") == "Added")
        #expect(GitLetter.word("?") == "Untracked")
        #expect(GitLetter.word("M") == "Modified")
        #expect(GitLetter.color("?") == GitLetter.color("A"))
        #expect(GitLetter.color("D") != GitLetter.color("M"))
    }

    @Test("a row's icon says folder, open folder, image, text or code")
    func icons() {
        let row = { (name: String, dir: Bool, open: Bool) in
            FileRow(path: name, entry: FsEntry(name: name, dir: dir, size: 0, mtime: 0), depth: 0, open: open)
        }
        #expect(FileRowView.icon(row("src", true, false)) == "folder")
        #expect(FileRowView.icon(row("src", true, true)) == "folder.fill")
        #expect(FileRowView.icon(row("logo.PNG", false, false)) == "photo")
        #expect(FileRowView.icon(row("README.md", false, false)) == "doc.text")
        #expect(FileRowView.icon(row("main.rs", false, false)) == "chevron.left.forwardslash.chevron.right")
        #expect(FileRowView.icon(row("Makefile", false, false)) == "doc")
    }

    @Test("a go-to-file hit marks the letters that matched in the base name")
    func hits() {
        let hit = SearchHit(path: "packages/pocket/src/explorer.rs", query: "expl")
        #expect(hit.name == "explorer.rs")
        #expect(hit.folder == "packages/pocket/src")
        #expect(hit.marked == [0, 1, 2, 3])
        #expect(SearchHit(path: "docs/naïve.md", query: "ve").marked == [3, 4])
        #expect(SearchHit(path: "a.rs", query: "").marked.isEmpty)
    }
}

@Suite("Image preview")
struct ZoomableImageTests {
    @Test("an image decodes off the main actor, downsampled to the longest side asked")
    func decodes() throws {
        let full = try #require(ZoomableImage.decode(Sample.png, maxPixels: 4096))
        #expect((full.width, full.height) == (160, 100))
        let small = try #require(ZoomableImage.decode(Sample.png, maxPixels: 80))
        #expect((small.width, small.height) == (80, 50))
        #expect(ZoomableImage.decode(Data("not an image".utf8), maxPixels: 80) == nil)
    }

    @Test("zoom stays between a little under fit and six times")
    func clamps() {
        #expect(ZoomableImage.clamp(0.1) == 0.8)
        #expect(ZoomableImage.clamp(3) == 3)
        #expect(ZoomableImage.clamp(40) == 6)
    }
}

@Suite("Files gallery")
@MainActor
struct FilesGalleryTests {
    @Test("every screen has a light and a dark scene, and the sample tree opens where the scenes say")
    func scenes() {
        let names = FilesGallery.scenes.map(\.name)
        #expect(names.count == Set(names).count)
        #expect(names.filter { $0.hasSuffix("-dark") }.count * 2 == names.count)
        let rows = FilesGallery.store().tree(Sample.fileTree).rows.map(\.path)
        #expect(rows.contains("packages/pocketd/internal/fsview/search.go"))
        #expect(rows.contains("docs/mobile/plan.md"))
        #expect(!rows.contains("node_modules"))
    }
}
