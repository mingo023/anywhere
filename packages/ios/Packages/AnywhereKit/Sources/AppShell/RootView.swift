import DesignSystem
import Domain
import FeaturePairing
import FeatureSettings
import Stores
import SwiftUI

/// The app's root: pairing until a Mac is paired, then tabs on iPhone and a split view on iPad. It opens deep links,
/// restores where the user was, and tells each Mac when the app goes to the background.
@MainActor
public struct RootView: View {
    public enum Layout: Sendable {
        case phone
        case pad
    }

    let app: AppModel
    let layout: Layout?
    @State private var navigator: Navigator
    @State private var restored = false
    @SceneStorage("navigation") private var savedNavigation: Data?
    @AppStorage(Appearance.key) private var appearance = Appearance.system
    @Environment(\.scenePhase) private var scenePhase
    #if os(iOS)
        @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    /// `layout` forces one layout, as the gallery does; nil follows the size class.
    public init(app: AppModel, layout: Layout? = nil, navigator: Navigator = Navigator()) {
        self.app = app
        self.layout = layout
        _navigator = State(initialValue: navigator)
    }

    public var body: some View {
        Group {
            if app.hosts.isEmpty {
                // Clearing the request on success stops the sheet below from showing it again once a Mac exists.
                PairingView(app: app, link: navigator.pairing?.link) { navigator.pairing = nil }
                    .id(navigator.pairing?.id)
            } else if resolvedLayout == .pad {
                PadRoot(app: app, navigator: navigator)
            } else {
                PhoneRoot(app: app, navigator: navigator)
            }
        }
        .preferredColorScheme(appearance.colorScheme)
        .tint(Palette.accent.color)
        .sheet(item: pairingSheet) { request in
            PairingView(app: app, link: request.link, knownHost: request.knownHost) { navigator.pairing = nil }
        }
        .onOpenURL { url in
            guard let link = DeepLink.parse(url) else { return }
            navigator.open(link, macs: macs)
        }
        .onChange(of: macs, initial: true) { _, macs in
            if !restored {
                restored = true
                if let savedNavigation { navigator.restore(savedNavigation, macs: macs) }
            }
            navigator.keepSelection(macs)
            navigator.release(macs)
        }
        .onChange(of: navigator.saved) { _, _ in savedNavigation = navigator.encoded() }
        .onChange(of: scenePhase) { _, phase in
            for host in app.hosts { host.scene(active: phase == .active) }
        }
    }

    private var resolvedLayout: Layout {
        if let layout { return layout }
        #if os(iOS)
            return sizeClass == .regular ? .pad : .phone
        #else
            return .pad
        #endif
    }

    private var macs: [MacRef] {
        app.hosts.map { MacRef(id: $0.id, serverId: $0.hello?.serverId ?? $0.credentials.serverId, online: $0.link == .online) }
    }

    /// With no Mac yet, the root itself is the pairing flow, so no sheet.
    private var pairingSheet: Binding<PairingRequest?> {
        Binding(get: { app.hosts.isEmpty ? nil : navigator.pairing }, set: { navigator.pairing = $0 })
    }
}
