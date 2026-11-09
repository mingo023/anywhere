import DesignSystem
import FeatureSessions
import Stores
import SwiftUI

/// The iPad's left column: each Mac, foldable, with its Sessions and projects, and Settings pinned below.
@MainActor
struct PadSidebar: View {
    let app: AppModel
    @Binding var selection: PadRoot.Item?
    let macId: String?
    @State private var open: Set<String> = []
    @State private var narrow = false

    var body: some View {
        List(selection: $selection) {
            ForEach(app.hosts) { host in
                Section {
                    // A row, as a section header adds 24pt the sidebar has no way to take back.
                    header(host)
                    if open.contains(host.id) { rows(host) }
                }
                #if os(iOS)
                    .listSectionSpacing(open.contains(host.id) ? Space.s : 0)
                #endif
            }
        }
        .listStyle(.sidebar)
        .environment(\.defaultMinListRowHeight, SidebarRow<EmptyView, EmptyView>.height)
        #if os(iOS)
            .listRowSpacing(0)
            .listSectionSpacing(0)
            .navigationBarTitleDisplayMode(.large)
        #endif
        .navigationTitle("Anywhere")
        .navigationSplitViewColumnWidth(min: 220, ideal: 260)
        .onGeometryChange(for: Bool.self) { $0.size.width < 240 } action: { narrow = $0 }
        .onChange(of: macId, initial: true) { if let macId { open.insert(macId) } }
        .safeAreaInset(edge: .bottom, spacing: 0) { settings }
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
        TreeSidebar(host: host, selected: selection, projectIndent: Self.indent, treeIndent: narrow ? 40 : 48) {
            PadRoot.Item(tab: .sessions, mac: host.id, tree: $0)
        }
    }

    private func header(_ host: HostModel) -> some View {
        let isOpen = open.contains(host.id)
        return Button {
            if isOpen { open.remove(host.id) } else { open.insert(host.id) }
        } label: {
            TimelineView(.explicit(host.reachabilityMarks)) { context in
                let reach = host.reachability(at: context.date)
                HStack(spacing: Space.s) {
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
                .padding(.bottom, Space.s)
                .frame(height: 44, alignment: .bottom)
                .contentShape(Rectangle())
            }
        }
        .buttonStyle(.plain)
        .listRowInsets(EdgeInsets(top: 0, leading: narrow ? 16 : 20, bottom: 0, trailing: narrow ? 16 : 20))
        .listRowBackground(Color.clear)
        .accessibilityAddTraits(.isHeader)
        .accessibilityValue(host.reachability(at: .now).label)
        .accessibilityHint(isOpen ? "Hides this Mac's sessions" : "Shows this Mac's sessions")
    }

    private var settings: some View {
        let item = PadRoot.Item(tab: .settings, mac: nil)
        return VStack(spacing: 6) {
            Rectangle().fill(Palette.separator.color).frame(height: 0.5)
            Button { selection = item } label: {
                SidebarRow("Settings", selected: item == selection) {
                    Image(systemName: "gearshape")
                } trailing: {
                    EmptyView()
                }
                .padding(.leading, Self.indent)
                .padding(.trailing, 20)
                .background(SidebarSelection(selected: item == selection))
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .padding(.bottom, 26)
        }
    }

    private static let indent: CGFloat = 20
}
