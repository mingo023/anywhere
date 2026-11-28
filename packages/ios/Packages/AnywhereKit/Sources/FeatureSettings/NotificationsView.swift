import DesignSystem
import Stores
import SwiftUI
import Wire

/// What to be told about, and the Live Activity.
@MainActor
public struct NotificationsView: View {
    @Bindable var center: PushCenter
    @Environment(\.openURL) private var openURL

    public init(center: PushCenter = .shared) {
        self.center = center
    }

    public var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ScreenTitle("Notifications")
                if center.permission == .denied { denied.padding(.top, Space.l) }
                GroupHeader("Notify me about")
                VStack(spacing: 0) {
                    ToggleRow("Needs you", isOn: pref(\.needsYou), first: true)
                    ToggleRow("Failed", isOn: pref(\.failed))
                    ToggleRow("Finished", isOn: pref(\.done))
                    ToggleRow("Automations", isOn: pref(\.automations))
                }
                .insetGroup()
                VStack(spacing: 0) {
                    ToggleRow("Live Activities", isOn: Binding(get: { center.liveActivities }, set: { center.set(liveActivities: $0) }), first: true)
                    ToggleRow("Show commands", isOn: pref(\.details))
                        .disabled(!center.liveActivities)
                        .opacity(center.liveActivities ? 1 : 0.45)
                }
                .insetGroup()
                .padding(.top, Space.xl)
                GroupFooter("Show commands puts the pending command on the Lock Screen and in notifications. Live Activity updates pass through Apple unencrypted.")
            }
        }
        .screenBackground()
        .pushedScreen("Notifications")
    }

    private var denied: some View {
        HStack(spacing: Space.m) {
            LucideIcon(.bellOff, size: 20).foregroundStyle(Palette.text3.color)
            Text("Notifications are off for Zoes in iOS Settings.").font(.system(size: 15)).fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
            Button("Open Settings", action: openSettings)
                .buttonStyle(.plain)
                .fontWeight(.semibold)
        }
        .font(.system(size: 15))
        .foregroundStyle(Palette.text.color)
        .padding(.horizontal, Inset.margin)
        .padding(.vertical, Space.m)
        .insetGroup()
    }

    private func openSettings() {
        #if os(iOS)
            if let url = URL(string: UIApplication.openNotificationSettingsURLString) { openURL(url) }
        #endif
    }

    private func pref(_ key: WritableKeyPath<PushPrefs, Bool>) -> Binding<Bool> {
        Binding(get: { center.prefs[keyPath: key] }, set: {
            var prefs = center.prefs
            prefs[keyPath: key] = $0
            center.set(prefs)
        })
    }
}
