import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

public enum FilesGallery {
    @MainActor public static var scenes: [GalleryScene] {
        [false, true].flatMap { dark in
            let suffix = dark ? "-dark" : ""
            return [
                GalleryScene("files-tree\(suffix)", dark: dark) { AnyView(NavigationStack { tree() }) },
                GalleryScene("files-go-to-file\(suffix)", dark: dark) { AnyView(goToFile()) },
                GalleryScene("files-code\(suffix)", dark: dark) { AnyView(preview("packages/desktop/crates/pocket/src/explorer.rs")) },
                GalleryScene("files-markdown\(suffix)", dark: dark) { AnyView(preview("docs/mobile/plan.md")) },
                GalleryScene("files-image\(suffix)", dark: dark) { AnyView(preview("docs/logo.png")) },
                GalleryScene("files-svg\(suffix)", dark: dark) { AnyView(preview("docs/icon.svg")) },
                GalleryScene("files-binary\(suffix)", dark: dark) { AnyView(preview("dist/app.zip")) },
                GalleryScene("files-too-large\(suffix)", dark: dark) { AnyView(preview("logs/pocketd.log")) },
            ]
        }
    }

    /// A store holding the sample tree as if pocketd had listed it.
    @MainActor static func store() -> FilesStore {
        let files = FilesStore()
        let model = files.tree(Sample.fileTree)
        for (dir, entries) in Sample.listings { _ = files.apply(.fsList(FsList(tree: Sample.fileTree, dir: dir, entries: entries))) }
        files.reveal(model, "packages/pocketd/internal/fsview")
        files.reveal(model, "docs/mobile")
        return files
    }

    @MainActor static func tree() -> some View {
        let files = store()
        return FileTreeScreen(files: files, model: files.tree(Sample.fileTree))
    }

    @MainActor static func goToFile() -> some View {
        let search = FileSearch(tree: Sample.fileTree, query: "expl", results: Sample.searchResults) { _ in Sample.searchResults }
        return GoToFileSheet(search: search) { _ in }
    }

    @MainActor static func preview(_ path: String) -> some View {
        let files = store()
        let preview = files.preview(tree: Sample.fileTree, path: path)
        _ = files.apply(.fsRead(Sample.fileRead(path)))
        return NavigationStack { FilePreviewScreen(files: files, preview: preview) }
    }
}
