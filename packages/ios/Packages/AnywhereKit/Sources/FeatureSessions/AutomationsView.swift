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
        TimelineView(.everyMinute) { context in
            let now = Int(context.date.timeIntervalSince1970 * 1000)
            let reachable = host.link == .online
            AutomationList(
                lines: (all?.automations ?? []).map { automation in
                    let last = all?.runs.first { $0.automationId == automation.id }
                    return AutomationText.line(
                        automation, last: last, project: AutomationText.project(of: automation.folder, in: host.workspace.projects), mac: host.name,
                        reachable: reachable, now: now)
                },
                mac: mac,
                setEnabled: { id, on in run { try await host.workspace.setAutomation(id, enabled: on) } },
                runNow: { id in run { try await host.workspace.runAutomation(id) } })
        }
        .alert("Something went wrong", isPresented: Binding(get: { failure != nil }, set: { if !$0 { failure = nil } })) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(failure ?? "")
        }
    }

    private func run(_ work: @escaping @MainActor () async throws -> Void) {
        Task {
            do { try await work() } catch {
                guard let text = ErrorText.describe(error, mac: host.name) else { return }
                Haptics.warning()
                failure = text
            }
        }
    }
}

/// The screen: a back button, the large title, and one inset group of rows.
@MainActor
struct AutomationList: View {
    let lines: [AutomationText.Line]
    let mac: String
    let setEnabled: (String, Bool) -> Void
    let runNow: (String) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        ScrollView {
            if !lines.isEmpty { group }
        }
        .overlay {
            if lines.isEmpty {
                Text("No automations yet. Make one on your Mac.")
                    .font(.system(size: 15)).foregroundStyle(Palette.text2.color).multilineTextAlignment(.center).padding(.horizontal, 40)
            }
        }
        .safeAreaInset(edge: .top, spacing: 0) { header }
        .screenBackground()
        #if os(iOS)
            .toolbar(.hidden, for: .navigationBar)
        #endif
    }

    private var group: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(spacing: 0) {
                ForEach(Array(lines.enumerated()), id: \.element.id) { index, line in
                    AutomationRow(line: line, mac: mac, setEnabled: { setEnabled(line.id, $0) }, runNow: { runNow(line.id) })
                        .rowDivider(index > 0, leading: Inset.margin)
                }
            }
            .insetGroup()
            Text("Make and edit automations on your Mac.")
                .font(.system(size: 13)).foregroundStyle(Palette.text3.color)
                .padding(.horizontal, Inset.margin * 2).padding(.top, Space.s)
        }
        .padding(.top, Inset.margin)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 0) {
            Button { dismiss() } label: { LucideIcon(.chevronLeft, size: 20) }
                .buttonStyle(RoundButtonStyle())
                .accessibilityLabel("Back")
                .padding(.leading, Inset.margin)
            ScreenTitle("Automations")
        }
        .background(Palette.background.color)
    }
}

@MainActor
struct AutomationRow: View {
    let line: AutomationText.Line
    let mac: String
    let setEnabled: (Bool) -> Void
    let runNow: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: Space.m) {
                Text(line.name).font(.system(size: 17)).foregroundStyle(Palette.text.color).lineLimit(1)
                Spacer(minLength: 0)
                InkSwitch(isOn: Binding(get: { line.enabled }, set: { setEnabled($0) })).accessibilityLabel("On")
            }
            Text(line.detail).font(.system(size: 13)).foregroundStyle(Palette.text2.color).padding(.top, 2).lineLimit(1)
            HStack(spacing: Space.m) {
                last
                Spacer(minLength: 0)
                Button("Run now", action: runNow).buttonStyle(PillButtonStyle(.secondary, height: 30))
            }
            .padding(.top, 6)
        }
        .padding(.vertical, Space.m)
        .padding(.horizontal, Inset.margin)
        .disabled(!line.reachable)
        .opacity(line.reachable ? 1 : 0.45)
    }

    @ViewBuilder
    private var last: some View {
        let text = Text(line.last).font(.system(size: 13)).foregroundStyle((line.failed ? Palette.failedText : Palette.text3).color).lineLimit(1)
        if let agent = line.agentId {
            Link(destination: DeepLink.agent(mac: mac, agentId: agent).url) { text }
        } else {
            text
        }
    }
}

/// The design's switch: ink when on, a quiet fill when off.
@MainActor
private struct InkSwitch: View {
    @Binding var isOn: Bool

    var body: some View {
        Button { isOn.toggle() } label: {
            Capsule()
                .fill((isOn ? Palette.text : Palette.fill4).color)
                .frame(width: 52, height: 32)
                .overlay(alignment: isOn ? .trailing : .leading) {
                    Circle()
                        .fill(isOn ? Palette.onText.color : .white)
                        .frame(width: 28, height: 28)
                        .shadow(color: .black.opacity(0.25), radius: 2, y: 2)
                        .padding(2)
                }
                .animation(.smooth(duration: 0.2), value: isOn)
        }
        .buttonStyle(.plain)
        .accessibilityRepresentation { Toggle("On", isOn: $isOn) }
    }
}
