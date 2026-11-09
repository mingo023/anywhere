import DesignSystem
import FeatureSessions
import Stores
import SwiftUI

/// The iPad's left column: each Mac, foldable, with its Sessions and projects, and Settings pinned below.
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
    @State private var bottomInset: CGFloat = 0

    var body: some View {
        List(selection: $selection) {
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
        .navigationTitle("Anywhere")
        .navigationSplitViewColumnWidth(min: 220, ideal: 260)
        .onGeometryChange(for: Bool.self) { $0.size.width < 240 } action: { narrow = $0 }
        .onChange(of: macId, initial: true) { if let macId { folds.openMacs.insert(macId) } }
        .safeAreaInset(edge: .bottom, spacing: 0) { settings }
        .background {
            // Without the keyboard's height, which would sink the button under the keyboard.
            Color.clear.ignoresSafeArea(.keyboard, edges: .bottom)
                .onGeometryChange(for: CGFloat.self) { $0.safeAreaInsets.bottom } action: { bottomInset = $0 }
        }
    }

    @ViewBuilder
    private func rows(_ host: HostModel) -> some View {
        let sessions = PadRoot.Item(tab: .sessions, mac: host.id)
        SidebarRow("Sessions", selected: sessions == selection) {
            Image(systemName: "bubble.left")
        } trailing: {
            if host.sessions.upNext.count > 0 { SidebarCount(host.sessions.upNext.count) }
        }
        .sidebarListRow(selected: sessions == selection, indent: Self.indent)
        .tag(sessions)
        TreeSidebar(host: host, selected: selection, folded: $folds.foldedProjects, projectIndent: Self.indent, treeIndent: narrow ? 40 : 48) {
            PadRoot.Item(tab: .sessions, mac: host.id, tree: $0)
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
                    Image(systemName: "laptopcomputer").font(.system(size: 14)).foregroundStyle(Palette.text3.color)
                    HStack(spacing: narrow ? 6 : Space.s) {
                        Text(host.name).font(Typography.body.bold()).foregroundStyle(Palette.text.color).lineLimit(1)
                        if reach == .online { Circle().fill(Palette.success.color).frame(width: 6, height: 6) }
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

    private var settings: some View {
        let item = PadRoot.Item(tab: .settings, mac: nil)
        let selected = item == selection
        return VStack(spacing: Space.s) {
            Rectangle().fill(Palette.separator.color).frame(height: 0.5)
            Button { selection = item } label: {
                SidebarRow("Settings", selected: selected) {
                    Image(systemName: "gearshape")
                } trailing: {
                    EmptyView()
                }
                .padding(.horizontal, Space.m)
                .frame(height: 36)
                .background((selected ? Palette.accentBackground : Palette.fill2).color, in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.horizontal, Space.xl)
        }
        // Into the safe area, whose height would otherwise leave three times the room below the button as above it.
        .padding(.bottom, Space.m - bottomInset)
        .background(Palette.surface.color.ignoresSafeArea())
    }

    #if os(iOS) && compiler(>=6.2)
        /// The title at the column's leading edge, level with the system's sidebar toggle.
        @ToolbarContentBuilder
        private var leadingTitle: some ToolbarContent {
            if #available(iOS 26, *) {
                NoBarTitle()
                ToolbarItem(placement: .topBarLeading) {
                    Text("Anywhere").font(Typography.headline).foregroundStyle(Palette.text.color).fixedSize()
                }
                .sharedBackgroundVisibility(.hidden)
            }
        }
    #endif

    private static let indent: CGFloat = 20
}
