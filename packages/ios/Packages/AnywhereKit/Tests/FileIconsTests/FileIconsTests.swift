import Testing
@testable import FileIcons

@Suite("File icons")
struct FileIconsTests {
    @Test("a Material icon matches like VS Code: the whole name, then its extensions longest first", arguments: [
        ("CHANGELOG.md", false, false, "changelog"), ("src/.gitignore", false, false, "git"), ("a.test.ts", false, false, "test-ts"),
        ("main.rs", false, false, "rust"), ("pnpm-lock.yaml", false, false, "pnpm_light"), ("Makefile.unknown-ext", false, false, "file"),
        ("src", true, false, "folder-src"), ("src", true, true, "folder-src-open"), ("nothing-special", true, true, "folder-open"),
    ] as [(String, Bool, Bool, String)])
    func materialNames(_ path: String, _ folder: Bool, _ open: Bool, _ icon: String) {
        #expect(MaterialIcons.name(path, folder: folder, open: open) == icon)
    }

    @MainActor @Test("every icon the manifest names ships as a picture")
    func materialFiles() {
        #expect(!MaterialIcons.everyName.isEmpty)
        for name in MaterialIcons.everyName.union(["file", "folder", "folder-open"]) { #expect(MaterialIcons.image(name) != nil, "\(name)") }
    }
}
