import Client
import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

/// The design's Changes frames (27 to 30), on its sample worktree: flaky-checkout on Minh's MacBook Pro.
public enum GitGallery {
    @MainActor public static var scenes: [GalleryScene] {
        [("dark", true), ("light", false)].flatMap { suffix, dark -> [GalleryScene] in
            func scene(_ frame: String, size: CGSize = CGSize(width: 393, height: 852), _ view: @escaping @MainActor () -> AnyView) -> GalleryScene {
                GalleryScene("\(frame)-\(suffix)", size: size, dark: dark, view: view)
            }
            return [
                scene("27-changes") { screen(Design.status(Design.uncommitted)) { GitView(host: $0, route: .changes(mac: Sample.mac, tree: Design.tree)) } },
                scene("27b-changes-no-changes") { screen(Design.status([])) { GitView(host: $0, route: .changes(mac: Sample.mac, tree: Design.tree)) } },
                // The Mac dropped: the list stays, as of when, with its writes off.
                scene("27c-changes-offline") {
                    screen(Design.status(Design.uncommitted), online: false) { GitView(host: $0, route: .changes(mac: Sample.mac, tree: Design.tree)) }
                },
                scene("27d-changes-unreachable") {
                    screen(nil, online: false) { GitView(host: $0, route: .changes(mac: Sample.mac, tree: Design.tree)) }
                },
                scene("27e-changes-vs-main") { screen(Design.status(Design.uncommitted)) { CompareList(host: $0, tree: Design.tree, against: "main") {} } },
                scene("28-diff") {
                    screen(Design.status(Design.reviewed), diffs: [Design.checkoutDiff]) {
                        DiffScreen(host: $0, tree: Design.tree, path: Design.checkoutDiff.path, at: .working, files: .changes(.changes))
                    }
                },
                scene("28b-diff-truncated") {
                    screen(Design.status(Design.lockfile, added: 2147, removed: 1862), diffs: [Design.lockDiff]) {
                        DiffScreen(host: $0, tree: Design.tree, path: Design.lockDiff.path, at: .working, files: .changes(.changes))
                    }
                },
                scene("28c-diff-wide", size: CGSize(width: 1024, height: 768)) {
                    screen(Design.status(Design.reviewed), diffs: [Design.checkoutDiff]) {
                        DiffScreen(host: $0, tree: Design.tree, path: Design.checkoutDiff.path, at: .working, files: .changes(.changes))
                    }
                },
                scene("29-history") { screen(Design.status(Design.uncommitted)) { HistoryScreen(host: $0, mac: Sample.mac, tree: Design.tree) } },
                scene("30-commit-detail") {
                    screen(Design.status(Design.uncommitted), commit: Design.commit) { CommitScreen(host: $0, tree: Design.tree, sha: Design.commit.sha) }
                },
            ]
        }
    }

    @MainActor private static func screen(
        _ status: GitStatus?, online: Bool = true, diffs: [GitDiff] = [], commit: CommitDetail? = nil, @ViewBuilder _ content: (HostModel) -> some View
    ) -> AnyView {
        var hello = Sample.hello
        hello.hostname = Design.mac
        let host = HostModel.offline(hello: hello, messages: [], droppedFor: online ? nil : Design.secondsSince(hour: 10, minute: 42))
        if let status { host.git.take(status) }
        host.git.take(Design.log, tree: Design.tree)
        for diff in diffs { host.git.take(diff, at: .working) }
        if let commit { host.git.take(commit) }
        host.git.take(Design.compare)
        return AnyView(NavigationStack { content(host) })
    }
}

private enum Design {
    static let mac = "Minh's MacBook Pro"
    static let tree = "/Users/mingo/.worktrees/anywhere/flaky-checkout"
    static let branch = "flaky-checkout"
    static var now: Int { Int(Date().timeIntervalSince1970) }

    static func file(_ path: String, _ status: FileStatus, staged: Bool, added: Int, removed: Int) -> GitFile {
        GitFile(path: path, status: status, staged: staged, unstaged: !staged, added: added, removed: removed)
    }

    /// Frame 27: one file staged and three not.
    static let uncommitted = [
        file("src/checkout/checkout.test.ts", .modified, staged: true, added: 7, removed: 2),
        file("src/mocks/server.ts", .modified, staged: false, added: 2, removed: 1),
        file("test/setup.ts", .modified, staged: false, added: 1, removed: 0),
        file("docs/flaky-tests.md", .added, staged: false, added: 2, removed: 0),
    ]

    /// Frame 28: the same four, none staged, so one list steps through all of them.
    static let reviewed = uncommitted.map { file($0.path, $0.status, staged: false, added: $0.added ?? 0, removed: $0.removed ?? 0) }

    static let lockfile = [
        file("src/checkout/checkout.test.ts", .modified, staged: false, added: 7, removed: 2),
        file("pnpm-lock.yaml", .modified, staged: false, added: 2140, removed: 1860),
    ]

    static func status(_ files: [GitFile], added: Int? = nil, removed: Int? = nil) -> GitStatus {
        GitStatus(
            tree: tree, rev: "design", branch: branch, upstream: "origin/\(branch)", base: "main", ahead: 3, behind: 0, unpushed: 0, unpulled: 0,
            totals: GitTotals(
                files: files.count, added: added ?? files.reduce(0) { $0 + ($1.added ?? 0) }, removed: removed ?? files.reduce(0) { $0 + ($1.removed ?? 0) },
                staged: files.count(where: \.staged), unstaged: files.count(where: \.unstaged)),
            files: files, commits: [])
    }

