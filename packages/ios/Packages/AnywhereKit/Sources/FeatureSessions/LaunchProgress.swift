import DesignSystem
import Domain
import Stores
import SwiftUI

/// A launch in its sheet, as the desktop's creating page. Hide, or a swipe down, leaves it running as a row under its
/// project; if it starts while shown, its chat opens.
@MainActor
struct LaunchProgress: View {
    let host: HostModel
    let mac: String
    let requestId: String
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    var shown: Creation?
    /// The launch as the sheet last showed it, kept while the sheet closes once the store has let it go.
    @State private var started: Creation?

    private var workspace: WorkspaceStore { host.workspace }

    var body: some View {
        let creation = shown ?? workspace.creations[requestId] ?? started
        let rows = LaunchRow.rows(creation)
        let error = creation?.error
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                LucideIcon(.monitorSmartphone, size: 32)
                    .foregroundStyle(Palette.onText.color)
                    .frame(width: 64, height: 64)
                    .background(Palette.text.color, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                    .padding(.top, 31)
                Text(error == nil ? "Starting session" : "Couldn't start the session")
                    .font(.system(size: 22, weight: .bold)).foregroundStyle(Palette.text.color)
                    .padding(.top, Space.l)
                if let subtitle = Self.subtitle(creation, project: creation?.plan.flatMap { workspace.project($0.spec.project)?.name }) {
                    Text(subtitle).font(.system(size: 15)).foregroundStyle(Palette.text2.color).lineLimit(1).padding(.top, 2)
                }
                if !rows.isEmpty {
                    VStack(alignment: .leading, spacing: 14) {
                        ForEach(Array(rows.enumerated()), id: \.offset) { _, row in LaunchStepRow(row: row) }
                    }
                    .padding(.top, Space.xl - 4)
                    ProgressBar(fraction: LaunchRow.progress(creation), failed: error != nil).padding(.top, 22)
                }
                if let error { LaunchError(message: error.message, detail: error.detail).padding(.top, Space.l) }
            }
            .padding(.horizontal, 24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollBounceBehavior(.basedOnSize)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            Group {
                if error != nil {
                    HStack(spacing: Space.s) {
                        Button { dismissFailed() } label: { Text("Dismiss").frame(maxWidth: .infinity) }.buttonStyle(PillButtonStyle(.secondary))
                        Button { workspace.runKept(requestId) } label: { Text("Try Again").frame(maxWidth: .infinity) }.buttonStyle(PillButtonStyle(.primary))
                    }
                } else {
                    Button { dismiss() } label: { Text("Hide").frame(maxWidth: .infinity) }.buttonStyle(PillButtonStyle(.secondary))
                }
            }
            .padding(.horizontal, 24)
            .padding(.vertical, Space.m)
            .background(Palette.surface.color)
        }
        .frame(maxWidth: 560)
        .frame(maxWidth: .infinity)
        .background(Palette.surface.color.ignoresSafeArea())
        .animation(.smooth, value: rows)
        #if os(iOS)
            .presentationDetents([.height(520), .large])
            .toolbar(.hidden, for: .navigationBar)
        #endif
        .onAppear { workspace.watch(requestId, true) }
        .onDisappear { workspace.watch(requestId, false) }
        .onChange(of: workspace.creations[requestId]?.error != nil) { _, failed in
            if failed { Haptics.warning() }
        }
        .onChange(of: workspace.creations[requestId]?.agentId, initial: true) { _, agentId in
            guard let agentId else { return }
            Haptics.success()
            started = workspace.creations[requestId]
            workspace.forget(creation: requestId)
            dismiss()
            openURL(DeepLink.agent(mac: mac, agentId: agentId).url)
        }
    }

    /// What is being started: the prompt's first line, else where it starts.
    nonisolated static func subtitle(_ creation: Creation?, project: String?) -> String? {
        guard let creation else { return nil }
        if !creation.title.isEmpty { return creation.title }
        return creation.plan.map { SessionText.launchWhere($0, project: project) }
    }

    private func dismissFailed() {
        started = workspace.creations[requestId]
        workspace.forget(creation: requestId)
        dismiss()
    }
}

/// A line of a launch's steps: what it says, a note pocketd sent with it, and where it stands.
struct LaunchRow: Hashable {
    enum Mark: Hashable { case done, running, failed, later }

    let label: String
    var note: String?
    let mark: Mark
    var took: TimeInterval?

