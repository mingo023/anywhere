import ChatModel
import DesignSystem
import Domain
import Foundation
import Stores
import SwiftUI
import Wire

/// The PR screen: its title and state, where to see its changes, its reviews and checks with what holds the merge, and
/// its conversation.
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
    var now = Int(Date().timeIntervalSince1970 * 1000)

    /// Open once the reader has set them; until then the row opens itself when there is something to look at.
    @State private var reviewsOpen: Bool?
    @State private var checksOpen: Bool?
    @State private var passedShown = false
    @State private var fixing: FixRequest?
    /// Delete after merging, once the reader has set it; until then the cleanup's default.
    @State private var deleteAfter: Bool?
    @State private var confirmingDelete = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                header
                sectionTitle("Changes")
                changes
                sectionTitle("Status")
                status
                let entries = PullRequestText.conversation(pr)
                if !entries.isEmpty {
                    sectionTitle("Conversation")
                    ForEach(entries) { ConversationRow(entry: $0, now: now) }
                }
            }
            .padding(.bottom, Space.xl)
        }
        .screenBackground()
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

    private func sectionTitle(_ title: String) -> some View {
        Text(title).font(.system(size: 20, weight: .bold)).foregroundStyle(Palette.text.color)
            .padding(.horizontal, Inset.margin).padding(.top, Space.xl).padding(.bottom, Space.s)
            .accessibilityAddTraits(.isHeader)
    }

    /// Full-width rows between hairlines, as the design sets Changes and Status.
    private func group(@ViewBuilder _ rows: () -> some View) -> some View {
        VStack(spacing: 0, content: rows)
            .frame(maxWidth: .infinity)
            .background(Palette.surface.color)
            .overlay(alignment: .top) { Palette.separator.color.frame(height: 0.5) }
            .overlay(alignment: .bottom) { Palette.separator.color.frame(height: 0.5) }
    }

    // MARK: Header

    private var header: some View {
        let repo = PullRequestText.repoName(pr.url) ?? ""
        return VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 6) {
                Text(PullRequestText.initials(repo))
                    .font(.system(size: 8, weight: .bold)).foregroundStyle(Palette.textBody.color)
                    .frame(width: 18, height: 18).background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: 5, style: .continuous))
                (Text(repo) + Text(" #\(pr.number)").font(.system(size: 13, design: .monospaced)))
                    .font(.system(size: 13)).foregroundStyle(Palette.text2.color)
            }
            Text(pr.title).font(.system(size: 24, weight: .bold)).foregroundStyle(Palette.text.color).padding(.top, 6)
            PrStateBadge(state: pr.state, draft: pr.draft).padding(.top, 10)
            sentence.font(.system(size: 13)).lineSpacing(2).foregroundStyle(Palette.text2.color).padding(.top, 8)
        }
        .padding(.horizontal, Inset.margin)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }

    private var sentence: Text {
        PullRequestText.branchSentence(state: pr.state, base: pr.base, head: newPr.branch, commits: commits).reduce(Text("")) { text, piece in
            text + (piece.code ? Text(piece.text).font(.system(size: 13, design: .monospaced)).foregroundStyle(Palette.textBody.color) : Text(piece.text))
        }
    }

    /// The branch's commits ahead, which are the PR's only while it is open into the base they were counted from.
    private var commits: Int? {
        guard pr.state == "OPEN", newPr.branch != nil, let base = newPr.base,
            PullRequestText.baseBranch(base) == PullRequestText.baseBranch(pr.base)
        else { return nil }
        return newPr.ahead
    }

    // MARK: Changes

    private var changes: some View {
        let open = ThreadGroups.unresolved(pr.threads)
        return group {
            PushRow(value: Route.changes(mac: mac.id, tree: model.tree, chat: fix.opener)) {
                PrListRow(icon: .fileDiff, title: "Files changed")
            }
            if let commits {
                PushRow(value: Route.history(mac: mac.id, tree: model.tree)) {
                    PrListRow(icon: .commit, title: PullRequestText.commits(commits)).rowDivider(true, leading: PrListRow.textColumn)
                }
            }
            // A value link, so the screen is in the stack's path and a file opened from it keeps it.
            PushRow(value: Route.reviewThreads(mac: mac.id, tree: model.tree, chat: fix.opener)) {
                PrListRow(icon: .comment, title: "Review threads", detail: pr.threads.isEmpty ? nil : open == 0 ? "All resolved" : "\(open) unresolved")
                    .rowDivider(true, leading: PrListRow.textColumn)
            }
        }
        .buttonStyle(.plain)
    }

    // MARK: Status

    private var status: some View {
        let review = PullRequestText.reviewHeadline(pr)
        let reviewers = PullRequestText.reviewers(pr)
        let checks = PullRequestText.checksHeadline(pr)
        let rows = PullRequestText.checkRows(pr)
        let reviewsShown = reviewsOpen ?? (review.mark == .requested || review.mark == .changesRequested)
        let checksShown = checksOpen ?? (checks.map { $0.tone != .success } ?? false)
        return group {
            StatusRow(title: "Reviews", detail: review.text, open: reviewers.isEmpty ? nil : reviewsShown) {
                reviewMark(review.mark)
            } toggle: {
                reviewsOpen = !reviewsShown
            }
            if reviewsShown { ForEach(reviewers) { ReviewerRow(reviewer: $0) } }
            if let checks {
                StatusRow(title: "Checks", detail: checks.text, open: checksShown) {
                    checkMark(checks.tone)
                } toggle: {
                    checksOpen = !checksShown
                }
                .rowDivider(true, leading: PrListRow.textColumn)
                if checksShown { checkLines(rows) }
            }
            StatusFooter(
                store: store, model: model, pr: pr, mac: mac, cleanup: deletable, deleteAfter: deleteAfterBinding, fix: fix,
                sendAll: sendAll, resolve: resolve, keep: { model.kept = true }, delete: startDelete,
                merged: { if deleteAfter ?? deletable?.deletesByDefault ?? false { startDelete() } }
            )
            .rowDivider(true, leading: 0)
        }
        .buttonStyle(.plain)
    }

    @ViewBuilder
    private func reviewMark(_ mark: PullRequestText.ReviewMark) -> some View {
        switch mark {
        case .approved: LucideIcon(.circleCheck, size: 20).foregroundStyle(Palette.success.color)
        case .changesRequested: LucideIcon(.circleAlert, size: 20).foregroundStyle(Palette.failed.color)
        case .requested, .none: Circle().strokeBorder(Palette.text3.color, lineWidth: 1.5).frame(width: 14, height: 14)
        }
    }

    @ViewBuilder
    private func checkMark(_ tone: PrTone) -> some View {
        switch tone {
        case .failed: LucideIcon(.circleX, size: 20).foregroundStyle(Palette.failed.color)
        case .waiting: RingSpinner(size: 18)
        default: LucideIcon(.circleCheck, size: 20).foregroundStyle(Palette.success.color)
        }
    }

    /// The failing and running checks by name, then the passed and skipped ones behind their count.
    @ViewBuilder
    private func checkLines(_ rows: PullRequestText.CheckRows) -> some View {
        let sent = model.fixing?.checks ?? []
        // Keyed apart from other sections' rows: the keys must not repeat across the screen.
        ForEach(Self.keyed("named", rows.named), id: \.key) { line in
            CheckLine(run: line.run, send: sendable(line.run, sent: sent))
        }
        let rest = pr.runs.filter { $0.outcome == "passed" || $0.outcome == "skipped" }
        if !rest.isEmpty {
            Button { passedShown.toggle() } label: {
                HStack(spacing: Space.s + 2) {
                    LucideIcon(.circleCheck, size: 18).foregroundStyle(Palette.success.color)
                    Text(rows.passed + rows.skipped == 1 ? "1 successful check" : "\(rows.passed + rows.skipped) successful checks")
                        .font(.system(size: 15)).foregroundStyle(Palette.text2.color)
                    Spacer(minLength: 0)
                    LucideIcon(passedShown ? .chevronDown : .chevronRight, size: 14).foregroundStyle(Palette.text3.color)
                }
                .frame(minHeight: 44)
                .padding(.leading, PrListRow.textColumn)
                .padding(.trailing, 12)
                .contentShape(Rectangle())
            }
            if passedShown { ForEach(Self.keyed("rest", rest), id: \.key) { CheckRow(run: $0.run) } }
        }
    }

    /// Send for one failed check, offered while the agent isn't already on it.
    private func sendable(_ run: CheckRun, sent: Set<String>) -> (() -> Void)? {
        guard run.outcome == "failed", fix.offered, !sent.contains(run.name) else { return nil }
        return { fixing = FixRequest(prompt: FixPrompt.fix(pr, checks: [run], threads: []), checks: [run.name]) }
    }

    /// What's left to send while the agent isn't already fixing something, as the desktop's callout.
    private var sendAll: StatusFooter.SendAll? {
        let (checks, threads) = ThreadGroups.unsent(pr, fixing: model.fixing)
        guard pr.state == "OPEN", (model.fixing?.items ?? 0) == 0, !(checks.isEmpty && threads.isEmpty) else { return nil }
        let send = fix.offered ? { fixing = FixRequest(prompt: FixPrompt.fix(pr, checks: checks, threads: threads), threads: threads.map(\.id), checks: checks.map(\.name)) } : nil
        return StatusFooter.SendAll(title: fix.target.title("Send all to"), send: send)
    }

    private var resolve: StatusFooter.Resolve? {
        guard pr.gate == .conflicts, fix.offered else { return nil }
        let working = model.fixing?.conflict == true
        return StatusFooter.Resolve(
            title: fix.target.title("Resolve with"), working: working ? PullRequestText.resolving(fix.target.agent ?? "agent") : nil,
            run: { fixing = FixRequest(prompt: FixPrompt.conflict(base: pr.base), conflict: true) })
    }

    nonisolated static func keyed(_ section: String, _ runs: [CheckRun]) -> [(key: String, run: CheckRun)] {
        runs.enumerated().map { ("\(section)-\($0.offset)", $0.element) }
    }
}

