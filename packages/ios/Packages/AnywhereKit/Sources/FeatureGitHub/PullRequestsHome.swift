import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// The pull requests of every paired Mac's projects, grouped by what they want: the PRs tab's root.
@MainActor
public struct PullRequestsHome: View {
    let hosts: [HostModel]
    let open: ((Route) -> Void)?
    let load: Bool

    @State private var loaded = false
    /// The last load reached no Mac, so an empty list says nothing about what's open.
    @State private var unanswered = false
    @State private var merging: PrEntry?
    @State private var busy: Set<String> = []
    @State private var failure: String?

    /// `open` shows a PR where Fix takes it; `load` false keeps the screen to what the stores hold, as the gallery does.
    public init(hosts: [HostModel], open: ((Route) -> Void)? = nil, load: Bool = true) {
        self.hosts = hosts
        self.open = open
        self.load = load
    }

    public var body: some View {
        let entries = Self.entries(hosts)
        let macs = Self.problems(hosts)
        let notices = PrProblems.notices(macs)
        let groups = PrSections.group(entries)
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                ScreenTitle("Pull requests")
                ForEach(notices) { GhNoticeRow(notice: $0) }
                if PrProblems.unreachable(macs) {
                    NetStrip(.offline, "Couldn't reach GitHub · showing the last answer", action: ("Retry", { Task { await loadAll() } }))
                        .padding(.horizontal, Inset.margin)
                        .padding(.top, Inset.margin)
                }
                if groups.isEmpty && notices.isEmpty && (loaded || !load) {
                    if unanswered {
                        Placeholder("Couldn't load pull requests", icon: .cloudOff, detail: "No Mac answered. Try again once one is online.") {
                            Task { await loadAll() }
                        }
                    } else {
                        NoPullRequests()
                    }
                }
                ForEach(Array(groups.enumerated()), id: \.element.id) { index, group in
                    CountedHeader(group.section.title, count: group.entries.count, first: index == 0)
                    LazyVStack(spacing: 0) {
                        ForEach(Array(group.entries.enumerated()), id: \.element.id) { row, entry in
                            self.row(entry, divided: row > 0)
                        }
                    }
                    .insetGroup()
                }
            }
            .padding(.bottom, Space.xl)
        }
        .screenBackground()
        .refreshable { await loadAll() }
        .task(id: loadKey) { if load { await loadAll() } }
        .confirmationDialog(merging.map { "Merge #\($0.item.number)?" } ?? "", isPresented: mergingBinding, titleVisibility: .visible, presenting: merging) { entry in
            ForEach(MergeMethod.allCases, id: \.self) { method in
                Button(method.label) { merge(entry, method: method) }
            }
        } message: { entry in
            Text(entry.item.title)
        }
        .alert("Pull request", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
    }

    private func row(_ entry: PrEntry, divided: Bool) -> some View {
        let action = PrRowAction.of(entry.item)
        let online = host(entry)?.link == .online
        return PushRow(value: entry.route) {
            PrRow(entry: entry, action: action, enabled: online && !busy.contains(entry.id)) { act($0, on: entry) }
                .rowDivider(divided, leading: PrRow.textColumn)
        }
        .buttonStyle(.plain)
        .contextMenu {
            if let open { Button { open(entry.route) } label: { Label("Open", lucide: .pullRequest) } }
            switch action {
            case .fix?:
                if let open { Button { open(entry.route) } label: { Label("Fix", lucide: .sparkle) } }
            case .merge?:
                Button { merging = entry } label: { Label("Merge…", lucide: .merge) }.disabled(!online || busy.contains(entry.id))
            case nil:
                EmptyView()
            }
        }
    }

    private func act(_ action: PrRowAction, on entry: PrEntry) {
        switch action {
        case .fix: open?(entry.route)
        case .merge: merging = entry
        }
    }

    private var mergingBinding: Binding<Bool> {
        Binding(get: { merging != nil }, set: { if !$0 { merging = nil } })
    }

    private func host(_ entry: PrEntry) -> HostModel? {
        hosts.first { $0.mac == entry.mac }
    }

    /// The merge needs the whole PR, which the list doesn't carry: ask for it first.
    private func merge(_ entry: PrEntry, method: MergeMethod) {
        guard let store = host(entry)?.github else { return }
        let tree = entry.item.tree
        busy.insert(entry.id)
        Task {
            defer { busy.remove(entry.id) }
            await store.refresh(tree)
            let model = store.pullRequest(tree)
            // The tree may have moved to another PR since the list loaded: merge only the one the reader confirmed.
            guard model.pr?.number == entry.item.number, model.error == nil else {
                Haptics.warning()
                failure = "#\(entry.item.number) is no longer this branch's pull request. Pull to refresh."
                model.error = nil
                return
            }
            await store.merge(tree, method: method)
            if model.pr?.state == "MERGED" {
                Haptics.success()
            } else {
                Haptics.warning()
                failure = model.error ?? "Couldn't merge #\(entry.item.number)."
                model.error = nil
            }
            try? await store.refreshList(entry.projectPath)
        }
    }

    /// Changes when a Mac connects or its projects arrive, so the lists load then and not only on first appearance.
    private var loadKey: [String] {
        hosts.map { "\($0.id)|\($0.link == .online)|\($0.workspace.projects.map(\.path).joined(separator: ","))" }
    }

    private func loadAll() async {
        let targets = hosts.filter { $0.link == .online }.flatMap { host in host.workspace.projects.map { (host.github, $0.path) } }
        let loads = targets.map { store, path in Task { (try? await store.recheckList(path)) != nil } }
        var answered = false
        for load in loads where await load.value { answered = true }
        let emptyWorkspace = hosts.contains { $0.link == .online && $0.workspace.loaded && $0.workspace.projects.isEmpty }
        unanswered = !hosts.isEmpty && !answered && !emptyWorkspace
        loaded = true
    }
}

