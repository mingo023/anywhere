import ChatModel
import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// The PR card: where it stands, its checks, its review, what holds the merge, and its comments.
@MainActor
struct PullRequestCard: View {
    let store: GitHubStore
    let model: TreePullRequest
    let pr: PullRequest
    var mac = PrMac()
    var cleanup: WorktreeCleanup?
    var deleteTree: (@MainActor () -> Void)?
    var fix = FixContext()

    @State private var passedShown = false
    @State private var fixing: FixRequest?
    /// Delete after merging, once the reader has set it; until then the cleanup's default.
    @State private var deleteAfter: Bool?
    @State private var confirmingDelete = false

    var body: some View {
        List {
            header
            if pr.state == "OPEN" { callout }
            checks
            review
            if pr.state == "OPEN" {
                MergeSection(
                    store: store, model: model, pr: pr, offline: mac.offline, cleanup: deletable, deleteAfter: deleteAfterBinding,
                    resolveTitle: fix.offered ? fix.target.title("Resolve with") : nil,
                    resolving: model.fixing?.conflict == true ? PullRequestText.resolving(fix.target.agent ?? "agent") : nil,
                    resolve: { fixing = FixRequest(prompt: FixPrompt.conflict(base: pr.base), conflict: true) }
                ) {
                    if deleteAfter ?? deletable?.deletesByDefault ?? false { startDelete() }
                }
            }
            if pr.state == "MERGED", let deletable, !model.kept {
                MergedTreeSection(cleanup: deletable, offline: mac.offline) {
                    model.kept = true
                } delete: {
                    startDelete()
                }
            }
            if !pr.threads.isEmpty || pr.state == "OPEN" {
                Section {
                    // A value link, so the screen is in the stack's path and a file opened from it keeps it.
                    NavigationLink(value: Route.reviewThreads(mac: mac.id, tree: model.tree, chat: fix.opener)) {
                        LabeledContent {
                            let open = ThreadGroups.unresolved(pr.threads)
                            if open > 0 { Text("\(open) unresolved").foregroundStyle(Palette.waitingText.color) }
                        } label: {
                            Label("Comments", systemImage: "text.bubble")
                        }
                    }
                }
            }
        }
        .groupedList()
        .refreshable { await store.recheck(model.tree) }
        .errorAlert(model)
        .sendsFixes($fixing, store: store, tree: model.tree, context: fix)
        .confirmationDialog(deletable?.confirmTitle ?? "", isPresented: $confirmingDelete, titleVisibility: .visible) {
            Button("Delete Worktree", role: .destructive) { deleteTree?() }
        } message: {
            Text(deletable?.confirmMessage ?? "")
        }
    }

    private var deletable: WorktreeCleanup? {
        deleteTree == nil ? nil : cleanup
    }

    private var deleteAfterBinding: Binding<Bool> {
        Binding(get: { deleteAfter ?? deletable?.deletesByDefault ?? false }, set: { deleteAfter = $0 })
    }

    private func startDelete() {
        guard let deletable else { return }
        if deletable.asks { confirmingDelete = true } else { deleteTree?() }
    }

    /// What's left to send while the agent isn't already fixing something, as the desktop's callout.
    @ViewBuilder
    private var callout: some View {
        let (checks, threads) = ThreadGroups.unsent(pr, fixing: model.fixing)
        if (model.fixing?.items ?? 0) == 0 && !(checks.isEmpty && threads.isEmpty) {
            FixCallout(checks: checks.count, comments: threads.count, send: fix.offered ? fix.target.title("Send all to") : nil,
                       canDrive: fix.canDrive, mac: mac) {
                fixing = FixRequest(prompt: FixPrompt.fix(pr, checks: checks, threads: threads), threads: threads.map(\.id), checks: checks.map(\.name))
            }
        }
    }

