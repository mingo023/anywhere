import Testing
@testable import Domain

@Suite("File text")
struct FileTextTests {
    @Test("names languages and sizes as the desktop's Explorer")
    func languagesAndSizes() {
        #expect(FileText.language("src/a.tsx") == "TypeScript")
        #expect(FileText.language("Makefile") == "Plain text")
        #expect(FileText.size(612) == "612 B")
        #expect(FileText.size(2048) == "2.0 KB")
        #expect(FileText.size(3 * 1024 * 1024) == "3.0 MB")
    }

    @Test("a path splits into folder, base name and extension; a dotfile has none")
    func parts() {
        #expect(FileText.folder("src/chat/view.rs") == "src/chat")
        #expect(FileText.folder("README.md") == "")
        #expect(FileText.baseName("src/chat/view.rs") == "view.rs")
        #expect(FileText.ext("a/logo.PNG") == "PNG")
        #expect(FileText.ext(".env") == "")
        #expect(FileText.ext("Makefile") == "")
        #expect(FileText.isMarkdown("docs/PLAN.MD"))
    }

    @Test("a relative path joins its tree and folder")
    func joins() {
        #expect(FileText.absolute(tree: "/Users/me/app", "src/a.rs") == "/Users/me/app/src/a.rs")
        #expect(FileText.absolute(tree: "/Users/me/app", "") == "/Users/me/app")
        #expect(FileText.join("", "src") == "src")
        #expect(FileText.join("src", "a.rs") == "src/a.rs")
    }

    @Test("a markdown file's link finds its file beside it, inside the tree or nowhere")
    func links() {
        #expect(FileText.linked("lanes.md", from: "docs/README.md", tree: "/w") == "docs/lanes.md")
        #expect(FileText.linked("../src/./a.rs", from: "docs/README.md", tree: "/w") == "src/a.rs")
        #expect(FileText.linked("/w/src/a.rs", from: "docs/README.md", tree: "/w") == "src/a.rs")
        #expect(FileText.linked("../../etc/passwd", from: "docs/README.md", tree: "/w") == nil)
        #expect(FileText.linked("/etc/passwd", from: "README.md", tree: "/w") == nil)
    }
}
