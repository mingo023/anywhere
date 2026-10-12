import SwiftUI

/// AskUserQuestion's questions, one at a time; picking a single-select option answers it and moves on.
struct Questions: View {
    @ObservedObject var controller: Controller
    let ask: Ask
    let picks: Picks

    var body: some View {
        if let question = picks.question {
            VStack(alignment: .leading, spacing: 10) {
                HStack {
                    Text(question.header).font(.system(size: 11.5, weight: .medium)).foregroundColor(.waiting)
                    Spacer(minLength: 0)
                    if picks.questions.count > 1 {
                        Text("Question \(picks.page + 1) of \(picks.questions.count)")
                            .font(.system(size: 11, weight: .medium)).foregroundColor(.ink3).monospacedDigit()
                    }
                }
                // A long question or many options scroll rather than pushing the buttons off the island.
                ViewThatFits(in: .vertical) {
                    content(question)
                    ScrollView { content(question) }
                }
                .frame(maxHeight: 320)
                footer(question)
            }
        }
    }

    private func content(_ question: Question) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(question.question)
                .font(.system(size: 13, weight: .semibold))
                .foregroundColor(.ink)
                .fixedSize(horizontal: false, vertical: true)
            options(question)
        }
    }

    private func options(_ question: Question) -> some View {
        VStack(spacing: 4) {
            ForEach(Array(question.options.enumerated()), id: \.offset) { _, choice in
                let chosen = picks.isChosen(choice.label)
                Button { controller.pick(choice.label) } label: {
                    HStack(alignment: .firstTextBaseline, spacing: 9) {
                        Image(systemName: question.multiSelect ? (chosen ? "checkmark.square.fill" : "square") : (chosen ? "largecircle.fill.circle" : "circle"))
                            .font(.system(size: 12))
                            .foregroundColor(chosen ? .ink : .ink3)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(choice.label).font(.system(size: 13, weight: .medium)).foregroundColor(.ink)
                            if !choice.description.isEmpty {
                                Text(choice.description).font(.system(size: 11.5)).foregroundColor(.ink2)
                            }
                        }
                        .fixedSize(horizontal: false, vertical: true)
                        Spacer(minLength: 0)
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 7)
                }
                .buttonStyle(Row(chosen: chosen))
            }
        }
    }

    private func footer(_ question: Question) -> some View {
        HStack(spacing: 6) {
            if ask.feedback {
                Button("Reply instead…", action: controller.reply)
                    .buttonStyle(Pill(fill: .clear, hover: .clear, ink: .ink2, hoverInk: .ink, padding: 6))
            }
            Spacer(minLength: 0)
            if picks.page > 0 {
                Button("Back", action: controller.previousQuestion).buttonStyle(Pill(fill: .push, hover: .pushHover, ink: .ink))
            }
            if question.multiSelect {
                Button(picks.isLast ? "Submit" : "Next", action: controller.nextQuestion)
                    .buttonStyle(Pill(fill: .button, hover: .buttonHover, ink: .onButton))
                    .disabled(!picks.isAnswered)
                    .opacity(picks.isAnswered ? 1 : 0.4)
            }
        }
        .frame(height: 28)
    }
}

private struct Row: ButtonStyle {
    let chosen: Bool

    func makeBody(configuration: Configuration) -> some View {
        Pressable(configuration: configuration, chosen: chosen)
    }

    private struct Pressable: View {
        let configuration: Configuration
        let chosen: Bool
        @State private var hovered = false

        var body: some View {
            configuration.label
                .background(RoundedRectangle(cornerRadius: 8).fill(chosen || hovered ? Color.push : Color.fill))
                .contentShape(Rectangle())
                .opacity(configuration.isPressed ? 0.85 : 1)
                .onHover { hovered = $0 }
        }
    }
}
