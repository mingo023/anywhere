import DesignSystem
import Domain
import Foundation
import Stores
import SwiftUI
import Wire

/// The Mac's automations: each one's schedule, whether it is on, and its last run. pocketd sends them only to its owner.
@MainActor
struct AutomationsView: View {
    let host: HostModel
    let mac: String
    @State private var failure: String?

    var body: some View {
        let all = host.workspace.automations
        List {
            ForEach(all?.automations ?? []) { automation in
                AutomationRow(automation: automation, last: all?.runs.first { $0.automationId == automation.id }, mac: mac,
                              enabled: Binding(get: { automation.enabled }, set: { on in run { try await host.workspace.setAutomation(automation.id, enabled: on) } }))
                    .swipeActions {
                        Button { run { try await host.workspace.runAutomation(automation.id) } } label: { Label("Run Now", systemImage: "play.fill") }
                            .tint(Palette.accent.color)
                    }
                    .contextMenu {
                        Button { run { try await host.workspace.runAutomation(automation.id) } } label: { Label("Run Now", systemImage: "play.fill") }
                    }
            }
        }
        .overlay {
            if all?.automations.isEmpty ?? true {
                ContentUnavailableView("No automations", systemImage: "clock.arrow.circlepath", description: Text("Make one on your Mac."))
            }
        }
        .navigationTitle("Automations")
        .alert("Something went wrong", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
    }

    private func run(_ work: @escaping @MainActor () async throws -> Void) {
        Task {
            do { try await work() } catch {
                Haptics.warning()
                failure = WorkspaceStore.serverError(error).message
            }
        }
    }
}

@MainActor
struct AutomationRow: View {
    let automation: Automation
    let last: Run?
    let mac: String
    @Binding var enabled: Bool

    var body: some View {
        TimelineView(.everyMinute) { context in
            let now = Int(context.date.timeIntervalSince1970 * 1000)
            HStack(alignment: .top, spacing: Space.m) {
                VStack(alignment: .leading, spacing: Space.xxs) {
                    Text(automation.name).font(Typography.body.weight(.medium)).foregroundStyle(Palette.text.color)
                    Text(AutomationText.trigger(automation, now: now)).font(Typography.footnote).foregroundStyle(Palette.text3.color)
                    if let last {
                        let label = AutomationText.last(last, now: now)
                        if let agentId = last.agentId {
                            Link(destination: DeepLink.agent(mac: mac, agentId: agentId).url) {
                                Text("Last run: \(label)").font(Typography.footnote)
                            }
                        } else {
                            Text("Last run: \(label)").font(Typography.footnote).foregroundStyle(Palette.text3.color)
                        }
                    }
                }
                Spacer()
                Toggle("On", isOn: $enabled).labelsHidden()
            }
        }
    }
}
