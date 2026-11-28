import DesignSystem
import FeatureSessions
import Stores
import SwiftUI

/// The iPad's left column: Inbox, Sessions, PRs and Machines, then each Mac, foldable, with its projects.
@MainActor
struct PadSidebar: View {
    struct Folds {
        var openMacs: Set<String> = []
        var foldedProjects: Set<String> = []
    }

    let app: AppModel
    @Binding var selection: PadRoot.Item?
    let macId: String?
    @Binding var folds: Folds
    @State private var narrow = false

    var body: some View {
        List(selection: $selection) {
            Section {
                ForEach(TabItems.items(app.hosts)) { tab in
                    let item = PadRoot.Item(tab: tab.id)
                    SidebarRow(tab.title, selected: item == selection) {
                        LucideIcon(tab.icon, size: 17)
                    } trailing: {
                        if tab.badge > 0 { SidebarCount(tab.badge) }
                    }
                    .sidebarListRow(selected: item == selection, indent: 0)
                    .tag(item)
                }
            }
            ForEach(app.hosts) { host in
                Section {
                    // A row, as a section header adds 24pt the sidebar has no way to take back.
                    header(host)
                    if folds.openMacs.contains(host.id) { rows(host) }
                }
                #if os(iOS)
                    .listSectionSpacing(folds.openMacs.contains(host.id) ? Space.s : 0)
                #endif
            }
        }
        .listStyle(.sidebar)
        .scrollContentBackground(.hidden)
        .background(Palette.surface.color.ignoresSafeArea())
        .solidBar(Palette.surface.color)
        .environment(\.defaultMinListRowHeight, SidebarRow<EmptyView, EmptyView>.height)
        #if os(iOS)
            .listRowSpacing(0)
            .listSectionSpacing(0)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        #if os(iOS) && compiler(>=6.2)
            .toolbar { leadingTitle }
        #endif
        .navigationTitle("Zoes")
        .navigationSplitViewColumnWidth(min: 220, ideal: 260)
        .onGeometryChange(for: Bool.self) { $0.size.width < 240 } action: { narrow = $0 }
        .onChange(of: macId, initial: true) { if let macId { folds.openMacs.insert(macId) } }
    }

    private func rows(_ host: HostModel) -> some View {
        TreeSidebar(host: host, selected: selection, folded: $folds.foldedProjects, projectIndent: Self.indent, treeIndent: narrow ? 40 : 48) {
            PadRoot.Item(tab: .sessions, tree: $0)
        }
    }

    private func header(_ host: HostModel) -> some View {
        let isOpen = folds.openMacs.contains(host.id)
        return Button {
            if isOpen { folds.openMacs.remove(host.id) } else { folds.openMacs.insert(host.id) }
        } label: {
            TimelineView(.explicit(host.reachabilityMarks)) { context in
                let reach = host.reachability(at: context.date)
                HStack(spacing: Space.s) {
                    LucideIcon(.laptop, size: 14).foregroundStyle(Palette.text3.color)
                    HStack(spacing: narrow ? 6 : Space.s) {
                        Text(host.name).font(Typography.body.bold()).foregroundStyle(Palette.text.color).lineLimit(1)
                        LinkDot(reach.tone)
                    }
                    Spacer(minLength: 0)
                    if !isOpen {
                        if reach == .online {
                            if host.sessions.needsYouCount > 0 { SidebarCount(host.sessions.needsYouCount) }
                        } else if reach != .connecting {
                            Text(reach == .reconnecting ? reach.label : "Offline").font(Typography.footnote).foregroundStyle(Palette.text3.color).lineLimit(1)
                        }
                    }
                    SidebarChevron(open: isOpen)
                }
                .padding(.horizontal, Space.m)
                .frame(height: 36)
                .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
                .contentShape(Rectangle())
            }
        }
        .buttonStyle(.plain)
        .listRowInsets(EdgeInsets(top: 0, leading: Space.s, bottom: 0, trailing: Space.s))
        .listRowBackground(Color.clear)
        .accessibilityAddTraits(.isHeader)
        .accessibilityValue(host.reachability(at: .now).label)
        .accessibilityHint(isOpen ? "Hides this Mac's sessions" : "Shows this Mac's sessions")
    }

    #if os(iOS) && compiler(>=6.2)
        /// The title at the column's leading edge, level with the system's sidebar toggle.
        @ToolbarContentBuilder
        private var leadingTitle: some ToolbarContent {
            if #available(iOS 26, *) {
                NoBarTitle()
                ToolbarItem(placement: .topBarLeading) {
                    Text("Zoes").font(Typography.headline).foregroundStyle(Palette.text.color).fixedSize()
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
    #endif

    private static let indent: CGFloat = 20
}
