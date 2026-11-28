import ChatModel
import DesignSystem
import MarkdownView
import SwiftUI

/// What a row asks of the chat when tapped.
@MainActor
struct RowActions {
    var toggle: (_ key: String, _ fallback: Bool) -> Void = { _, _ in }
    var retry: (_ pending: Int) -> Void = { _ in }
    var sendNow: () -> Void = {}
    var retryTurn: (_ turn: Int) -> Void = { _ in }
    /// Scrolls to the phase holding a call, opens it and flashes it.
    var showCall: (_ id: String) -> Void = { _ in }
    var openLink: (MarkdownLink) -> Void = { _ in }
    /// Opens the screen of the sub-agent a call spawned.
    var openAgent: (_ callId: String) -> Void = { _ in }
    /// Where a streaming message's reveal is, read by the text it lets out alone.
    var reveal: (_ message: String) -> Reveal? = { _ in nil }
}

/// One transcript row, redrawn only when what it shows changes.
@MainActor
struct RowView: View, Equatable {
    let item: RowItem
    let flash: Bool
    let actions: RowActions

    nonisolated static func == (a: RowView, b: RowView) -> Bool {
        a.item == b.item && a.flash == b.flash
    }

    var body: some View {
        let _ = BodyCount.hit("RowView")
        if case .hidden = item.content {
            Color.clear.frame(height: 0)
        } else {
            content
                .frame(maxWidth: .infinity, alignment: .leading)
                .flash(flash)
                .padding(.top, item.gap)
        }
    }

    @ViewBuilder
    private var content: some View {
        switch item.content {
        case .hidden: EmptyView()
        case .prompt(let p): PromptBubble(text: p.text, images: p.images, at: p.at)
        case .step(let step): StepView(step: step, actions: actions)
        case .notice(let label, let call): NoticeRow(label: label) { if let call { actions.showCall(call) } }
        case .note(let text, let error): NoteRow(text: text, error: error)
        case .tasks(let todos): TasksCard(todos: todos)
        case .text(let t): TextRow(text: t, reveal: actions.reveal, onLink: actions.openLink)
        case .question(let value): QuestionMarker(value: value)
        case .compacted(let auto): CompactedRow(auto: auto)
        case .end(let end): EndRow(end: end, actions: actions)
        case .live(let live): LiveRow(live: live)
        case .working: Text("Sending…").font(ChatFont.small).foregroundStyle(Palette.text3.color).shimmer().padding(.top, 6)
        case .pending(let p): PendingBubble(pending: p, actions: actions)
        }
    }
}

/// A ring with an arc turning once a second, redrawn at most 20 times a second.
@MainActor
struct RingSpinner: View {
    var size: CGFloat = 12
    var tint: Token = Palette.working

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 20)) { context in
            let turn = context.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1)
            ZStack {
                Circle().stroke(Palette.fill4.color, lineWidth: 1.5)
                Circle().trim(from: 0, to: 0.3).stroke(tint.color, style: StrokeStyle(lineWidth: 1.5, lineCap: .round))
                    .rotationEffect(.degrees(turn * 360))
            }
        }
        .frame(width: size, height: size)
    }
}
