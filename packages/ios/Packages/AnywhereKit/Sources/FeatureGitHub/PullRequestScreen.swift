import DesignSystem
import Stores
import SwiftUI
import Wire

/// A tree's pull request: the card, or why there is none, kept fresh while on screen.
@MainActor
struct PullRequestScreen: View {
    let store: GitHubStore
    let model: TreePullRequest
    /// The branch's commits since its base, newest first, to fill a new PR.
    let commits: [CommitRef]
    /// Asks the Mac again this often while on screen; the Mac answers from its cache while that is fresh.
    var every: Duration? = .seconds(30)

    @State private var creating = false

    init(store: GitHubStore, tree: String, commits: [CommitRef] = [], every: Duration? = .seconds(30)) {
        self.store = store
        model = store.pullRequest(tree)
        self.commits = commits
        self.every = every
    }

    var body: some View {
        content
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
                    NoticeBar(notice: notice) { Task { await store.undo(model.tree) } }
                }
            }
            .animation(.snappy, value: model.notice)
            .sheet(isPresented: $creating) {
                CreatePullRequestSheet(store: store, tree: model.tree, commits: commits)
            }
            .task(id: model.tree) {
                guard let every else { return }
                while !Task.isCancelled {
                    await store.refresh(model.tree)
                    try? await Task.sleep(for: every)
                }
            }
    }

    @ViewBuilder
    private var content: some View {
        if let pr = model.pr {
            PullRequestCard(store: store, model: model, pr: pr)
        } else if let problem = model.problem {
            GhProblemView(problem: problem)
        } else if model.loaded {
            Placeholder("No pull request", systemImage: "arrow.triangle.pull", detail: "This branch has no pull request yet.")
                .overlay(alignment: .bottom) {
                    Button { creating = true } label: {
                        Text("Create PR").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .controlSize(.large)
                    .padding(Space.l)
                }
        } else if let error = model.error {
            Placeholder("Couldn't load the pull request", systemImage: "exclamationmark.triangle", detail: error)
        } else {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity).screenBackground()
        }
    }
}

/// Why gh can't be asked, and what to run on the Mac about it.
@MainActor
struct GhProblemView: View {
    let problem: GhState

    var body: some View {
        if problem == .missing {
            Placeholder("Install gh to see PRs", systemImage: "terminal", detail: "GitHub CLI isn't on this Mac. Install it with brew install gh, then sign in.")
        } else {
            Placeholder("Run gh auth login to see PRs", systemImage: "person.crop.circle.badge.exclamationmark",
                        detail: "GitHub CLI on this Mac isn't signed in to github.com.")
        }
    }
}