    private var header: some View {
        Section {
            VStack(alignment: .leading, spacing: Space.s) {
                HStack(alignment: .firstTextBaseline, spacing: Space.s) {
                    PrGlyph(state: pr.state, draft: pr.draft)
                    Text(pr.title).font(Typography.headline).foregroundStyle(Palette.text.color)
                }
                Text("#\(pr.number) into \(pr.base)").font(Typography.subheadline).foregroundStyle(Palette.text3.color)
                let (tone, text) = PullRequestText.verdict(pr, busy: model.busy)
                PrPill(text: text, tone: tone, systemImage: model.busy == nil ? nil : "hourglass")
            }
            .padding(.vertical, Space.xs)
        }
    }

    @ViewBuilder
    private var checks: some View {
        let rows = PullRequestText.checkRows(pr)
        Section {
            if pr.runs.isEmpty {
                Text("No checks on this branch").foregroundStyle(Palette.text3.color)
            }
            // Keyed apart from other sections' rows: one List keys every section's rows together.
            ForEach(Self.keyed("named", rows.named), id: \.key) { CheckRow(run: $0.run) }
            let rest = pr.runs.filter { $0.outcome == "passed" || $0.outcome == "skipped" }
            if !rest.isEmpty {
                DisclosureGroup(isExpanded: $passedShown) {
                    ForEach(Self.keyed("rest", rest), id: \.key) { CheckRow(run: $0.run) }
                } label: {
                    Label(PullRequestText.tally([(rows.passed, "passed"), (rows.skipped, "skipped")]), systemImage: "checkmark.circle.fill")
                        .foregroundStyle(Palette.successText.color)
                }
            }
        } header: {
            Text("Checks")
        } footer: {
            if !pr.runs.isEmpty { Text(PullRequestText.checksSummary(pr)) }
        }
    }

    private var review: some View {
        Section("Review") {
            ForEach(PullRequestText.reviews(pr), id: \.1) { line in
                Label {
                    Text(line.1).foregroundStyle(Palette.text.color)
                } icon: {
                    let (image, tone) = Self.mark(line.0)
                    Image(systemName: image).foregroundStyle(tone.text.color)
                }
            }
        }
    }

    nonisolated static func keyed(_ section: String, _ runs: [CheckRun]) -> [(key: String, run: CheckRun)] {
        runs.enumerated().map { ("\(section)-\($0.offset)", $0.element) }
    }

    nonisolated static func mark(_ m: PullRequestText.ReviewMark) -> (String, PrTone) {
        switch m {
        case .approved: ("checkmark.circle.fill", .success)
        case .changesRequested: ("exclamationmark.circle.fill", .failed)
        case .requested: ("person.crop.circle.badge.clock", .waiting)
        case .none: ("circle.dotted", .muted)
        }
    }
}

/// One check: its outcome, its name, and its log a tap away.
@MainActor
struct CheckRow: View {
    let run: CheckRun
    @Environment(\.openURL) private var openURL

    var body: some View {
        let (image, tone) = PullRequestText.outcome(run.outcome)
        Button {
            if let url = run.url.flatMap(URL.init(string:)) { openURL(url) }
        } label: {
            HStack(spacing: Space.m) {
                Image(systemName: image).foregroundStyle(tone.text.color)
                Text(run.name).foregroundStyle(Palette.text.color).lineLimit(1)
                Spacer(minLength: 0)
                if run.url != nil { Image(systemName: "arrow.up.right").font(Typography.caption).foregroundStyle(Palette.text4.color) }
            }
        }
        .disabled(run.url == nil)
        .accessibilityLabel("\(run.name), \(run.outcome)")
    }
}

extension View {
    /// What went wrong acting on the PR, until dismissed.
    @MainActor
    func errorAlert(_ model: TreePullRequest) -> some View {
        alert("Pull request", isPresented: Binding(get: { model.error != nil }, set: { if !$0 { model.error = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(model.error ?? "")
        }
    }

    /// The grouped list of iOS Settings; macOS, which has none, takes its inset list.
    @ViewBuilder
    func groupedList() -> some View {
        #if os(iOS)
            listStyle(.insetGrouped)
        #else
            listStyle(.inset)
        #endif
    }
}