/// A row of a group: a 20 pt icon, a title, and what's to see there.
@MainActor
struct PrListRow: View {
    /// Where a row's text starts, and so its hairline: the icon's 20 pt, 12 pt of gap and the 16 pt inset.
    static let textColumn: CGFloat = 48

    let icon: Lucide
    let title: String
    var detail: String?

    var body: some View {
        HStack(spacing: Space.m) {
            LucideIcon(icon, size: 20).foregroundStyle(Palette.text2.color).frame(width: 20)
            Text(title).font(.system(size: 17)).foregroundStyle(Palette.text.color).lineLimit(1)
            Spacer(minLength: Space.s)
            if let detail { Text(detail).font(.system(size: 15)).foregroundStyle(Palette.text2.color).lineLimit(1) }
            LucideIcon(.chevronRight, size: 14).foregroundStyle(Palette.text3.color)
        }
        .frame(minHeight: 44)
        .padding(.leading, Inset.margin)
        .padding(.trailing, Space.m)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
    }
}

/// Reviews or Checks: its verdict in its colour, with a chevron that opens what it is made of.
@MainActor
private struct StatusRow<Mark: View>: View {
    let title: String
    let detail: String
    /// Nil when there is nothing to open.
    let open: Bool?
    @ViewBuilder let mark: Mark
    let toggle: () -> Void

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: Space.m) {
                mark.frame(width: 20, height: 20)
                (Text(title) + Text(" · \(detail)").foregroundStyle(Palette.text2.color))
                    .font(.system(size: 17)).foregroundStyle(Palette.text.color).lineLimit(1)
                Spacer(minLength: Space.s)
                if let open { LucideIcon(open ? .chevronUp : .chevronDown, size: 14).foregroundStyle(Palette.text3.color) }
            }
            .frame(minHeight: 44)
            .padding(.leading, Inset.margin)
            .padding(.trailing, Space.m)
            .frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .disabled(open == nil)
        .accessibilityLabel("\(title), \(detail)")
    }
}