extension PullRequestsHome {
    /// How many pull requests across `hosts` need action, for the PRs tab's badge.
    @MainActor
    public static func needingAction(_ hosts: [HostModel]) -> Int {
        PrSections.needingAction(entries(hosts).map(\.item))
    }

    @MainActor
    static func entries(_ hosts: [HostModel]) -> [PrEntry] {
        hosts.flatMap { host in
            host.workspace.projects.flatMap { project in
                (host.github.lists[project.path] ?? []).map {
                    PrEntry(mac: host.mac, macName: host.name, projectPath: project.path, repo: project.name, item: $0)
                }
            }
        }
    }

    @MainActor
    static func problems(_ hosts: [HostModel]) -> [MacPrProblems] {
        hosts.map { host in
            MacPrProblems(id: host.mac, name: host.name, problems: host.workspace.projects.compactMap { host.github.listProblems[$0.path] })
        }
    }
}

/// gh isn't installed or signed in on a Mac, so none of its PRs show.
@MainActor
private struct GhNoticeRow: View {
    let notice: GhNotice

    var body: some View {
        HStack(alignment: .top, spacing: Space.m) {
            LucideIcon(.triangleAlert, size: 16).foregroundStyle(Palette.waitingDot.color).padding(.top, 2)
            VStack(alignment: .leading, spacing: 2) {
                Text(notice.title).font(.system(size: 15, weight: .semibold)).foregroundStyle(Palette.text.color)
                Text(notice.detailText).font(.system(size: 13)).foregroundStyle(Palette.text2.color)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Inset.margin)
        .padding(.vertical, 13)
        .insetGroup()
        .padding(.top, Inset.margin)
        .accessibilityElement(children: .combine)
    }
}

extension GhNotice {
    /// The detail with the command it names set in mono.
    @MainActor
    var detailText: AttributedString {
        var text = AttributedString(detail)
        for command in ["brew install gh", "gh auth login"] {
            if let range = text.range(of: command) { text[range].font = .system(size: 12, design: .monospaced) }
        }
        return text
    }
}

/// No PR on any Mac.
@MainActor
private struct NoPullRequests: View {
    var body: some View {
        VStack(spacing: 10) {
            LucideIcon(.pullRequest, size: 28).foregroundStyle(Palette.text4.color)
            Text("No open pull requests").font(.system(size: 17, weight: .semibold)).foregroundStyle(Palette.text.color)
            Text("Pull requests from your sessions show up here.").font(.system(size: 15)).foregroundStyle(Palette.text2.color)
                .multilineTextAlignment(.center)
        }
        .padding(.horizontal, 40)
        .padding(.top, 220)
        .frame(maxWidth: .infinity)
    }
}

/// One PR in a group: its state, repo and number, its title, and its checks, with at most one action.
@MainActor
struct PrRow: View {
    /// Where a row's text starts, and so its divider: the icon's 16 pt, 12 pt of gap and the 16 pt inset.
    static let textColumn: CGFloat = 44

