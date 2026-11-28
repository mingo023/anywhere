import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

/// The design's Files frames (31 and 32) on its sample worktree, then the states it doesn't draw.
public enum FilesGallery {
    @MainActor public static var scenes: [GalleryScene] {
        [("dark", true), ("light", false)].flatMap { suffix, dark -> [GalleryScene] in
            func scene(_ name: String, _ view: @escaping @MainActor () -> AnyView) -> GalleryScene {
                GalleryScene("\(name)-\(suffix)", dark: dark, view: view)
            }
            return [
                scene("31-files-tree") { AnyView(NavigationStack { designTree() }) },
                scene("32-file-preview-code") { AnyView(designPreview("src/checkout/checkout.ts", Design.checkout)) },
                scene("32b-file-preview-from-chat") {
                    AnyView(designPreview("src/checkout/checkout.ts", Design.checkout, context: Design.chat))
                },
                scene("32c-file-preview-markdown") { AnyView(designPreview("README.md", Design.readme, tree: Sample.fileTree, language: "markdown")) },
                scene("32d-files-offline") { AnyView(NavigationStack { offlineTree() }) },
                scene("33-files-tree-sample") { AnyView(NavigationStack { tree() }) },
                scene("33-files-go-to-file") { AnyView(goToFile()) },
                scene("33-files-markdown") { AnyView(preview("docs/mobile/plan.md")) },
                scene("33-files-image") { AnyView(preview("docs/logo.png")) },
                scene("33-files-svg") { AnyView(preview("docs/icon.svg")) },
                scene("33-files-binary") { AnyView(preview("dist/app.zip")) },
                scene("33-files-too-large") { AnyView(preview("logs/pocketd.log")) },
                // No Mac in the gallery, so these reads fail as offline.
                scene("33-files-failed") { AnyView(failedPreview()) },
                scene("33-files-folder-failed") { AnyView(NavigationStack { tree(at: "docs/adr") }) },
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

    @MainActor static func tree(at dir: String = "") -> some View {
        let files = store()
        return FileTreeScreen(files: files, model: files.tree(Sample.fileTree), dir: dir)
    }

    @MainActor static func failedPreview() -> some View {
        let files = store()
        return NavigationStack { FilePreviewScreen(files: files, preview: files.preview(tree: Sample.fileTree, path: "src/gone.rs")) }
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

    /// The design's worktree listed, with its first folders open.
    @MainActor private static func designTree() -> some View {
        let files = FilesStore()
        let model = files.tree(Design.tree)
        for (dir, entries) in Design.listings { _ = files.apply(.fsList(FsList(tree: Design.tree, dir: dir, entries: entries))) }
        files.reveal(model, "src/checkout")
        return FileTreeScreen(files: files, model: model)
    }

    @MainActor private static func offlineTree() -> some View {
        let files = FilesStore()
        let offline = FilesContext.Offline(mac: Design.mac, since: Design.tenFortyTwo)
        return FileTreeScreen(files: files, model: files.tree(Design.tree), context: FilesContext(offline: offline))
    }

    @MainActor private static func designPreview(
        _ path: String, _ text: String, tree: String = Design.tree, language: String = "typescript", context: FilesContext = FilesContext()
    ) -> some View {
        let files = FilesStore()
        let preview = files.preview(tree: tree, path: path)
        let read = ServerMessage.fsRead(FsRead(tree: tree, path: path, kind: .text, size: text.utf8.count, mtime: Sample.now, text: text, language: language))
        _ = files.apply(read)
        // The screen asks again on appear, which a store with no Mac fails; the answer landing after clears that.
        return NavigationStack { FilePreviewScreen(files: files, preview: preview, context: context) }
            .task {
                try? await Task.sleep(for: .milliseconds(100))
                _ = files.apply(read)
            }
    }
}

private enum Design {
    static let mac = "Minh's MacBook Pro"
    static let tree = "/Users/mingo/.worktrees/anywhere/flaky-checkout"

    /// The last 10:42 before now, in Unix milliseconds.
    static var tenFortyTwo: Int {
        let calendar = Calendar.current
        let now = Date()
        var at = calendar.date(bySettingHour: 10, minute: 42, second: 0, of: now) ?? now
        if at > now { at = calendar.date(byAdding: .day, value: -1, to: at) ?? now }
        return Int(at.timeIntervalSince1970 * 1000)
    }

    static var chat: FilesContext { FilesContext(target: .one(ChatSession(id: "a-flaky", title: "Fix flaky checkout test", agent: "Claude")), fromChat: true) }

    static let listings: [String: [FsEntry]] = {
        func dir(_ name: String, git: String? = nil) -> FsEntry { FsEntry(name: name, dir: true, size: 192, mtime: Sample.now, git: git) }
        func file(_ name: String, git: String? = nil) -> FsEntry { FsEntry(name: name, dir: false, size: 1_024, mtime: Sample.now, git: git) }
        return [
            "": [dir("src", git: "M"), dir("docs", git: "M"), dir("test", git: "M"), file("package.json"), file("README.md"), file("tsconfig.json")],
            "src": [dir("checkout"), dir("mocks", git: "M"), dir("theme")],
            "src/checkout": [file("cart.ts"), file("checkout.ts"), file("checkout.test.ts", git: "M")],
        ]
    }()

    static let checkout = """
        import { api } from '../api'
        import type { Cart, Order } from './types'

        const RETRIES = 2

        /** Submits the cart and waits for payment. */
        export async function submitOrder(cart: Cart): Promise<Order> {
          if (cart.items.length === 0) {
            throw new Error('Cart is empty')
          }

          let attempt = 0
          while (true) {
            try {
              const res = await api.post('/orders', cart)
              return res.data
            } catch (err) {
              if (++attempt > RETRIES) throw err
              await wait(250 * attempt)
            }
          }
        }

        function wait(ms: number) {
          return new Promise((r) => setTimeout(r, ms))
        }
        """

    static let readme = """
        # anywhere

        Watch and drive coding agents on your own Macs.

        ## Develop

        ```bash
        pnpm install
        pnpm dev
        ```

        ## Requirements

        - Node 22 or newer
        - pnpm 9
        - Tailscale on both devices
        """
}
