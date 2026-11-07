import Client
import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

public enum GitGallery {
    @MainActor public static var scenes: [GalleryScene] {
        [false, true].flatMap { dark in
            let suffix = dark ? "-dark" : ""
            return [
                GalleryScene("git-changes\(suffix)", dark: dark) { screen { GitView(host: $0, route: .changes(mac: Sample.mac, tree: Sample.tree)) } },
                // The Mac dropped: the list stays, as of when, with its writes off.
                GalleryScene("git-changes-offline\(suffix)", dark: dark) {
                    screen(online: false) { GitView(host: $0, route: .changes(mac: Sample.mac, tree: Sample.tree)) }
                },
                GalleryScene("git-changes-unreachable\(suffix)", dark: dark) {
                    screen(online: false, status: false) { GitView(host: $0, route: .changes(mac: Sample.mac, tree: Sample.tree)) }
                },
                GalleryScene("git-compare\(suffix)", dark: dark) {
                    screen { CompareList(host: $0, tree: Sample.tree, against: "main") {} }
                },
                GalleryScene("git-diff\(suffix)", dark: dark) {
                    screen { DiffScreen(host: $0, tree: Sample.tree, path: Sample.smallDiff.path, at: .working, files: .list([Sample.smallDiff.path])) }
                },
                GalleryScene("git-diff-split\(suffix)", size: CGSize(width: 1024, height: 768), dark: dark) {
                    screen { DiffScreen(host: $0, tree: Sample.tree, path: Sample.smallDiff.path, at: .working, files: .list([])) }
                },
                // A long diff opens on its first hunk, at the left edge.
                GalleryScene("git-diff-long\(suffix)", size: CGSize(width: 1024, height: 768), dark: dark) {
                    screen { DiffScreen(host: $0, tree: Sample.tree, path: Sample.diff().path, at: .working, files: .list([])) }
                },
                GalleryScene("git-history\(suffix)", dark: dark) { screen { HistoryScreen(host: $0, mac: Sample.mac, tree: Sample.tree) } },
                GalleryScene("git-commit\(suffix)", dark: dark) { screen { CommitScreen(host: $0, tree: Sample.tree, sha: Sample.commit.sha) } },
            ]
        }
    }

    @MainActor private static func screen(online: Bool = true, status: Bool = true, @ViewBuilder _ content: (HostModel) -> some View) -> AnyView {
        let host =
            online
            ? HostModel.offline(hello: Sample.hello, messages: [])
            : HostModel(credentials: Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "gallery", macName: "Mac", clientId: "gallery"))
        if status { host.git.take(Sample.gitStatus(files: 40)) }
        host.git.take(Sample.log(commits: 50), tree: Sample.tree)
        host.git.take(Sample.smallDiff, at: .working)
        host.git.take(Sample.diff(lines: 2000), at: .working)
        host.git.take(Sample.commit)
        let files = Sample.gitStatus(files: 40).files.filter { $0.status != .untracked }
        host.git.take(GitCompare(
            tree: Sample.tree, against: "main", totals: CompareTotals(files: files.count, added: files.compactMap(\.added).reduce(0, +), removed: files.compactMap(\.removed).reduce(0, +)),
            files: files.map { CommitFile(path: $0.path, oldPath: $0.oldPath, status: $0.status, added: $0.added, removed: $0.removed) }))
        return AnyView(NavigationStack { content(host) })
    }
}
