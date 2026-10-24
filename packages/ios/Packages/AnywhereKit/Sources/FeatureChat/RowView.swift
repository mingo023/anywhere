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
}

/// One transcript row, redrawn only when what it shows changes.
@MainActor
struct RowView: View, Equatable {
    let item: RowItem
    let provider: String
    let flash: Bool
    let actions: RowActions

    nonisolated static func == (a: RowView, b: RowView) -> Bool {
        a.item == b.item && a.provider == b.provider && a.flash == b.flash
    }

    var body: some View {
        let _ = BodyCount.hit("RowView")
        if case .hidden = item.content {
            Color.clear.frame(height: 0)
        } else {
            content
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, item.rail == .none ? 0 : RailSpine.indent)
                .background(alignment: .topLeading) {
                    if item.rail != .none { RailSpine(last: item.rail == .last, gap: item.gap).stroke(Palette.rail.color, lineWidth: 1) }
                }
                .flash(flash)
                .padding(.top, item.gap)
        }
    }

    @ViewBuilder
    private var content: some View {
        switch item.content {
        case .hidden: EmptyView()
        case .prompt(let p): PromptBubble(text: p.text, images: p.images, at: p.at)
        case .fold(let label, let open, let settled):
            FoldRow(label: label, open: open, settled: settled, provider: provider) { actions.toggle(item.id, false) }
        case .phase(let p): PhaseRow(phase: p, provider: provider, actions: actions)
        case .notice(let label, let call): NoticeRow(label: label) { if let call { actions.showCall(call) } }
        case .note(let text, let error): NoteRow(text: text, error: error)
        case .tasks(let todos): TasksCard(todos: todos)
        case .text(let t): TextRow(text: t, onLink: actions.openLink)
        case .question(let value): QuestionMarker(value: value)
        case .compacted(let auto): CompactedRow(auto: auto)
        case .end(let end): EndRow(end: end, actions: actions)
        case .live(let live): LiveRow(live: live, provider: provider)
        case .working: Text("Sending…").font(ChatFont.small).foregroundStyle(Palette.text3.color).shimmer().padding(.top, 6)
        case .pending(let p): PendingBubble(pending: p, actions: actions)
        }
    }
}

/// The 1 pt spine at x = 7 a fold's rows hang from, with an 8 pt rounded elbow into each; the last row's spine stops there.
struct RailSpine: Shape {
    static let indent: CGFloat = 22
    static let x: CGFloat = 7
    static let elbow: CGFloat = 11
    let last: Bool
    let gap: CGFloat

    func path(in rect: CGRect) -> Path {
        var p = Path()
        p.move(to: CGPoint(x: Self.x, y: -gap))
        p.addLine(to: CGPoint(x: Self.x, y: Self.elbow - 8))
        p.addQuadCurve(to: CGPoint(x: Self.x + 8, y: Self.elbow), control: CGPoint(x: Self.x, y: Self.elbow))
        if !last {
            p.move(to: CGPoint(x: Self.x, y: Self.elbow - 8))
            p.addLine(to: CGPoint(x: Self.x, y: rect.maxY))
        }
        return p
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
