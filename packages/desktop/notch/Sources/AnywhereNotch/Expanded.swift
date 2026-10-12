import SwiftUI

struct Expanded: View {
    @ObservedObject var controller: Controller
    let ask: Ask
    let replying: Bool

    private var subject: some View {
        Text(ask.shell ? "$ \(ask.subject)" : ask.subject)
            .font(.system(size: 12, design: .monospaced))
            .foregroundColor(.ink)
            .lineSpacing(3)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            head
            VStack(alignment: .leading, spacing: 5) {
                Text(ask.verb).font(.system(size: 11.5, weight: .medium)).foregroundColor(.waiting)
                // A long command scrolls rather than hiding its tail, as allowing it runs all of it.
                ViewThatFits(in: .vertical) {
                    subject
                    ScrollView { subject.frame(maxWidth: .infinity, alignment: .leading) }
                }
                .frame(maxHeight: 100)
                .padding(.horizontal, 10)
                .padding(.vertical, 7)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(RoundedRectangle(cornerRadius: 8).fill(Color.fill))
            }
            if replying {
                ReplyField(ask: ask, send: { controller.answer(.reply($0)) }, back: controller.back)
            } else {
                buttons
            }
        }
        .padding(.top, controller.notch.height + 6)
        .padding(.horizontal, 16)
        .padding(.bottom, 14)
        .frame(width: 480)
        .fixedSize(horizontal: false, vertical: true)
    }

    private var head: some View {
        HStack(spacing: 10) {
            ProviderIcon(ask: ask, size: 16)
                .frame(width: 28, height: 28)
                .background(RoundedRectangle(cornerRadius: 8).fill(Color.fill))
            VStack(alignment: .leading, spacing: 0) {
                Text(ask.title).font(.system(size: 13, weight: .semibold)).foregroundColor(.ink)
                Text(ask.place).font(.system(size: 11)).foregroundColor(.ink3)
            }
            .lineLimit(1)
            Spacer(minLength: 0)
            if controller.island.asks.count > 1 {
                Text("1 of \(controller.island.asks.count)").font(.system(size: 11, weight: .medium)).foregroundColor(.ink3).monospacedDigit()
            }
            Button { controller.open(ask) } label: {
                Image(systemName: "arrow.up.right.square").font(.system(size: 13)).frame(width: 26, height: 26)
            }
            .buttonStyle(Pill(fill: .clear, hover: .fill, ink: .ink2, padding: 0))
            .help("Open in Anywhere")
        }
        .frame(height: 30)
    }

    private var buttons: some View {
        HStack(spacing: 6) {
            if ask.feedback {
                Button("Reply instead…", action: controller.reply)
                    .buttonStyle(Pill(fill: .clear, hover: .clear, ink: .ink2, hoverInk: .ink, padding: 6))
            }
            Spacer(minLength: 0)
            Button("Deny and stop") { controller.answer(.deny) }.buttonStyle(Pill(fill: .push, hover: .pushHover, ink: .ink))
            Button("Allow") { controller.answer(.allow) }.buttonStyle(Pill(fill: .button, hover: .buttonHover, ink: .onButton))
        }
        .frame(height: 28)
    }
}
