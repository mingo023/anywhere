import DesignSystem
import Domain
import Foundation
import Stores
import SwiftUI

/// What Changes shows with nothing to commit: that, and the last three commits over a way into History.
@MainActor
struct NoChanges: View {
    let host: HostModel
    let mac: String
    let tree: String

    var body: some View {
        let store = host.git
        let commits = store.history(tree).commits.prefix(3)
        let now = Int(Date().timeIntervalSince1970)
        VStack(spacing: 0) {
            Text("No changes")
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(Palette.text.color)
                .padding(.top, Space.xxl)
            GroupHeader("Last commits")
            VStack(spacing: 0) {
                ForEach(Array(commits.enumerated()), id: \.element.sha) { i, commit in
                    PushRow(value: .commit(mac: mac, tree: tree, sha: commit.sha)) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(commit.subject).font(Typography.body).foregroundStyle(Palette.text.color).lineLimit(1)
                            HStack(spacing: 0) {
                                Text(GitText.short(commit.sha)).font(.system(size: 13, design: .monospaced))
                                Text(" · \(commit.author) · \(GitText.age(commit.at, now: now))")
                            }
                            .font(Typography.footnote)
                            .foregroundStyle(Palette.text2.color)
                        }
                        .padding(.vertical, Space.m)
                        .padding(.leading, Space.l)
                        .padding(.trailing, Space.l)
                    }
                    .buttonStyle(.plain)
                    .rowDivider(i > 0, leading: Space.l)
                }
                PushRow(value: .history(mac: mac, tree: tree)) {
                    HStack {
                        Text("Show all in History").font(Typography.body).foregroundStyle(Palette.text.color)
                        Spacer()
                        LucideIcon(.chevronRight, size: 13).foregroundStyle(Palette.text4.color)
                    }
                    .padding(.horizontal, Space.l)
                    .frame(minHeight: HitTarget.minimum)
                }
                .buttonStyle(.plain)
                .rowDivider(!commits.isEmpty, leading: Space.l)
            }
            .insetGroup()
        }
        .task { if commits.isEmpty { await store.loadHistory(tree) } }
    }
}
