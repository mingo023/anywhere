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
                GalleryScene("git-diff\(suffix)", dark: dark) {
                    screen { DiffScreen(host: $0, tree: Sample.tree, path: Sample.smallDiff.path, at: .working, siblings: [Sample.smallDiff.path]) }
                },
                GalleryScene("git-diff-split\(suffix)", size: CGSize(width: 1024, height: 768), dark: dark) {
                    screen { DiffScreen(host: $0, tree: Sample.tree, path: Sample.smallDiff.path, at: .working, siblings: []) }
                },
                GalleryScene("git-history\(suffix)", dark: dark) { screen { HistoryScreen(host: $0, mac: Sample.mac, tree: Sample.tree) } },
                GalleryScene("git-commit\(suffix)", dark: dark) { screen { CommitScreen(host: $0, tree: Sample.tree, sha: Sample.commit.sha) } },
            ]
        }
    }

    @MainActor private static func screen(@ViewBuilder _ content: (HostModel) -> some View) -> AnyView {
        let host = HostModel(credentials: Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "gallery", macName: "Mac", clientId: "gallery"))
        host.git.take(Sample.gitStatus(files: 40))
        host.git.take(Sample.log(commits: 50), tree: Sample.tree)
        host.git.take(Sample.smallDiff, at: .working)
        host.git.take(Sample.commit)
        return AnyView(NavigationStack { content(host) })
    }
}
