import SwiftUI

struct ReplyField: View {
    let ask: Ask
    let send: (String) -> Void
    let back: () -> Void
    @State private var text = ""
    @FocusState private var focused: Bool

    var body: some View {
        HStack(spacing: 6) {
            TextField(ask.questions.isEmpty ? "Tell \(ask.name) what to do instead" : "Your answer", text: $text)
                .textFieldStyle(.plain)
                .font(.system(size: 13))
                .foregroundColor(.ink)
                .focused($focused)
                .onSubmit(submit)
            Button(action: submit) {
                Image(systemName: "arrow.up").font(.system(size: 12, weight: .semibold)).frame(width: 26, height: 26)
            }
            .buttonStyle(Pill(fill: .button, hover: .buttonHover, ink: .onButton, padding: 0))
        }
        .padding(.leading, 11)
        .padding(.trailing, 4)
        .frame(height: 34)
        .background(RoundedRectangle(cornerRadius: 9).fill(Color.fill))
        .onAppear { focused = true }
        .onExitCommand(perform: back)
    }

    private func submit() {
        let reply = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !reply.isEmpty {
            send(reply)
        }
    }
}
