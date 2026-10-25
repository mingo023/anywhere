import ChatModel
import DesignSystem
import SwiftUI

/// The turn's latest todo list: done struck through, the active one turning, the rest hollow.
@MainActor
struct TasksCard: View {
    let todos: [Todo]

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: "list.bullet").font(.system(size: 11))
                Text("Tasks").font(ChatFont.mono.weight(.medium))
                Spacer()
                Pill(todoProgress(todos), tone: todoCount(todos).done == todos.count ? .success : .neutral)
            }
            .foregroundStyle(Palette.text2.color)
            ForEach(Array(todos.enumerated()), id: \.offset) { _, todo in
                HStack(alignment: .firstTextBaseline, spacing: 8) {
                    mark(todo.status).frame(width: 14, height: 14).alignmentGuide(.firstTextBaseline) { $0[.bottom] - 2 }
                    Text(todo.text)
                        .strikethrough(todo.status == .done)
                        .foregroundStyle(color(todo.status).color)
                }
                .font(ChatFont.small)
            }
        }
        .padding(Space.m)
        .background(Palette.fill1.color, in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Palette.hairline.color))
    }

    @ViewBuilder
    private func mark(_ status: Todo.Status) -> some View {
        switch status {
        case .done:
            Image(systemName: "checkmark").font(.system(size: 8, weight: .bold)).foregroundStyle(Palette.successText.color)
                .frame(width: 14, height: 14).background(Palette.successBackground.color, in: Circle())
        case .active: RingSpinner(size: 12)
        case .pending: Circle().strokeBorder(Palette.text5.color, lineWidth: 1.2)
        }
    }

    private func color(_ status: Todo.Status) -> Token {
        switch status {
        case .done: Palette.text4
        case .active: Palette.text
        case .pending: Palette.text2
        }
    }
}
