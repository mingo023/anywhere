import DesignSystem
import Foundation
import Stores
import SwiftUI

/// Sessions as they stood when their Mac went offline: a strip says since when, and rows dim and stop their timers.
/// A blip under `Reachability.reconnectingAfter` shows nothing.
@MainActor
struct Stale: ViewModifier {
    let host: HostModel
    @State private var asOf: Int?

    func body(content: Content) -> some View {
        content
            .environment(\.staleSince, asOf)
            .safeAreaInset(edge: .top, spacing: 0) {
                if let asOf { StaleStrip(mac: host.name, asOf: asOf) }
            }
            .task(id: host.link) {
                asOf = nil
                guard host.link != .online, host.hello != nil, let since = host.offlineSince else { return }
                let wait = Reachability.reconnectingAfter - Date().timeIntervalSince(since)
                if wait > 0 { try? await Task.sleep(for: .seconds(wait)) }
                guard !Task.isCancelled else { return }
                asOf = Int(since.timeIntervalSince1970 * 1000)
            }
    }
}

private struct StaleSinceKey: EnvironmentKey {
    static let defaultValue: Int? = nil
}

extension EnvironmentValues {
    /// When the Mac the rows come from went offline, in ms; nil while it is online.
    var staleSince: Int? {
        get { self[StaleSinceKey.self] }
        set { self[StaleSinceKey.self] = newValue }
    }
}

extension View {
    @MainActor
    func stale(_ host: HostModel) -> some View {
        modifier(Stale(host: host))
    }
}
