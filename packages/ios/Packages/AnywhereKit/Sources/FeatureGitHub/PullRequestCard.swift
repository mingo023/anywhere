import ChatModel
import DesignSystem
import Domain
import Foundation
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
    var newPr = NewPr()

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
                )
            }
            if pr.state == "MERGED", let deletable, !model.kept {
                MergedTreeSection(cleanup: deletable, offline: mac.offline) {
                    model.kept = true
                } delete: {
                    startDelete()
                }
            }
            if !pr.threads.isEmpty || pr.state == "OPEN" { threads }
        }
        .scrollContentBackground(.hidden)
        .screenBackground()
        .groupedList()
        .refreshable { await store.recheck(model.tree) }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if pr.state == "OPEN", !pr.draft, pr.gate != .conflicts || !fix.offered {
                MergeBar(store: store, model: model, pr: pr, offline: mac.offline, cleanup: deletable, deleteAfter: deleteAfterBinding) {
                    if deleteAfter ?? deletable?.deletesByDefault ?? false { startDelete() }
                }
            }
        }
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
                HStack(spacing: Space.s) {
                    let (tone, text) = PullRequestText.verdict(pr, busy: model.busy)
                    PrPill(text: text, tone: tone, systemImage: model.busy == nil ? nil : "hourglass", dot: model.busy == nil)
                    Text("#\(pr.number)").font(Typography.subheadline).foregroundStyle(Palette.text3.color)
                }
                Text(pr.title).font(Typography.title).foregroundStyle(Palette.text.color)
                Text(line).font(Typography.codeSmall).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.middle)
            }
            .padding(.vertical, Space.xs)
        }
        .listRowBackground(Color.clear)
        .listRowInsets(EdgeInsets(top: 0, leading: Space.xs, bottom: 0, trailing: Space.xs))
    }

    /// The branch into its base; `pr.head` is only a commit, so the tree's own branch names it.
    private var line: String {
        newPr.branch.map { PullRequestText.headLine(branch: $0, base: pr.base, ahead: newPr.ahead) } ?? "into \(pr.base)"
    }

    private var threads: some View {
        let open = ThreadGroups.unresolved(pr.threads)
        return Section {
            // A value link, so the screen is in the stack's path and a file opened from it keeps it.
            NavigationLink(value: Route.reviewThreads(mac: mac.id, tree: model.tree, chat: fix.opener)) {
                if let thread = pr.threads.first(where: { !$0.resolved }), let first = thread.comments.first {
                    ThreadSummary(thread: thread, first: first)
                } else {
                    Text("No unresolved comments").foregroundStyle(Palette.text3.color)
                }
            }
        } header: {
            HStack {
                Text("Review threads").sectionTitle()
                Spacer()
                Text("\(open) unresolved · \(pr.threads.count - open) resolved").font(Typography.footnote).foregroundStyle(Palette.text3.color).textCase(nil)
            }
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
            HStack {
                Text("Checks").sectionTitle()
                Spacer()
                if !pr.runs.isEmpty { Text(PullRequestText.checksSummary(pr)).font(Typography.footnote).foregroundStyle(Palette.text3.color).textCase(nil) }
            }
        }
    }

    private var review: some View {
        Section {
            ForEach(PullRequestText.reviews(pr), id: \.1) { line in
                Label {
                    Text(line.1).foregroundStyle(Palette.text.color)
                } icon: {
                    let (image, tone) = Self.mark(line.0)
                    Image(systemName: image).foregroundStyle(tone.text.color)
                }
            }
        } header: {
            Text("Review").sectionTitle()
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

/// An open review thread: where it's anchored, who said what, and how much more there is.
@MainActor
private struct ThreadSummary: View {
    let thread: ReviewThread
    let first: ThreadComment

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            Text(thread.line.map { "\(thread.path):\($0)" } ?? thread.path).font(Typography.caption).fontDesign(.monospaced).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.head)
            HStack(spacing: Space.xs) {
                Text(first.author).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color)
                Text("· \(TimeText.ago(ms: first.at * 1000, now: Int(Date().timeIntervalSince1970 * 1000)))").font(Typography.subheadline).foregroundStyle(Palette.text3.color)
            }
            Text(first.body).font(Typography.subheadline).foregroundStyle(Palette.textBody.color).lineLimit(3)
            HStack(spacing: Space.xs) {
                if thread.comments.count > 1 { Text(replies).foregroundStyle(Palette.text3.color) }
                if thread.comments.count > 1 { Text("·").foregroundStyle(Palette.text4.color) }
                Text("Reply").fontWeight(.semibold).foregroundStyle(Palette.text.color)
            }
            .font(Typography.footnote)
        }
        .padding(.vertical, Space.xxs)
    }

    private var replies: String {
        let n = thread.comments.count - 1
        return n == 1 ? "1 reply" : "\(n) replies"
    }
}

/// One check: its outcome, its name, and its log a tap away.
@MainActor
struct CheckRow: View {
    let run: CheckRun
    @Environment(\.openURL) private var openURL

    nonisolated static func status(_ outcome: String) -> Status {
        switch outcome {
        case "failed": .failed
        case "pending": .working
        case "skipped": .idle
        default: .done
        }
    }

    var body: some View {
        Button {
            if let url = run.url.flatMap(URL.init(string:)) { openURL(url) }
        } label: {
            HStack(spacing: Space.m) {
                StatusGlyph(Self.status(run.outcome))
                Text(run.name).font(Typography.code).foregroundStyle(Palette.text.color).lineLimit(1)
                Spacer(minLength: 0)
                if run.outcome == "pending" { Text("Running").font(Typography.footnote).foregroundStyle(Palette.text3.color) }
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
