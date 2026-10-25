import DesignSystem
import Stores
import SwiftUI
import Wire

/// What to be told about, the Live Activity, and each Mac's push registration, as sections of the Settings form.
@MainActor
public struct NotificationsSection: View {
    /// One Mac's row: its name, where its registration stands, and its test alert.
    struct Row: Identifiable {
        let id: String
        let name: String
        let state: PushStore.State
        let test: @MainActor () async throws -> Void
    }

    @Bindable var center: PushCenter
    let rows: [Row]
    @State private var testing: Set<String> = []
    @State private var tested: [String: String] = [:]
    @Environment(\.openURL) private var openURL

    public init(center: PushCenter = .shared, hosts: [HostModel]) {
        self.init(center: center, rows: hosts.map { host in
            Row(id: host.id, name: host.name, state: host.push.state, test: { try await host.push.sendTest() })
        })
    }

    init(center: PushCenter, rows: [Row]) {
        self.center = center
        self.rows = rows
    }

    public var body: some View {
        Section {
            if center.permission == .denied { denied }
            Toggle("Needs You", isOn: pref(\.needsYou))
            Toggle("Done", isOn: pref(\.done))
            Toggle("Failed", isOn: pref(\.failed))
            Toggle("Automations", isOn: pref(\.automations))
        } header: {
            Text("Notifications")
        } footer: {
            Text("Your Mac encrypts each alert so only this phone can read what it says.")
        }
        Section {
            Toggle("Live Activities", isOn: Binding(get: { center.liveActivities }, set: { center.set(liveActivities: $0) }))
            Toggle("Show Commands", isOn: pref(\.details))
                .disabled(!center.liveActivities)
        } footer: {
            Text("Shows what a session asks to run on the Lock Screen and in the Dynamic Island. Live Activity updates pass through Apple unencrypted.")
        }
        if !rows.isEmpty {
            Section("Alerts from Each Mac") {
                ForEach(rows) { row in macRow(row) }
            }
        }
    }

    private var denied: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            Label("Notifications are off for Anywhere", systemImage: "bell.slash.fill")
                .foregroundStyle(Palette.failedText.color)
            #if os(iOS)
                Button("Turn On in Settings") {
                    if let url = URL(string: UIApplication.openNotificationSettingsURLString) { openURL(url) }
                }
            #endif
        }
    }

    private func macRow(_ row: Row) -> some View {
        HStack(spacing: Space.m) {
            Image(systemName: "laptopcomputer").foregroundStyle(Palette.text3.color)
            VStack(alignment: .leading, spacing: 2) {
                Text(row.name).foregroundStyle(Palette.text.color)
                Text(tested[row.id] ?? Self.status(row.state, permission: center.permission))
                    .font(Typography.footnote)
                    .foregroundStyle(Self.failed(row.state) ? Palette.failedText.color : Palette.text3.color)
            }
            Spacer()
            if testing.contains(row.id) {
                ProgressView().controlSize(.small)
            } else {
                Button("Send Test") { test(row) }
                    .buttonStyle(.borderless)
                    .disabled(row.state != .registered)
            }
        }
    }

    private func test(_ row: Row) {
        testing.insert(row.id)
        Task {
            do {
                try await row.test()
                tested[row.id] = "Test notification sent"
            } catch {
                tested[row.id] = PushStore.describe(error)
            }
            testing.remove(row.id)
        }
    }

    private func pref(_ key: WritableKeyPath<PushPrefs, Bool>) -> Binding<Bool> {
        Binding(get: { center.prefs[keyPath: key] }, set: {
            var prefs = center.prefs
            prefs[keyPath: key] = $0
            center.set(prefs)
        })
    }

    nonisolated static func status(_ state: PushStore.State, permission: PushCenter.Permission) -> String {
        switch state {
        case .unsupported: "Update Anywhere on this Mac to get notifications"
        case .waiting: permission == .denied ? "Notifications are off" : "Not registered yet"
        case .registering: "Registering…"
        case .registered: "Notifies this phone"
        case .failed(let why): why
        }
    }

    nonisolated static func failed(_ state: PushStore.State) -> Bool {
        if case .failed = state { true } else { false }
    }
}
