import DesignSystem
import Domain
import Stores
import SwiftUI

/// A launch in its sheet: the mascot signalling while it waits, a bar for how far along it is, and its steps. Hide, or
/// a swipe down, leaves it running as a row under its project; if it starts while shown, its chat opens.
@MainActor
struct LaunchProgress: View {
    let host: HostModel
    let mac: String
    let requestId: String
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    /// The launch as it started its agent, kept while the sheet closes on it once the store has let it go.
    @State private var started: Creation?

    private var workspace: WorkspaceStore { host.workspace }

    var body: some View {
        let creation = workspace.creations[requestId] ?? started
        let rows = LaunchRow.rows(creation)
        let failed = creation?.error != nil
        GeometryReader { screen in
        ScrollView {
            VStack(spacing: Space.xl) {
                VStack(spacing: Space.m) {
                    Mascot(failed ? .stuck : creation?.agentId != nil ? .happy : .waiting, size: 112)
                    VStack(spacing: Space.xs) {
                        Text(creation?.error?.message ?? "Starting session").font(Typography.title).foregroundStyle(Palette.text.color)
                        if let place { Text(place).font(Typography.subheadline).foregroundStyle(Palette.text3.color) }
                    }
                    .multilineTextAlignment(.center)
                }
                if !rows.isEmpty { LaunchMeter(rows: rows, progress: LaunchRow.progress(creation), failed: failed) }
                if let detail = creation?.error?.detail {
                    Text(detail)
                        .font(Typography.codeSmall)
                        .foregroundStyle(Palette.text3.color)
                        .padding(Space.m)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
                }
                if !rows.isEmpty { LaunchSteps(rows: rows) }
            }
            .padding(.vertical, Space.l)
            // Centred while it fits, scrolling on a small phone or at a large text size.
            .frame(minHeight: screen.size.height)
        }
        .scrollBounceBehavior(.basedOnSize)
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            Group {
                if failed {
                    HStack(spacing: Space.s) {
                        Button { dismissFailed() } label: { Text("Dismiss").frame(maxWidth: .infinity) }.buttonStyle(CapsuleButtonStyle(primary: false))
                        Button { workspace.runKept(requestId) } label: { Text("Try Again").frame(maxWidth: .infinity) }.buttonStyle(CapsuleButtonStyle())
                    }
                } else {
                    Button { dismiss() } label: { Text("Hide").frame(maxWidth: .infinity) }.buttonStyle(CapsuleButtonStyle(primary: false))
                }
            }
            .padding(.top, Space.s)
        }
        .padding(.horizontal, Space.xl)
        .padding(.bottom, Space.l)
        .frame(maxWidth: 560)
        .frame(maxWidth: .infinity)
        .screenBackground()
        .animation(.smooth, value: rows)
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

/// A line of a launch's steps: what it says, a note pocketd sent with it, and where it stands.
struct LaunchRow: Hashable {
    enum Mark: Hashable { case done, running, failed, later }

    let label: String
    var note: String?
    let mark: Mark

    /// Done ones, the running one, and those still to come in pocketd's order.
    nonisolated static func rows(_ creation: Creation?) -> [LaunchRow] {
        let steps = creation?.steps ?? []
        let failed = creation?.error != nil
        let waiting = creation?.waiting == true
        var rows: [LaunchRow] = []
        if steps.isEmpty && !failed && !waiting { rows.append(LaunchRow(label: "Asking your Mac", mark: .running)) }
        for (i, step) in steps.enumerated() {
            let mark: Mark = i < steps.count - 1 || creation?.done == true ? .done : failed ? .failed : waiting ? .done : .running
            rows.append(LaunchRow(label: SessionText.step(step.step), note: step.note, mark: mark))
        }
        if let creation, waiting { rows.append(LaunchRow(label: SessionText.creating(creation), mark: .running)) }
        return rows + SessionText.stepsLeft(after: steps.last?.step, order: order(creation)).map { LaunchRow(label: SessionText.step($0), mark: .later) }
    }

    /// The steps pocketd sends, as the desktop lists them (`Create::new`): a new worktree's, less copy or setup when off;
    /// only Preparing and Starting agent in an existing worktree or Local.
    nonisolated static func order(_ creation: Creation?) -> [String] {
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

/// The launch's bar: the step under way, how many of all, and how far that is.
@MainActor
private struct LaunchMeter: View {
    let rows: [LaunchRow]
    let progress: Double
    let failed: Bool

    var body: some View {
        let at = rows.firstIndex { $0.mark == .running || $0.mark == .failed } ?? rows.lastIndex { $0.mark == .done }
        VStack(spacing: Space.s) {
            HStack {
                Text(at.map { rows[$0].label } ?? "").foregroundStyle(Palette.text2.color).lineLimit(1)
                Spacer(minLength: Space.s)
                if let at, rows.count > 1 { Text("\(at + 1) of \(rows.count)").font(Typography.digits).foregroundStyle(Palette.text4.color) }
            }
            .font(Typography.footnote)
            GeometryReader { g in
                ZStack(alignment: .leading) {
                    Capsule().fill(Palette.fill3.color)
                    Capsule().fill((failed ? Palette.failed : Palette.teal).color).frame(width: max(g.size.width * progress, progress > 0 ? 6 : 0))
                }
            }
            .frame(height: 6)
            .accessibilityElement()
            .accessibilityLabel("Progress")
            .accessibilityValue("\(Int(progress * 100))%")
        }
    }
}

/// A launch's steps down a rail: done ones ticked, the running one turning, and those still to come.
@MainActor
private struct LaunchSteps: View {
    let rows: [LaunchRow]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { i, row in
                HStack(alignment: .top, spacing: Space.m) {
                    VStack(spacing: Space.xxs) {
                        mark(row.mark).frame(width: 20, height: 20)
                        if i < rows.count - 1 {
                            Capsule().fill((row.mark == .done ? Palette.teal : Palette.separator).color).frame(width: 2).frame(maxHeight: .infinity)
                        }
                    }
                    VStack(alignment: .leading, spacing: Space.xxs) {
                        Text(row.label).font(row.mark == .running ? Typography.body.weight(.semibold) : Typography.body).foregroundStyle(tint(row.mark).color)
                        if let note = row.note { Text(note).font(Typography.footnote).foregroundStyle(Palette.text3.color) }
                    }
                    .padding(.top, 1)
                    .padding(.bottom, i < rows.count - 1 ? Space.m : 0)
                }
                .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(Space.l)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
    }

    private func tint(_ mark: LaunchRow.Mark) -> Token {
        switch mark {
        case .done: Palette.text2
        case .running: Palette.text
        case .failed: Palette.failedText
        case .later: Palette.text4
        }
    }

    @ViewBuilder
    private func mark(_ mark: LaunchRow.Mark) -> some View {
        switch mark {
        case .done:
            Image(systemName: "checkmark").font(.system(size: 10, weight: .heavy)).foregroundStyle(Palette.onText.color)
                .frame(width: 18, height: 18).background(Palette.teal.color, in: Circle())
        case .running: GradientSpinner(size: 14)
        case .failed: StatusGlyph(.failed)
        case .later: Circle().strokeBorder(Palette.text5.color, lineWidth: 1.5).frame(width: 14, height: 14)
        }
    }
}
