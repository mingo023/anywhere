import ChatModel
import DesignSystem
import SwiftUI

/// The tool waiting on permission, in place of the composer: Deny, Reply with feedback, or Allow once. A phone that
/// may only watch sees the ask without the buttons.
@MainActor
struct PermissionCard: View {
    let ask: Ask
    let who: String
    let feedback: Bool
    let answerable: Bool
    let allow: () -> Void
    let deny: (String?) -> Void
    @State private var replying = false
    @State private var reply = ""
    @FocusState private var focused: Bool

    var body: some View {
        let (glyph, verb, what) = permissionSubject(ask.tool, ask.toolName)
        VStack(alignment: .leading, spacing: Space.m) {
            HStack(spacing: 10) {
                Circle().fill(Palette.waitingDot.color).frame(width: 8, height: 8)
                    .padding(3).overlay(Circle().strokeBorder(Palette.waitingBackground.color, lineWidth: 3))
                VStack(alignment: .leading, spacing: 1) {
                    Text("Allow \(ask.toolName)?").font(ChatFont.body.weight(.semibold)).foregroundStyle(Palette.text.color)
                    Text("\(who) is paused until you answer").font(ChatFont.caption).foregroundStyle(Palette.text3.color)
                }
            }
            HStack(alignment: .top, spacing: 8) {
                Image(systemName: glyph.symbol).font(.system(size: 12)).foregroundStyle(Palette.text3.color).frame(width: 16)
                ScrollView {
                    Text("\(Text(verb).foregroundStyle(Palette.text3.color)) \(Text(what))")
                        .font(ChatFont.mono).foregroundStyle(Palette.text.color)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .textSelection(.enabled)
                }
                .frame(maxHeight: 120)
                .fixedSize(horizontal: false, vertical: true)
            }
            .padding(Space.s)
            .background(Palette.well.color, in: RoundedRectangle(cornerRadius: Radius.small))
            if answerable {
                if replying {
                    TextField("Tell the agent what to do instead", text: $reply, axis: .vertical)
                        .lineLimit(1...6)
                        .font(ChatFont.body)
                        .focused($focused)
                        .textFieldStyle(.plain)
                        .padding(Space.s)
                        .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: Radius.small))
                        .onAppear { focused = true }
                }
                HStack(spacing: Space.s) {
                    button("Deny", primary: false) {
                        Haptics.warning()
                        deny(nil)
                    }
                    .keyboardShortcut(.escape, modifiers: [])
                    if feedback {
                        if replying {
                            button("Send reply", primary: false) { deny(reply) }.disabled(reply.allSatisfy(\.isWhitespace))
                        } else {
                            button("Reply", primary: false) { replying = true }
                        }
                    }
                    Spacer(minLength: 0)
                    button("Allow once", primary: true) {
                        Haptics.success()
                        allow()
                    }
                    .keyboardShortcut(.return, modifiers: [])
                }
            }
        }
        .padding(Space.m)
        .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Palette.waitingBackground.color, lineWidth: 1))
        .transition(.opacity)
    }

    private func button(_ title: String, primary: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(ChatFont.small.weight(.semibold))
                .foregroundStyle((primary ? Palette.onText : Palette.text).color)
                .padding(.horizontal, 14)
                .frame(minHeight: 36)
                .background((primary ? Palette.text : Palette.fill3).color, in: Capsule())
        }
        .buttonStyle(.plain)
    }
}