/// Who reviewed or is asked, under the Reviews row.
@MainActor
private struct ReviewerRow: View {
    let reviewer: Reviewer

    var body: some View {
        HStack(spacing: Space.s + 2) {
            Circle().fill(dot.color).frame(width: 7, height: 7)
            Text(PullRequestText.initials(reviewer.login))
                .font(.system(size: 10, weight: .semibold)).foregroundStyle(Palette.textBody.color)
                .frame(width: 24, height: 24).background(Palette.fill3.color, in: Circle())
            Text(reviewer.login).font(.system(size: 15)).foregroundStyle(Palette.text.color).lineLimit(1)
            Spacer(minLength: Space.s)
            Text(reviewer.state.label).font(.system(size: 13)).foregroundStyle(Palette.text2.color)
        }
        .frame(minHeight: 44)
        .padding(.leading, PrListRow.textColumn)
        .padding(.trailing, Inset.margin)
        .accessibilityElement(children: .combine)
    }

    private var dot: Token {
        switch reviewer.state {
        case .approved: Palette.success
        case .changesRequested: Palette.failed
        case .commented: Palette.text3
        case .pending: Palette.waitingDot
        }
    }
}

/// A failing or running check: its name, and Send to hand it to the agent.
@MainActor
private struct CheckLine: View {
    let run: CheckRun
    let send: (() -> Void)?
    @Environment(\.openURL) private var openURL

