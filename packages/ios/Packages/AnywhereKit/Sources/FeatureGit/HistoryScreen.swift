import DesignSystem
import DiffModel
import Domain
import Foundation
import Stores
import SwiftUI
import Wire

/// A tree's history by day, fifty commits a page, loading more as the list nears its end.
@MainActor
struct HistoryScreen: View {
    let host: HostModel
    let mac: String
    let tree: String
    @AppStorage("git.history.all") private var all = false

    var body: some View {
        let store = host.git
        let history = store.history(tree)
        let head = store.changes(tree).head
        let now = Int(Date().timeIntervalSince1970)
        ScrollView {
            LazyVStack(spacing: 0) {
                Toggle("All branches", isOn: $all)
                    .toggleStyle(InkToggleStyle())
                    .padding(.horizontal, Inset.margin)
                    .frame(height: HitTarget.minimum)
                    .frame(maxWidth: .infinity)
                    .background(Palette.surface.color)
                    .overlay(alignment: .top) { Palette.separator.color.frame(height: 0.5) }
                    .overlay(alignment: .bottom) { Palette.separator.color.frame(height: 0.5) }
                    .padding(.top, Space.s)
                ForEach(HistoryRows.items(history.commits, now: now)) { item in
                    switch item.kind {
                    case .day(let label):
                        Text(label)
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(Palette.text2.color)
                            .padding(.horizontal, Inset.margin)
                            .padding(.top, Space.xl)
                            .padding(.bottom, Space.s)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .accessibilityAddTraits(.isHeader)
                    case .commit(let commit, let index, let first, let last):
                        PushRow(value: Route.commit(mac: mac, tree: tree, sha: commit.sha)) {
                            HistoryRow(
                                commit: commit, branch: all ? Self.labels(commit.sha, tips: history.tips, head: head).first?.name : nil, now: now
                            )
                            .equatable()
                        }
                        .buttonStyle(.plain)
                        .background(Palette.surface.color)
                        .overlay(alignment: .top) { Palette.separator.color.frame(height: 0.5).padding(.leading, first ? 0 : Inset.margin) }
                        .overlay(alignment: .bottom) { if last { Palette.separator.color.frame(height: 0.5) } }
                        .onAppear {
                            if index >= history.commits.count - 10 { Task { await store.loadHistory(tree, all: all) } }
                        }
                    }
                }
                if history.loading {
                    ProgressView().frame(maxWidth: .infinity).padding(Space.l)
                } else if let error = history.error {
                    ErrorRow(message: error) { history.error = nil }.padding(Space.l)
                } else if history.commits.isEmpty, !history.more {
                    Text("No commits yet").foregroundStyle(Palette.text3.color).padding(Space.l)
                }
            }
            .padding(.bottom, Space.xl)
        }
        .screenBackground()
        .navigationTitle("Commits")
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .principal) { BarHeading("Commits", detail: all ? "All branches" : head?.branch) }
        }
        .refreshable { await store.refreshHistory(tree) }
        .task(id: all) {
            if history.all != all || history.commits.isEmpty { await store.loadHistory(tree, all: all) }
        }
        // The tree's rev moves with a commit made here or anywhere else; a first page with the same tips changes nothing.
        .task(id: head?.rev) {
            if !history.commits.isEmpty { await store.refreshHistory(tree) }
        }
        .onAppear { store.show(tree) }
        .onDisappear { store.hide(tree) }
    }

    /// The tips pointing at a commit, named from the tree's status where it knows them.
    nonisolated static func labels(_ sha: String, tips: LogTips, head: GitHead?) -> [TipLabel] {
        var out: [TipLabel] = []
        if sha == tips.head { out.append(TipLabel(name: tips.branch ?? "HEAD", color: .head)) }
        if sha == tips.upstream { out.append(TipLabel(name: head?.upstream ?? "upstream", color: .upstream)) }
        if sha == tips.base { out.append(TipLabel(name: head?.base ?? "base", color: .base)) }
        return out
    }
}

struct TipLabel: Hashable {
    let name: String
    let color: LaneColor
}

/// One commit: its subject, then its author's avatar and name, when it was made, and its sha.
@MainActor
private struct HistoryRow: View, Equatable {
    let commit: LogCommit
    /// The branch a tip of this commit names, shown when every branch is.
    let branch: String?
    let now: Int

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(commit.subject)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(Palette.text.color)
                .lineLimit(1)
            HStack(spacing: 6) {
                Text(GitText.initials(commit.author))
                    .font(.system(size: 8, weight: .bold))
                    .foregroundStyle(Palette.textBody.color)
                    .frame(width: 18, height: 18)
                    .background(Palette.fill3.color, in: Circle())
                Text(commit.author).foregroundStyle(Palette.textBody.color).lineLimit(1)
                Text("committed \(GitText.committed(at: commit.at, now: now))").lineLimit(1)
                if let branch { Text(branch).font(.system(size: 12, design: .monospaced)).lineLimit(1) }
                Spacer(minLength: Space.s)
                Text(GitText.short(commit.sha))
                    .font(.system(size: 12, design: .monospaced))
                    .foregroundStyle(Palette.textBody.color)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: Radius.chip))
            }
            .font(Typography.footnote)
            .foregroundStyle(Palette.text2.color)
            .monospacedDigit()
        }
        .padding(.vertical, Space.m)
        .padding(.horizontal, Inset.margin)
        .frame(maxWidth: .infinity, alignment: .leading)
        .accessibilityElement(children: .combine)
    }
}

/// A switch in the design's ink: the track fills with ink once on, the knob turning to match.
struct InkToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack {
                configuration.label.font(Typography.body).foregroundStyle(Palette.text.color)
                Spacer()
                Capsule()
                    .fill((configuration.isOn ? Palette.text : Palette.fill4).color)
                    .frame(width: 52, height: 32)
                    .overlay(alignment: configuration.isOn ? .trailing : .leading) {
                        Circle().fill(configuration.isOn ? Palette.onText.color : Color.white).frame(width: 28, height: 28)
                            .shadow(color: .black.opacity(0.25), radius: 2, y: 2).padding(2)
                    }
                    .animation(.snappy(duration: 0.2), value: configuration.isOn)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityValue(configuration.isOn ? "On" : "Off")
    }
}