    static func secondsSince(hour: Int, minute: Int) -> TimeInterval {
        let calendar = Calendar.current
        let now = Date()
        var at = calendar.date(bySettingHour: hour, minute: minute, second: 0, of: now) ?? now
        if at > now { at = calendar.date(byAdding: .day, value: -1, to: at) ?? now }
        return now.timeIntervalSince(at)
    }

    /// Commits newest first: `age` is seconds ago; `yesterday` ones sit the evening before today.
    static let log: GitLog = {
        func commit(_ sha: String, _ subject: String, _ author: String, ago: Int) -> LogCommit {
            LogCommit(sha: sha + String(repeating: "0", count: 40 - sha.count), parents: [], author: author, at: now - ago, subject: subject)
        }
        let midnight = Int(Calendar.current.startOfDay(for: Date()).timeIntervalSince1970)
        var yesterday = commit("9be2d15", "Log cart state when the test fails", "Claude", ago: 0)
        yesterday.at = midnight - 3 * 3_600
        return GitLog(
            tips: LogTips(), commits: [
                commit("a1c9e3f", "Await server.listen in checkout tests", "Minh", ago: 12 * 60),
                commit("7d02b4a", "Move payment mock into beforeEach", "Claude", ago: 25 * 60),
                commit("3f8e910", "Add repeat flag to the test script", "Claude", ago: 3_600),
                commit("c41aa07", "Merge main into flaky-checkout", "Minh", ago: 3 * 3_600),
                yesterday,
            ], more: false)
    }()

    static let commit = CommitDetail(
        sha: "a1c9e3f" + String(repeating: "0", count: 33), subject: "Await server.listen in checkout tests",
        body: "The payment mock registered its handlers after the first request, so about one run in five read an empty cart.", author: "Minh",
        at: now - 12 * 60,
        files: [
            CommitFile(path: "src/checkout/checkout.test.ts", status: .modified, added: 5, removed: 2),
            CommitFile(path: "src/mocks/server.ts", status: .modified, added: 2, removed: 1),
        ])

    static let compare = GitCompare(
        tree: tree, against: "main", totals: CompareTotals(files: 2, added: 31, removed: 9),
        files: [
            CommitFile(path: "src/checkout/checkout.test.ts", status: .modified, added: 18, removed: 6),
            CommitFile(path: "package.json", status: .modified, added: 13, removed: 3),
        ])

    private static func context(_ o: Int, _ n: Int, _ t: String) -> DiffRow { DiffRow(k: .context, o: o, n: n, t: t) }
    private static func added(_ n: Int, _ t: String) -> DiffRow { DiffRow(k: .added, n: n, t: t) }
    private static func deleted(_ o: Int, _ t: String) -> DiffRow { DiffRow(k: .deleted, o: o, t: t) }
    private static func hunk(_ t: String) -> DiffRow { DiffRow(k: .hunk, t: t) }

    static let checkoutDiff = GitDiff(
        tree: tree, path: "src/checkout/checkout.test.ts", language: "typescript", added: 7, removed: 2,
        lines: [
            hunk("@@ -18,11 +18,14 @@"),
            context(18, 18, "import { render } from '@testing-library/react'"),
            context(19, 19, "import { server } from '../mocks/server'"),
            context(20, 20, "import { submitOrder } from './checkout'"),
            context(21, 21, ""),
            deleted(22, "server.listen()"),
            added(22, "beforeEach(async () => {"),
            added(23, "  server.resetHandlers()"),
            added(24, "  await server.listen({ onUnhandledRequest: 'error' })"),
            added(25, "})"),
            context(23, 26, ""),
            context(24, 27, "describe('checkout', () => {"),
            context(25, 28, "  it('submits the cart', async () => {"),
            deleted(26, "    const order = submitOrder(cart)"),
            added(29, "    const order = await submitOrder(cart)"),
            context(27, 30, "    expect(order.items).toHaveLength(3)"),
            hunk("@@ -51,6 +54,8 @@ describe('checkout')"),
            context(51, 54, "  it('retries a declined card', async () => {"),
            context(52, 55, "    mockDecline(once)"),
            added(56, "    await server.ready()"),
            added(57, ""),
            context(53, 58, "    const order = await submitOrder(cart)"),
        ])

    static let lockDiff = GitDiff(
        tree: tree, path: "pnpm-lock.yaml", language: "yaml", added: 2140, removed: 1860,
        lines: [
            hunk("@@ -388,13 +388,13 @@ packages:"),
            context(388, 388, "  /@babel/core@7.26.0:"),
            context(389, 389, "    resolution: {integrity: sha512-i1SLeK+DzNnQ3LL/CswPCa}"),
            context(390, 390, "    engines: {node: '>=6.9.0'}"),
            context(391, 391, "    dependencies:"),
            deleted(392, "      '@babel/code-frame': 7.25.9"),
            added(392, "      '@babel/code-frame': 7.26.2"),
            deleted(393, "      '@babel/generator': 7.25.9"),
            added(393, "      '@babel/generator': 7.26.2"),
            context(394, 394, "      '@babel/helper-compilation-targets': 7.25.9"),
            deleted(395, "      '@babel/parser': 7.25.9"),
            added(395, "      '@babel/parser': 7.26.2"),
            context(396, 396, "      convert-source-map: 2.0.0"),
            context(397, 397, "      debug: 4.3.7"),
            context(398, 398, "      gensync: 1.0.0-beta.2"),
            context(399, 399, "      json5: 2.2.3"),
            context(400, 400, "      semver: 6.3.1"),
        ], truncated: Truncated(shown: 400, total: 4000))
}
