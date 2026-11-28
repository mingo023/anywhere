import DesignSystem
import Stores
import SwiftUI
import Wire

/// A file of a compare, opened as its diff against the ref.
struct CompareFileRoute: Hashable {
    let tree: String
    let against: String
    let path: String
}

/// What a tree committed since it left `against`, read-only: no swipes, no composer, no staging. Rows are a lazy stack,
/// loading the next page near the end.
@MainActor
struct CompareList: View {
    let host: HostModel
    let tree: String
    let against: String
    /// `against` names nothing; the screen falls back to the base.
    let missing: () -> Void
    @ScaledMetric(relativeTo: .body) private var rowHeight: CGFloat = 61

    var body: some View {
        let store = host.git
        let compare = store.compare(tree, against: against)
        let head = store.changes(tree).head
        let rev = head?.rev
        ScrollView {
            if let totals = compare.totals, !compare.files.isEmpty {
                HStack {
                    Text(GitText.compareTitle(files: totals.files, ahead: head?.base == against ? head?.ahead : nil, against: against))
                    Spacer()
                    HStack(spacing: Space.xs) {
                        Text("+\(totals.added)").foregroundStyle(Palette.successText.color)
                        Text("−\(totals.removed)").foregroundStyle(Palette.failedText.color)
                    }
                    .font(Typography.codeSmall)
                }
                .font(Typography.footnote.weight(.semibold))
                .foregroundStyle(Palette.text2.color)
                .padding(.horizontal, Inset.margin * 2)
                .padding(.top, Space.xl)
                .padding(.bottom, Space.s)
                let first = compare.files.first?.path
                LazyVStack(spacing: 0) {
                    ForEach(compare.files, id: \.path) { f in
                        NavigationLink(value: CompareFileRoute(tree: tree, against: against, path: f.path)) {
                            FileRow(
                                path: f.path, oldPath: f.oldPath, status: f.status,
                                added: f.added, removed: f.removed, binary: f.added == nil && f.status != .deleted, letters: false)
                            .frame(height: rowHeight)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .rowDivider(f.path != first)
                    }
                    if compare.nextCursor != nil {
                        ProgressView().frame(maxWidth: .infinity).padding(.vertical, Space.l)
                            .onAppear { Task { await store.loadCompare(tree, against: against, more: true) } }
                    }
                }
                .insetGroup()
                .padding(.bottom, Space.xl)
            }
        }
        .overlay {
            if compare.missing {
                Placeholder("Can't find \(against)", icon: .search, detail: "No branch or commit has that name.")
            } else if compare.totals == nil, let error = compare.error {
                Placeholder("Couldn't compare with \(against)", icon: .triangleAlert, detail: error) {
                    Task { await store.loadCompare(tree, against: against) }
                }
            } else if compare.totals == nil {
                VStack(spacing: Space.s) {
                    ProgressView()
                    Text("Comparing…").foregroundStyle(Palette.text3.color)
                }
            } else if compare.files.isEmpty {
                Placeholder("No differences with \(against)", icon: .circleCheck)
            }
        }
        .refreshable { await store.loadCompare(tree, against: against) }
        .task(id: CompareLoad(against: against, rev: rev)) {
            await store.loadCompare(tree, against: against)
            if compare.missing { missing() }
        }
    }

    private struct CompareLoad: Equatable {
        let against: String
        let rev: String?
    }
}