    /// Done ones, the running one, and those still to come in pocketd's order.
    nonisolated static func rows(_ creation: Creation?) -> [LaunchRow] {
        let steps = creation?.steps ?? []
        let failed = creation?.error != nil
        let waiting = creation?.waiting == true
        var rows: [LaunchRow] = []
        if steps.isEmpty && !failed && !waiting { rows.append(LaunchRow(label: "Asking your Mac", mark: .running)) }
        for (i, step) in steps.enumerated() {
            let mark: Mark = i < steps.count - 1 || creation?.done == true ? .done : failed ? .failed : waiting ? .done : .running
            rows.append(LaunchRow(label: SessionText.step(step.step), note: step.note, mark: mark, took: creation?.took(i)))
        }
        if let creation, waiting { rows.append(LaunchRow(label: SessionText.creating(creation), mark: .running)) }
        return rows + SessionText.stepsLeft(after: steps.last?.step, order: order(creation)).map { LaunchRow(label: SessionText.step($0), mark: .later) }
    }

    /// The steps pocketd sends, as the desktop lists them (`Create::new`): a new worktree's, less copy or setup when off;
    /// only Preparing and Starting agent in an existing worktree or Local, or tried again in the worktree it made.
    nonisolated static func order(_ creation: Creation?) -> [String] {
        if creation?.agentOnly == true { return ["prepare", "agent"] }
        guard let plan = creation?.plan, plan.worktree == nil else { return SessionText.launchSteps }
        guard case .new(let new) = plan.spec.checkout else { return ["prepare", "agent"] }
        return SessionText.launchSteps.filter { !($0 == "copy" && new.copy == false || $0 == "setup" && new.setup == false) }
    }

    /// How far along it is, 0...1: the steps done and half the running one, of all it will take; nothing before the
    /// Mac reports its first.
    nonisolated static func progress(_ creation: Creation?) -> Double {
        guard let creation, !creation.steps.isEmpty else { return 0 }
        let rows = rows(creation)
        let done = Double(rows.count { $0.mark == .done || $0.mark == .failed })
        let running = Double(rows.count { $0.mark == .running })
        return min((done + running / 2) / Double(rows.count), 1)
    }
}

@MainActor
private struct ProgressBar: View {
    let fraction: Double
    let failed: Bool

    var body: some View {
        GeometryReader { g in
            Capsule().fill(Palette.fill3.color)
                .overlay(alignment: .leading) {
                    Capsule().fill((failed ? Palette.failed : Palette.text).color).frame(width: g.size.width * fraction)
                }
        }
        .frame(height: 4)
        .accessibilityElement()
        .accessibilityLabel("Progress")
        .accessibilityValue("\(Int(fraction * 100))%")
    }
}

@MainActor
private struct LaunchStepRow: View {
    let row: LaunchRow

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Space.m) {
            mark.frame(width: 20, height: 20).alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + 5 }
            VStack(alignment: .leading, spacing: 2) {
                Text(row.label).font(.system(size: 15)).foregroundStyle(color.color)
                if let note = row.note { Text(note).font(.system(size: 13)).foregroundStyle(Palette.text3.color) }
            }
            Spacer(minLength: Space.s)
            if let took = row.took { Text(TimeText.took(seconds: took)).font(.system(size: 13, design: .monospaced)).foregroundStyle(Palette.text4.color) }
        }
        .accessibilityElement(children: .combine)
    }

    private var color: Token {
        switch row.mark {
        case .later: Palette.text3
        case .failed: Palette.failedText
        default: Palette.text
        }
    }

    @ViewBuilder
    private var mark: some View {
        switch row.mark {
        case .done: LucideIcon(.circleCheck, size: 20, weight: 1.75).foregroundStyle(Palette.success.color)
        case .running: DotSpinner(size: 13)
        case .failed: LucideIcon(.circleX, size: 20, weight: 1.75).foregroundStyle(Palette.failed.color)
        case .later: Circle().strokeBorder(Palette.text4.color, lineWidth: 1.5).frame(width: 10, height: 10)
        }
    }
}

@MainActor
private struct LaunchError: View {
    let message: String
    let detail: String?

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            Text(message).font(Typography.subheadline).foregroundStyle(Palette.failedText.color)
            if let detail, !detail.isEmpty { Text(detail).font(Typography.codeSmall).foregroundStyle(Palette.text2.color) }
        }
        .padding(.horizontal, Space.m)
        .padding(.vertical, Space.m - 2)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.failedBackground.color, in: RoundedRectangle(cornerRadius: Radius.small + 2, style: .continuous))
    }
}
