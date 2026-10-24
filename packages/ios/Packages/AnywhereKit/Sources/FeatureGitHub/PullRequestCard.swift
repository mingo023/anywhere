import DesignSystem
import Stores
import SwiftUI
import Wire

/// The PR card: where it stands, its checks, its review, what holds the merge, and its comments.
@MainActor
struct PullRequestCard: View {
    let store: GitHubStore
    let model: TreePullRequest
    let pr: PullRequest
    /// Fixes the clock comments are dated against, for the gallery.
    var now: Int?

    @State private var merging = false
    @State private var passedShown = false
    @Environment(\.openURL) private var openURL

    var body: some View {
        List {
            header
            checks
            review
            merge
            if !pr.threads.isEmpty || pr.state == "OPEN" {
                Section {
                    NavigationLink {
                        ThreadsScreen(store: store, model: model, now: now)
                    } label: {
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
        .refreshable { await store.refresh(model.tree) }
        .errorAlert(model)
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

    @ViewBuilder
    private var merge: some View {
        if pr.state == "OPEN" {
            Section("Merge") {
                Label {
                    Text(PullRequestText.gate(pr.gate, base: pr.base)).foregroundStyle(Palette.text.color)
                } icon: {
                    let tone: PrTone = pr.gate == .ready ? .success : pr.gate == .conflicts || pr.gate == .checksFailed ? .failed : .waiting
                    Image(systemName: pr.gate == .ready ? "checkmark.seal.fill" : "exclamationmark.triangle.fill").foregroundStyle(tone.text.color)
                }
                if let line = PullRequestText.mergeable(pr) {
                    Label {
                        Text(line).foregroundStyle(Palette.text2.color)
                    } icon: {
                        let clean = pr.mergeable == "MERGEABLE"
                        Image(systemName: clean ? "checkmark.circle" : "arrow.triangle.branch")
                            .foregroundStyle((clean ? PrTone.success : pr.mergeable == "CONFLICTING" ? .failed : .muted).text.color)
                    }
                }
                if pr.draft {
                    Button {
                        Haptics.tap()
                        Task { await store.markReady(model.tree) }
                    } label: {
                        Text(model.busy ?? "Ready for review").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(model.busy != nil)
                } else {
                    Button {
                        merging = true
                    } label: {
                        Label(model.busy ?? "Merge pull request", systemImage: "arrow.triangle.merge").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(Palette.success.color)
                    .disabled(pr.gate != .ready || model.busy != nil)
                    .confirmationDialog("Merge #\(pr.number) into \(pr.base)?", isPresented: $merging, titleVisibility: .visible) {
                        ForEach(MergeMethod.allCases, id: \.self) { method in
                            Button(method.label) {
                                Task {
                                    await store.merge(model.tree, method: method)
                                    if model.pr?.state == "MERGED" { Haptics.success() } else { Haptics.warning() }
                                }
                            }
                        }
                    } message: {
                        Text("The branch stays.")
                    }
                }
            }
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
