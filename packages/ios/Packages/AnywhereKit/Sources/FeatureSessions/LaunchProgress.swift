import DesignSystem
import Domain
import Stores
import SwiftUI

/// A launch's steps in its sheet. Hide, or a swipe down, leaves it running as a row under its project; if it starts
/// while shown, its chat opens.
@MainActor
struct LaunchProgress: View {
    let host: HostModel
    let mac: String
    let requestId: String
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    private var workspace: WorkspaceStore { host.workspace }

    var body: some View {
        let creation = workspace.creations[requestId]
        VStack(spacing: Space.xl) {
            Spacer(minLength: Space.xl)
            VStack(spacing: Space.xs) {
                if creation?.error != nil {
                    Image(systemName: "exclamationmark.triangle").font(.title2).foregroundStyle(Palette.failed.color).padding(.bottom, Space.xs)
                }
                Text(creation?.error?.message ?? "Starting").font(Typography.title).foregroundStyle(Palette.text.color)
                if let place { Text(place).font(Typography.subheadline).foregroundStyle(Palette.text3.color) }
            }
            .multilineTextAlignment(.center)
            if let detail = creation?.error?.detail {
                Text(detail)
                    .font(Typography.codeSmall)
                    .foregroundStyle(Palette.text3.color)
                    .padding(Space.m)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
            }
            LaunchSteps(creation: creation)
            Spacer(minLength: Space.xl)
            if creation?.error != nil {
                HStack(spacing: Space.s) {
                    Button { dismissFailed() } label: { Text("Dismiss").frame(maxWidth: .infinity) }.buttonStyle(CapsuleButtonStyle(primary: false))
                    Button { workspace.runKept(requestId) } label: { Text("Try Again").frame(maxWidth: .infinity) }.buttonStyle(CapsuleButtonStyle())
                }
            } else {
                Button { dismiss() } label: { Text("Hide").frame(maxWidth: .infinity) }.buttonStyle(CapsuleButtonStyle(primary: false))
            }
        }
        .padding(.horizontal, Space.xl)
        .padding(.bottom, Space.l)
        .frame(maxWidth: 560)
        .frame(maxWidth: .infinity)
        .screenBackground()
        .onAppear { workspace.watch(requestId, true) }
        .onDisappear { workspace.watch(requestId, false) }
        .onChange(of: workspace.creations[requestId]?.error != nil) { _, failed in
            if failed { Haptics.warning() }
        }
        .onChange(of: workspace.creations[requestId]?.agentId, initial: true) { _, agentId in
            guard let agentId else { return }
            Haptics.success()
            workspace.forget(creation: requestId)
            dismiss()
            openURL(DeepLink.agent(mac: mac, agentId: agentId).url)
        }
    }

    private func dismissFailed() {
        workspace.forget(creation: requestId)
        dismiss()
    }

    /// "anywhere · Worktree": what's being started, and where.
    private var place: String? {
        guard let plan = workspace.creations[requestId]?.plan else { return nil }
        let name = workspace.project(plan.spec.project)?.name
        return [name, plan.worktree != nil ? "Worktree" : nil].compactMap(\.self).joined(separator: " · ")
    }
}

/// A launch's steps: done ones ticked, the running one turning, and those still to come.
@MainActor
private struct LaunchSteps: View {
    let creation: Creation?

    var body: some View {
        let steps = creation?.steps ?? []
        let stopped = creation?.done == true || creation?.error != nil || creation?.waiting == true
        VStack(alignment: .leading, spacing: Space.m) {
            if steps.isEmpty && creation?.error == nil && creation?.waiting != true {
                row("Asking your Mac", .running)
            }
            ForEach(Array(steps.enumerated()), id: \.offset) { i, step in
                VStack(alignment: .leading, spacing: Space.xxs) {
                    row(SessionText.step(step.step), i < steps.count - 1 || creation?.done == true ? .done : creation?.error != nil ? .failed : stopped ? .done : .running)
                    if let note = step.note { Text(note).font(Typography.footnote).foregroundStyle(Palette.text3.color).padding(.leading, 28) }
                }
            }
            if let creation, creation.waiting { row(SessionText.creating(creation), .running) }
            ForEach(SessionText.stepsLeft(after: steps.last?.step), id: \.self) { row(SessionText.step($0), .later) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private enum Mark { case done, running, failed, later }

    private func row(_ text: String, _ mark: Mark) -> some View {
        HStack(spacing: Space.m) {
            Group {
                switch mark {
                case .done: Image(systemName: "checkmark").font(.system(size: 13, weight: .bold)).foregroundStyle(Palette.text3.color)
                case .running: ProgressView().controlSize(.small)
                case .failed: StatusGlyph(.failed)
                case .later: Circle().strokeBorder(Palette.text5.color, lineWidth: 1.5).frame(width: 12, height: 12)
                }
            }
            .frame(width: 16)
            Text(text).font(Typography.body).foregroundStyle((mark == .later ? Palette.text4 : Palette.text).color)
        }
    }
}
