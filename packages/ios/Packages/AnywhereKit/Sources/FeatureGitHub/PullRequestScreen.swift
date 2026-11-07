import DesignSystem
import Stores
import SwiftUI
import Wire

/// The Mac a PR screen is on: its id for routes, its name for copy, and whether it is offline now.
struct PrMac: Hashable {
    var id = ""
    var name = "your Mac"
    var offline = false
}

/// What a new PR from the tree would open: its branch into its base, and the commits since that base, newest first.
struct NewPr: Hashable {
    var branch: String?
    var base: String?
    var ahead: Int?
    var commits: [CommitRef] = []

    /// The branch is off its base, so it can have a PR.
    var offered: Bool { PullRequestText.offersCreate(branch: branch, base: base) }

    var line: String? {
        guard let branch, let base else { return nil }
        return PullRequestText.headLine(branch: branch, base: base, ahead: ahead)
    }
}

/// A tree's pull request: the card, or why there is none, kept fresh while on screen.
@MainActor
struct PullRequestScreen: View {
    let store: GitHubStore
    let model: TreePullRequest
    let mac: PrMac
    let newPr: NewPr
    /// Set when the tree is a worktree this phone may delete once its PR merges.
    let cleanup: WorktreeCleanup?
    let deleteTree: (@MainActor () async throws -> Void)?
    /// The branches a new PR may open into.
    let branches: (@MainActor () async throws -> BranchList)?
    let fix: FixContext
    /// Asks the Mac again this often while on screen; the Mac answers from its cache while that is fresh.
    var every: Duration? = .seconds(30)

    @State private var creating = false

    init(
        store: GitHubStore, tree: String, mac: PrMac = PrMac(), newPr: NewPr = NewPr(), cleanup: WorktreeCleanup? = nil,
        deleteTree: (@MainActor () async throws -> Void)? = nil, branches: (@MainActor () async throws -> BranchList)? = nil,
        fix: FixContext = FixContext(), every: Duration? = .seconds(30)
    ) {
        self.store = store
        model = store.pullRequest(tree)
        self.mac = mac
        self.newPr = newPr
        self.cleanup = cleanup
        self.deleteTree = deleteTree
        self.branches = branches
        self.fix = fix
        self.every = every
    }

    var body: some View {
        content
            .safeAreaInset(edge: .top, spacing: 0) {
                if mac.offline && model.loaded {
                    StaleStrip(mac: mac.name, asOf: model.at)
                } else if model.stale && model.pr != nil {
                    UnreachableStrip { recheck() }
                }
            }
            .navigationTitle(model.pr.map { "#\($0.number)" } ?? "Pull request")
            .toolbar {
                if let url = model.pr.flatMap({ URL(string: $0.url) }) {
                    ToolbarItem(placement: .primaryAction) {
                        Link(destination: url) { Label("Open on GitHub", systemImage: "safari") }
                    }
                }
            }
            .overlay(alignment: .bottom) {
                if let notice = model.notice {
                    NoticeBar(notice: notice) { Task { await store.undo(model.tree) } }.disabled(mac.offline)
                }
            }
            .animation(.snappy, value: model.notice)
            .sheet(isPresented: $creating) {
                CreatePullRequestSheet(store: store, tree: model.tree, newPr: newPr, branches: branches, offline: mac.offline)
            }
            .task(id: model.tree) {
                guard let every else { return }
                while !Task.isCancelled {
                    await store.refresh(model.tree)
                    try? await Task.sleep(for: every)
                }
            }
    }

    private func recheck() {
        Task { await store.recheck(model.tree) }
    }

    private var createButton: some View {
        Button { creating = true } label: {
            Text("Create PR").frame(maxWidth: .infinity)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .disabled(mac.offline)
        .padding(Space.l)
    }

    @ViewBuilder
    private var content: some View {
        switch PullRequestPhase.of(hasPr: model.pr != nil, problem: model.problem, loaded: model.loaded, error: model.error) {
        case .card:
            if let pr = model.pr {
                PullRequestCard(store: store, model: model, pr: pr, mac: mac, cleanup: cleanup, deleteTree: deleteTree, fix: fix)
            }
        case .ghMissing, .ghSignedOut:
            Pullable(refresh: { await store.recheck(model.tree) }) {
                GhProblemView(problem: model.problem ?? .signedOut, mac: mac.name) { recheck() }
            }
        case .noRemote:
            Pullable(refresh: { await store.recheck(model.tree) }) {
                Placeholder("No GitHub remote", systemImage: "network.slash",
                            detail: "This project's repository has no GitHub remote, so its branches can't have pull requests.")
            }
        case .unreachable:
            Pullable(refresh: { await store.recheck(model.tree) }) {
                Placeholder("Couldn't reach GitHub", systemImage: "exclamationmark.icloud",
                            detail: "gh on \(mac.name) couldn't get this branch's pull request.") { recheck() }
            }
        case .none:
            Pullable(refresh: { await store.recheck(model.tree) }) {
                Placeholder("No pull request", systemImage: "arrow.triangle.pull", detail: newPr.line ?? "This branch has no pull request yet.")
            }
            .overlay(alignment: .bottom) {
                if newPr.offered { createButton }
            }
        case .failed(let error):
            Pullable(refresh: { await store.recheck(model.tree) }) {
                Placeholder("Couldn't load the pull request", systemImage: "exclamationmark.triangle", detail: error) { recheck() }
            }
        case .loading:
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity).screenBackground()
        }
    }
}

/// Why gh can't be asked, what to run on the Mac about it, and Check again once that's done.
@MainActor
struct GhProblemView: View {
    let problem: GhState
    let mac: String
    let checkAgain: (() -> Void)?

    var body: some View {
        let (title, detail) = Self.text(problem, mac: mac)
        Placeholder(title, systemImage: problem == .missing ? "terminal" : "person.crop.circle.badge.exclamationmark", detail: detail,
                    actionTitle: "Check Again", action: checkAgain)
    }

    nonisolated static func text(_ problem: GhState, mac: String) -> (title: String, detail: String) {
        problem == .missing
            ? ("GitHub CLI isn't installed", "On \(mac), run brew install gh, then gh auth login.")
            : ("GitHub CLI isn't signed in", "On \(mac), run gh auth login.")
    }
}
