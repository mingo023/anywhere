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
    @ScaledMetric(relativeTo: .body) private var rowHeight: CGFloat = 58

    var body: some View {
        let store = host.git
        let compare = store.compare(tree, against: against)
        let rev = store.changes(tree).head?.rev
        ScrollView {
            if let totals = compare.totals, !compare.files.isEmpty {
                Text("\(totals.files) files · +\(totals.added) −\(totals.removed)")
                    .font(Typography.digits)
                    .foregroundStyle(Palette.text3.color)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, Space.l * 2)
                    .padding(.top, Space.l)
                    .padding(.bottom, Space.s)
                LazyVStack(spacing: 0) {
                    ForEach(compare.files, id: \.path) { f in
                        NavigationLink(value: CompareFileRoute(tree: tree, against: against, path: f.path)) {
                            FileRow(
                                path: f.path, oldPath: f.oldPath, status: f.status,
                                added: f.added, removed: f.removed, binary: f.added == nil && f.status != .deleted)
                            
                            .padding(.horizontal, Space.l)
                            .frame(height: rowHeight)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .overlay(alignment: .bottom) { Palette.separator.color.frame(height: 1).padding(.leading, Space.l + 16 + Space.s) }
                    }
                    if compare.nextCursor != nil {
                        ProgressView().frame(maxWidth: .infinity).padding(.vertical, Space.l)
                            .onAppear { Task { await store.loadCompare(tree, against: against, more: true) } }
                    }
                }
                .groupedCard()
                .padding(.bottom, Space.xl)
            }
        }
        .overlay {
            if compare.missing {
                Placeholder("Can't find \(against)", systemImage: "questionmark.folder", detail: "No branch or commit has that name.")
            } else if compare.totals == nil, let error = compare.error {
                Placeholder("Couldn't compare with \(against)", systemImage: "exclamationmark.triangle", detail: error) {
                    Task { await store.loadCompare(tree, against: against) }
                }
            } else if compare.totals == nil {
                VStack(spacing: Space.s) {
                    ProgressView()
                    Text("Comparing…").foregroundStyle(Palette.text3.color)
                }
            } else if compare.files.isEmpty {
                Placeholder("No differences with \(against)", systemImage: "checkmark.circle")
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
