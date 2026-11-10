import DesignSystem
import Stores
import SwiftUI
import Wire

/// One Mac: its link, Tailscale and keep-awake, what this phone may do on it, and forgetting it.
@MainActor
struct MacSettingsView: View {
    let app: AppModel
    let host: HostModel
    @State private var forgetting = false
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        Form {
            Section {
                LabeledContent("Status") {
                    TimelineView(.explicit(host.reachabilityMarks)) { _ in Text(host.reachability(at: .now).label) }
                }
                LabeledContent("Address") { Text(host.credentials.host).font(Typography.codeSmall).textSelection(.enabled) }
                if let version = host.hello?.version { LabeledContent("Anywhere", value: version) }
                if let state = host.hostState {
                    LabeledContent("Tailscale", value: state.tailnet ? "Connected" : "Off")
                    LabeledContent("Keeping awake", value: state.keepingAwake ? "On" : "Off")
                    if let failed = state.upgradeFailed {
                        LabeledContent("Last update", value: "\(failed) failed").foregroundStyle(Palette.failedText.color)
                    }
                }
                if host.reachability(at: .now) != .online {
                    Button("Reconnect", systemImage: "arrow.clockwise") {
                        Haptics.tap()
                        if host.reachability(at: .now).isFinal { app.restart(host.id) } else { host.scene(active: true) }
                    }
                }
            } header: {
                Text("Connection")
            }

            Section {
                ForEach(MacFacts.grants(host.hello?.scopes)) { grant in
                    Label {
                        Text(grant.title).foregroundStyle(grant.allowed ? Palette.text.color : Palette.text4.color)
                    } icon: {
                        Image(systemName: grant.allowed ? grant.systemImage : "xmark")
                            .foregroundStyle(grant.allowed ? Palette.text2.color : Palette.text4.color)
                    }
                    .accessibilityValue(grant.allowed ? "Allowed" : "Not allowed")
                }
            } header: {
                Text("This iPhone can")
            } footer: {
                Text("The Mac decides these in Anywhere → Settings → Phone.")
            }

            Section {
                Button("Forget this Mac", role: .destructive) { forgetting = true }
            } footer: {
                Text("This iPhone stops connecting to \(host.name). To remove it on the Mac too, revoke it in Anywhere → Settings → Phone.")
            }
        }
        .formStyle(.grouped)
        .navigationTitle(host.name)
        .confirmationDialog("Forget \(host.name)?", isPresented: $forgetting, titleVisibility: .visible) {
            Button("Forget", role: .destructive) {
                dismiss()
                app.forget(host.id)
            }
        }
    }
}
