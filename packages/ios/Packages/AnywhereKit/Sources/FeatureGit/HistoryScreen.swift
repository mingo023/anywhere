import DesignSystem
import DiffModel
import Domain
import Foundation
import Stores
import SwiftUI
import Wire

/// A tree's history with its graph, fifty commits a page, loading more as the list nears its end.
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
        let lanes = min(history.layout.width, GraphCell.maxLanes)
        let now = Int(Date().timeIntervalSince1970)
        List {
            ForEach(Array(zip(history.commits.indices, history.commits)), id: \.1.sha) { i, commit in
                NavigationLink(value: Route.commit(mac: mac, tree: tree, sha: commit.sha)) {
                    HistoryRow(
                        commit: commit, edges: GraphEdges(history.layout.rows[i], sha: commit.sha, parents: commit.parents.count),
                        column: history.layout.rows[i].column, color: history.layout.rows[i].color, lanes: lanes,
                        labels: Self.labels(commit.sha, tips: history.tips, head: head), now: now)
                }
                .listRowInsets(EdgeInsets(top: 0, leading: Space.m, bottom: 0, trailing: Space.m))
                .onAppear {
                    if i >= history.commits.count - 10 { Task { await store.loadHistory(tree, all: all) } }
                }
            }
            if history.loading {
                HStack {
                    Spacer()
                    ProgressView()
                    Spacer()
                }
            } else if let error = history.error {
                ErrorRow(message: error) { history.error = nil }
            } else if history.commits.isEmpty, !history.more {
                Text("No commits yet").foregroundStyle(Palette.text3.color)
            }
        }
        .listStyle(.plain)
        .navigationTitle("History")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Toggle("All Branches", systemImage: "point.3.connected.trianglepath.dotted", isOn: $all)
            }
        }
        .task(id: all) {
            if history.all != all || history.commits.isEmpty { await store.loadHistory(tree, all: all) }
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

/// One commit: its graph cell, subject, tips, author and age.
@MainActor
private struct HistoryRow: View, Equatable {
    let commit: LogCommit
    let edges: GraphEdges
    let column: Int
    let color: LaneColor
    let lanes: Int
    let labels: [TipLabel]
    let now: Int

    var body: some View {
        HStack(spacing: Space.s) {
            GraphCell(edges: edges, column: column, color: color, lanes: lanes, head: labels.contains { $0.color == .head })
                .frame(height: 56)
            VStack(alignment: .leading, spacing: 3) {
                HStack(spacing: Space.xs) {
                    ForEach(labels, id: \.self) { label in
                        Text(label.name)
                            .font(Typography.captionStrong)
                            .lineLimit(1)
                            .foregroundStyle(GraphCell.token(label.color).color)
                            .padding(.horizontal, 6)
                            .padding(.vertical, 1)
                            .overlay(Capsule().strokeBorder(GraphCell.token(label.color).color.opacity(0.6)))
                    }
                    Text(commit.subject).font(Typography.subheadline).foregroundStyle(Palette.text.color).lineLimit(1)
                }
                HStack(spacing: Space.xs) {
                    Text(GitText.short(commit.sha)).font(Typography.codeSmall)
                    Text("·")
                    Text(commit.author).lineLimit(1)
                    Text("·")
                    Text(GitText.age(commit.at, now: now)).monospacedDigit()
                }
                .font(Typography.caption)
                .foregroundStyle(Palette.text3.color)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// The lanes through one commit's row, drawn with Canvas: lanes passing by curve to their new column, lanes waiting
/// for the commit end in its dot, and lanes to its parents leave it.
@MainActor
struct GraphCell: View {
    let edges: GraphEdges
    let column: Int
    let color: LaneColor
    let lanes: Int
    let head: Bool
    nonisolated static let laneWidth: CGFloat = 14
    nonisolated static let maxLanes = 8

    var body: some View {
        Canvas { ctx, size in
            let mid = size.height / 2
            let dot = CGPoint(x: Self.x(column), y: mid)
            for t in edges.through {
                stroke(&ctx, from: CGPoint(x: Self.x(t.from), y: 0), to: CGPoint(x: Self.x(t.to), y: size.height), t.color)
            }
            for e in edges.into {
                stroke(&ctx, from: CGPoint(x: Self.x(e.column), y: 0), to: dot, e.color)
            }
            for e in edges.out {
                stroke(&ctx, from: dot, to: CGPoint(x: Self.x(e.column), y: size.height), e.color)
            }
            let fill = Self.token(color).color
            let r: CGFloat = head ? 5 : 4
            let circle = Path(ellipseIn: CGRect(x: dot.x - r, y: dot.y - r, width: r * 2, height: r * 2))
            if head {
                ctx.fill(circle, with: .color(Palette.surface.color))
                ctx.stroke(circle, with: .color(fill), lineWidth: 2.5)
            } else {
                ctx.fill(circle, with: .color(fill))
            }
        }
        .frame(width: Self.laneWidth * CGFloat(max(lanes, 1)))
        .clipped()
        .accessibilityHidden(true)
    }

    private func stroke(_ ctx: inout GraphicsContext, from: CGPoint, to: CGPoint, _ color: LaneColor) {
        var p = Path()
        p.move(to: from)
        if from.x == to.x {
            p.addLine(to: to)
        } else {
            let midY = (from.y + to.y) / 2
            p.addCurve(to: to, control1: CGPoint(x: from.x, y: midY), control2: CGPoint(x: to.x, y: midY))
        }
        ctx.stroke(p, with: .color(Self.token(color).color), style: StrokeStyle(lineWidth: 1.75, lineCap: .round))
    }

    nonisolated static func x(_ lane: Int) -> CGFloat { laneWidth * (CGFloat(lane) + 0.5) }

    nonisolated static func token(_ color: LaneColor) -> Token {
        switch color {
        case .head: Palette.graphHead
        case .upstream: Palette.graphUpstream
        case .base: Palette.graphBase
        case .lane(let n): Palette.graphLanes[n % Palette.graphLanes.count]
        }
    }
}
