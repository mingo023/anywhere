import DesignSystem
import Domain
import Stores
import SwiftUI

extension View {
    /// The screen's glass buttons in its top corners. On iOS they are the navigation bar's items, which the system draws
    /// in glass, over a title the screen sets itself; the gallery has no bar, so macOS draws them over the top, with a
    /// back button for a screen that is pushed.
    func cornerButtons<Trailing: View>(back: Bool = false, @ViewBuilder trailing: () -> Trailing) -> some View {
        #if os(iOS)
            toolbar {
                NoBarTitle()
                ToolbarItemGroup(placement: .primaryAction) { trailing() }
            }
            .navigationBarTitleDisplayMode(.inline)
        #else
            modifier(GalleryCorner(back: back, trailing: trailing()))
        #endif
    }
}

extension EnvironmentValues {
    /// A strip the shell would put under the navigation bar, for the gallery to draw there.
    @Entry var topStrip: AnyView? = nil
}

#if !os(iOS)
    /// The gallery's stand-in for the navigation bar: the corner buttons, and the shell's strip under them.
    @MainActor
    private struct GalleryCorner<Trailing: View>: ViewModifier {
        let back: Bool
        let trailing: Trailing
        @Environment(\.topStrip) private var strip

        func body(content: Content) -> some View {
            content.safeAreaInset(edge: .top, spacing: 0) {
                VStack(spacing: 0) {
                    HStack(spacing: Space.s) {
                        if back { Button {} label: { LucideIcon(.chevronLeft, size: 20) }.buttonStyle(RoundButtonStyle()) }
                        Spacer(minLength: 0)
                        trailing.buttonStyle(RoundButtonStyle())
                    }
                    .padding(.horizontal, Inset.margin)
                    .frame(height: HitTarget.minimum)
                    if let strip { strip.padding(.horizontal, Inset.margin).padding(.vertical, Space.xs) }
                }
            }
        }
    }
#endif

/// The corner's +: a new session on the first Mac that is online, with no Mac to start one on otherwise.
@MainActor
struct NewSessionButton: View {
    let hosts: [HostModel]
    var project: String?
    var place: String?
    let open: ((Route) -> Void)?

    var body: some View {
        let online = hosts.first { $0.link == .online }
        Button { if let online { open?(.newSession(mac: online.mac, project: project, place: place)) } } label: {
            LucideIcon(.plus, size: 20).foregroundStyle(Palette.text.color)
        }
        .disabled(online == nil)
        .accessibilityLabel("New session")
    }
}

/// The small round ··· beside a screen's title, opening its menu.
@MainActor
struct TitleMenu<Content: View>: View {
    @ViewBuilder let content: Content

    var body: some View {
        Menu { content } label: {
            LucideIcon(.ellipsis, size: 17)
                .foregroundStyle(Palette.textBody.color)
                .frame(width: 32, height: 32)
                .background(Palette.fill3.color, in: Circle())
                .contentShape(Circle())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .accessibilityLabel("More")
    }
}