    var body: some View {
        HStack(spacing: Space.s + 2) {
            Button {
                if let url = run.url.flatMap(URL.init(string:)) { openURL(url) }
            } label: {
                HStack(spacing: Space.s + 2) {
                    CheckRow.mark(run.outcome)
                    VStack(alignment: .leading, spacing: 0) {
                        Text(run.name).font(.system(size: 15, design: .monospaced)).foregroundStyle(Palette.text.color).lineLimit(1)
                        Text(run.outcome == "pending" ? "Running" : "Failed").font(.system(size: 13)).foregroundStyle(Palette.text2.color)
                    }
                    Spacer(minLength: 0)
                }
                .contentShape(Rectangle())
            }
            .disabled(run.url == nil)
            if let send {
                Button("Send") {
                    Haptics.tap()
                    send()
                }
                .buttonStyle(.plain)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(Palette.text.color)
                .padding(.horizontal, 14)
                .frame(height: 30)
                .background(Palette.fill3.color, in: Capsule())
            }
        }
        .padding(.vertical, Space.s)
        .padding(.leading, PrListRow.textColumn)
        .padding(.trailing, Inset.margin)
        .accessibilityElement(children: .contain)
    }
}

/// A passed or skipped check, its log a tap away.
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

    /// A tick, a cross or the running ring, as the desktop marks checks.
    @ViewBuilder
    static func mark(_ outcome: String) -> some View {
        switch status(outcome) {
        case .done: LucideIcon(.circleCheck, size: 18).foregroundStyle(Palette.success.color)
        case .failed: LucideIcon(.circleX, size: 18).foregroundStyle(Palette.failed.color)
        case .working: RingSpinner(size: 18)
        default: LucideIcon(.runSkipped, size: 18).foregroundStyle(Palette.text3.color)
        }
    }

    var body: some View {
        Button {
            if let url = run.url.flatMap(URL.init(string:)) { openURL(url) }
        } label: {
            HStack(spacing: Space.s + 2) {
                Self.mark(run.outcome)
                Text(run.name).font(.system(size: 15, design: .monospaced)).foregroundStyle(Palette.text.color).lineLimit(1)
                Spacer(minLength: 0)
            }
            .frame(minHeight: 36)
            .padding(.leading, PrListRow.textColumn)
            .padding(.trailing, Inset.margin)
            .contentShape(Rectangle())
        }
        .disabled(run.url == nil)
        .accessibilityLabel("\(run.name), \(run.outcome)")
    }
}

/// A line of the conversation: a verdict or a comment, who said it and when it can be told.
@MainActor
private struct ConversationRow: View {
    let entry: ConversationEntry
    let now: Int

    var body: some View {
        HStack(alignment: .top, spacing: Space.m) {
            icon.frame(width: 24, height: 24).background(Palette.fill3.color, in: Circle())
            (Text(entry.who).fontWeight(.semibold).foregroundStyle(Palette.text.color) + Text(" \(entry.what)")
                + Text(time).foregroundStyle(Palette.text3.color))
                .font(.system(size: 15)).foregroundStyle(Palette.text2.color).lineSpacing(3)
                .frame(maxWidth: .infinity, alignment: .leading).padding(.top, 1)
        }
        .padding(.vertical, 9)
        .padding(.horizontal, Inset.margin)
        .accessibilityElement(children: .combine)
    }

    private var time: String {
        entry.at.map { " " + TimeText.ago(ms: $0 * 1000, now: now) } ?? ""
    }

    @ViewBuilder private var icon: some View {
        switch entry.kind {
        case .approved: LucideIcon(.check, size: 13).foregroundStyle(Palette.success.color)
        case .changesRequested: LucideIcon(.circleAlert, size: 13).foregroundStyle(Palette.failed.color)
        case .comment: LucideIcon(.comment, size: 13).foregroundStyle(Palette.text2.color)
        }
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
