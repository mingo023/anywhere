import ChatModel
import DesignSystem
import SwiftUI

/// The tool waiting on permission, in place of the composer: Deny, Reply with feedback, or Allow once. A phone that
/// may only watch sees the ask without the buttons. Once an answer is `sent`, both wait with a spinner; offline, they
/// wait for the Mac.
@MainActor
struct PermissionCard: View {
    let ask: Ask
    let who: String
    let feedback: Bool
    let answerable: Bool
    let sent: Bool
    let online: Bool
    var focus: FocusState<ChatField?>.Binding
    let allow: () -> Void
    let deny: (String?) -> Void
    @State private var replying = false
    @State private var reply = ""

    var body: some View {
        let what = permissionSubject(ask.tool, ask.toolName).what
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 10) {
                Circle().fill(Palette.waitingDot.color).frame(width: 8, height: 8)
                    .background(Circle().fill(Palette.waitingBackground.color).frame(width: 16, height: 16))
                Text("Allow \(ask.toolName)?").font(.system(size: 17, weight: .semibold)).foregroundStyle(Palette.text.color)
                Spacer(minLength: 0)
                if sent { ProgressView().controlSize(.small) }
            }
            Text("\(who) is paused until you answer").font(.system(size: 13)).foregroundStyle(Palette.text2.color)
                .padding(.leading, 18).padding(.top, 4)
            ScrollView {
                Text(what)
                    .font(.system(size: 14, design: .monospaced)).lineSpacing(3).foregroundStyle(Palette.text.color)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
                    .padding(Space.m)
            }
            .frame(maxHeight: 120)
            .fixedSize(horizontal: false, vertical: true)
            .background(Palette.well.color, in: RoundedRectangle(cornerRadius: Radius.small))
            .padding(.top, 14)
            if answerable {
                if replying {
                    TextField("Tell the agent what to do instead", text: $reply, axis: .vertical)
                        .lineLimit(1...6)
                        .font(ChatFont.body)
                        .focused(focus, equals: .reply)
                        .textFieldStyle(.plain)
                        .padding(Space.s)
                        .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: Radius.small))
                        .padding(.top, Space.s)
                        .onAppear { focus.wrappedValue = .reply }
                }
                buttons
                    .padding(.top, 16)
                    .disabled(sent || !online)
                    .opacity(sent || !online ? 0.5 : 1)
            }
        }
        .padding(EdgeInsets(top: 18, leading: 16, bottom: 16, trailing: 16))
        .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).strokeBorder(Palette.separator.color, lineWidth: 0.5))
        .transition(.opacity)
    }

    /// Deny and Reply take a share each and Allow once 1.3, as the design sets them; without Reply, Deny and Allow once.
    nonisolated static func shares(feedback: Bool) -> [CGFloat] {
        feedback ? [1, 1, 1.3] : [1, 1.3]
    }

    private var buttons: some View {
        let weights = Self.shares(feedback: feedback)
        return GeometryReader { row in
            let unit = (row.size.width - Space.s * CGFloat(weights.count - 1)) / weights.reduce(0, +)
            HStack(spacing: Space.s) {
                Button(role: .destructive) {
                    Haptics.warning()
                    deny(nil)
                } label: {
                    Text("Deny").frame(maxWidth: .infinity)
                }
                .buttonStyle(AnswerStyle(.destructive))
                .keyboardShortcut(.escape, modifiers: [])
                .frame(width: unit * weights[0])
                if feedback {
                    if replying {
                        Button { deny(reply) } label: { Text("Send reply").frame(maxWidth: .infinity) }
                            .buttonStyle(AnswerStyle(.secondary))
                            .disabled(reply.allSatisfy(\.isWhitespace))
                            .frame(width: unit * weights[1])
                    } else {
                        Button { replying = true } label: { Text("Reply").frame(maxWidth: .infinity) }
                            .buttonStyle(AnswerStyle(.secondary))
                            .frame(width: unit * weights[1])
                    }
                }
                Button {
                    Haptics.success()
                    allow()
                } label: {
                    Text("Allow once").frame(maxWidth: .infinity)
                }
                .buttonStyle(AnswerStyle(.primary))
                // Return while typing a reply mustn't allow what the reply argues against.
                .keyboardShortcut(replying ? nil : KeyboardShortcut(.return, modifiers: []))
                .frame(width: unit * weights[weights.count - 1])
            }
        }
        .frame(height: 44)
    }
}

/// `PillButtonStyle` with room for its words on a narrow screen, where the buttons share the row.
private struct AnswerStyle: ButtonStyle {
    let kind: PillButtonStyle.Kind
    @Environment(\.isEnabled) private var isEnabled

    init(_ kind: PillButtonStyle.Kind) {
        self.kind = kind
    }

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 15, weight: .semibold))
            .lineLimit(1)
            .minimumScaleFactor(0.85)
            .foregroundStyle(foreground.color)
            .padding(.horizontal, Space.s)
            .frame(height: 44)
            .background(background.color, in: Capsule())
            .contentShape(Capsule())
            .opacity(configuration.isPressed ? 0.7 : 1)
    }

    private var foreground: Token {
        guard isEnabled else { return Palette.text4 }
        switch kind {
        case .primary: return Palette.onText
        case .secondary: return Palette.text
        case .destructive: return Palette.failedText
        }
    }

    private var background: Token {
        guard isEnabled else { return Palette.fill3 }
        switch kind {
        case .primary: return Palette.text
        case .secondary: return Palette.fill3
        case .destructive: return Palette.failedBackground
        }
    }
}
