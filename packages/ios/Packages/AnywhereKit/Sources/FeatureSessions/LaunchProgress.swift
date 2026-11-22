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
    /// The launch as it started its agent, kept while the sheet closes on it once the store has let it go.
    @State private var started: Creation?

    private var workspace: WorkspaceStore { host.workspace }

    var body: some View {
        let creation = shown ?? workspace.creations[requestId] ?? started
        let rows = LaunchRow.rows(creation)
        let error = creation?.error
        GeometryReader { screen in
        ScrollView {
            VStack(spacing: Space.xl) {
                VStack(spacing: Space.l) {
                    LaunchStage(mood: error != nil ? .stuck : creation?.agentId != nil ? .happy : .waiting)
                    VStack(spacing: Space.xs) {
                        Text(error == nil ? "Starting session" : "Couldn't start the session").font(Typography.title).foregroundStyle(Palette.text.color)
                        if let plan = creation?.plan {
                            Label(SessionText.launchWhere(plan, project: workspace.project(plan.spec.project)?.name), systemImage: Self.isLocal(plan) ? "folder" : "arrow.triangle.branch")
                                .font(Typography.codeSmall)
                                .foregroundStyle(Palette.text3.color)
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }
                    }
                    .multilineTextAlignment(.center)
                }
                if !rows.isEmpty {
                    LaunchSteps(rows: rows, progress: LaunchRow.progress(creation), started: creation?.started, ended: creation?.ended)
                }
                if let error { LaunchError(message: error.message, detail: error.detail) }
            }
            .padding(.vertical, Space.l)
            // Centred while it fits, scrolling on a small phone or at a large text size.
            .frame(minHeight: screen.size.height)
        }
        .scrollBounceBehavior(.basedOnSize)
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            Group {
                if error != nil {
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

    private static func isLocal(_ plan: LaunchPlan) -> Bool {
        if case .local = plan.spec.checkout { true } else { false }
    }

    private func dismissFailed() {
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

/// The mascot on a lit disc: its body is near black, so on a dark screen it needs a light behind it to read.
@MainActor
private struct LaunchStage: View {
    let mood: Mascot.Mood

    private static let centre = Token(0xffffffff, 0x3a3a40ff)
    private static let edge = Token(0xf0f0f2ff, 0x161618ff)

    var body: some View {
        Mascot(mood, size: 92)
            .offset(y: -4)
            .frame(width: 144, height: 144)
            .background {
                Circle()
                    .fill(RadialGradient(colors: [Self.centre.color, Self.edge.color], center: UnitPoint(x: 0.5, y: 0.62), startRadius: 0, endRadius: 80))
                    .overlay(Circle().strokeBorder(Palette.hairline.color, lineWidth: 0.5))
            }
    }
}

@MainActor
private struct LaunchSteps: View {
    let rows: [LaunchRow]
    let progress: Double
    let started: Date?
    let ended: Date?

    var body: some View {
        let card = RoundedRectangle(cornerRadius: Radius.card, style: .continuous)
        let at = rows.firstIndex { $0.mark == .running || $0.mark == .failed } ?? rows.lastIndex { $0.mark == .done }
        VStack(spacing: 0) {
            GeometryReader { g in
                Rectangle().fill((ended != nil ? Palette.failed : Palette.success).color).frame(width: g.size.width * progress)
            }
            .frame(height: 3)
            .background(Palette.fill2.color)
            .accessibilityElement()
            .accessibilityLabel("Progress")
            .accessibilityValue("\(Int(progress * 100))%")
            HStack {
                if let at, rows.count > 1 { Text("Step \(at + 1) of \(rows.count)") }
                Spacer(minLength: Space.s)
                if let started {
                    TimelineView(.periodic(from: started, by: 1)) { context in
                        Text(TimeText.stopwatch(seconds: Int((ended ?? context.date).timeIntervalSince(started))))
                    }
                    .fontDesign(.monospaced)
                }
            }
            .font(Typography.digits)
            .foregroundStyle(Palette.text3.color)
            .padding(.horizontal, Space.l)
            .padding(.top, Space.m)
            .padding(.bottom, Space.xs)
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                LaunchStepRow(row: row)
            }
        }
        .padding(.bottom, Space.s)
        .background(Palette.surface.color)
        .clipShape(card)
        .overlay(card.strokeBorder(Palette.hairline.color, lineWidth: 0.5))
    }
}

@MainActor
private struct LaunchStepRow: View {
    let row: LaunchRow

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Space.m) {
            mark.frame(width: 16, height: 16).alignmentGuide(.firstTextBaseline) { $0[VerticalAlignment.center] + 5 }
            VStack(alignment: .leading, spacing: 2) {
                Text(row.label).font(Typography.subheadline).foregroundStyle((row.mark == .later ? Palette.text4 : row.mark == .failed ? Palette.failedText : Palette.text).color)
                if let note = row.note { Text(note).font(Typography.caption).foregroundStyle(Palette.text3.color) }
            }
            Spacer(minLength: Space.s)
            if let took = row.took { Text(TimeText.took(seconds: took)).font(Typography.caption).fontDesign(.monospaced).foregroundStyle(Palette.text4.color) }
        }
        .padding(.horizontal, Space.l)
        .padding(.vertical, 7)
    }

    @ViewBuilder
    private var mark: some View {
        switch row.mark {
        case .done: Image(systemName: "checkmark").font(.system(size: 12, weight: .bold)).foregroundStyle(Palette.success.color)
        case .running: GradientSpinner(size: 14)
        case .failed: Image(systemName: "xmark").font(.system(size: 11, weight: .bold)).foregroundStyle(Palette.failed.color)
        case .later: Circle().fill(Palette.text5.color).frame(width: 6, height: 6)
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
