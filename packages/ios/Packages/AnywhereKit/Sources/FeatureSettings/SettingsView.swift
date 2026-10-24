import DesignSystem
import Domain
import Stores
import SwiftUI

/// Paired Macs, notifications, the Live Activity and about.
@MainActor
public struct SettingsView: View {
    let app: AppModel

    public init(app: AppModel) {
        self.app = app
    }

    public var body: some View {
        Placeholder("Settings", systemImage: "gearshape")
    }
}