    let entry: PrEntry
    var action: PrRowAction?
    var enabled = true
    let act: (PrRowAction) -> Void

    var body: some View {
        let item = entry.item
        let merged = item.state == "MERGED"
        let icon = PrSection.icon(item)
        HStack(alignment: .top, spacing: Space.m) {
            LucideIcon(icon.icon, size: 16).foregroundStyle(icon.tone.icon.color).padding(.top, 14)
            VStack(alignment: .leading, spacing: 0) {
                (Text(entry.repo) + Text(" #\(item.number)").font(.system(size: 12, design: .monospaced)))
                    .font(.system(size: 13)).foregroundStyle(Palette.text2.color).lineLimit(1)
                Text(item.title)
                    .font(.system(size: 16, weight: .semibold)).foregroundStyle((merged ? Palette.text2 : Palette.text).color)
                    .lineLimit(2).padding(.top, 2)
                HStack(spacing: Space.s) {
                    PrMetaLine(meta: PrMeta.of(item), muted: merged)
                    if let action {
                        Button(action.title) { act(action) }.buttonStyle(RowActionStyle()).disabled(!enabled)
                    }
                }
                .frame(minHeight: action == nil ? 18 : 28)
                .padding(.top, 6)
            }
            .padding(.vertical, 13)
            .padding(.trailing, Inset.margin)
        }
        .padding(.leading, Inset.margin)
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// "2 failed · Changes requested": the checks in their colour, the rest plain.
@MainActor
struct PrMetaLine: View {
    let meta: PrMeta
    /// A merged PR's line is quiet throughout.
    var muted = false

    var body: some View {
        HStack(spacing: Space.xs) {
            if let checks = meta.checks { checksPart(checks) }
            if meta.checks != nil && meta.detail != nil { separator }
            if let detail = meta.detail { Text(detail).foregroundStyle((muted ? Palette.text3 : Palette.text2).color).lineLimit(1) }
            if meta.isEmpty { Text("No checks").foregroundStyle(Palette.text3.color) }
        }
        .font(.system(size: 13))
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var separator: some View {
        Text("·").foregroundStyle(Palette.text3.color).padding(.horizontal, 5)
    }

    @ViewBuilder
    private func checksPart(_ checks: PrMeta.Checks) -> some View {
        let ink: Token = muted ? Palette.text3 : checks.ink
        HStack(spacing: Space.xs) {
            if checks == .running {
                RingSpinner(size: 13)
            } else {
                LucideIcon(checks.icon, size: 13).foregroundStyle((muted ? Palette.text3 : checks.iconInk).color)
            }
            Text(checks.text).fontWeight(checks == .running || muted ? .regular : .medium).foregroundStyle(ink.color)
        }
        .lineLimit(1)
        .fixedSize()
    }
}

extension PrMeta.Checks {
    var icon: Lucide { self == .passed ? .circleCheck : .circleX }
    var ink: Token {
        switch self {
        case .failed: Palette.failedText
        case .passed: Palette.successText
        case .running: Palette.text2
        }
    }
    var iconInk: Token {
        switch self {
        case .failed: Palette.failed
        case .passed: Palette.success
        case .running: Palette.text2
        }
    }
}
