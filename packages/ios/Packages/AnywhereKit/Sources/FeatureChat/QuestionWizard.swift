import ChatModel
import DesignSystem
import MarkdownView
import SwiftUI

/// The agent's questions in place of the composer, a page each: a single pick moves on after 220 ms, typed words win
/// over picks, and the last page submits every answer.
@MainActor
struct QuestionWizard: View {
    let wizard: Wizard
    let answerable: Bool
    let change: ((inout Wizard) -> Wizard.Step) -> Void
    let skip: () -> Void
    let openLink: (MarkdownLink) -> Void
    @State private var advancing: Task<Void, Never>?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        let q = wizard.question
        let (title, body) = titleAndBody(q.question)
        VStack(alignment: .leading, spacing: Space.m) {
            HStack(spacing: 8) {
                if !q.header.isEmpty {
                    Text(trackedUpper(q.header)).font(.system(size: 10.5, weight: .semibold)).foregroundStyle(Palette.text3.color)
                }
                Spacer(minLength: 0)
                if wizard.questions.count > 1 {
                    HStack(spacing: 3) {
                        ForEach(wizard.questions.indices, id: \.self) { i in
                            Capsule().fill((i <= wizard.page ? Palette.text2 : Palette.fill4).color).frame(width: 14, height: 3)
                        }
                    }
                    Text("\(wizard.page + 1) of \(wizard.questions.count)").font(ChatFont.caption).monospacedDigit().foregroundStyle(Palette.text3.color)
                }
            }
            VStack(alignment: .leading, spacing: Space.s) {
                Text(title).font(.system(size: 15, weight: .semibold)).foregroundStyle(Palette.text.color)
                if let body {
                    ScrollView { MarkdownView(body, style: .compact, onLink: openLink) }.frame(maxHeight: 184).fixedSize(horizontal: false, vertical: true)
                }
                if q.multi { Text("Select one or more.").font(ChatFont.caption).foregroundStyle(Palette.text3.color) }
                let typed = !wizard.currentTyped.allSatisfy(\.isWhitespace)
                VStack(spacing: 6) {
                    ForEach(Array(q.options.enumerated()), id: \.offset) { i, option in
                        OptionCard(n: i + 1, choice: option, multi: q.multi, picked: wizard.isPicked(i), keys: !typed) { pick(i) }
                            .opacity(typed ? 0.55 : 1)
                            .disabled(!answerable)
                    }
                }
                HStack(alignment: .top, spacing: 8) {
                    Image(systemName: "pencil").font(.system(size: 12)).foregroundStyle(Palette.text3.color).padding(.top, 3)
                    TextField(wizard.currentPlaceholder, text: Binding(get: { wizard.currentTyped }, set: { text in change { w in w.setTyped(text); return .stay } }), axis: .vertical)
                        .lineLimit(1...4)
                        .textFieldStyle(.plain)
                        .font(ChatFont.small)
                        .disabled(!answerable)
                }
                .padding(Space.s)
                .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: 11))
            }
            .id(wizard.page)
            .transition(reduceMotion ? .opacity : .asymmetric(insertion: .offset(x: wizard.forward ? 14 : -14).combined(with: .opacity), removal: .opacity))
            if answerable {
                HStack(spacing: Space.s) {
                    Button("Skip") {
                        Haptics.warning()
                        skip()
                    }
                    .buttonStyle(.plain).font(ChatFont.small).foregroundStyle(Palette.text3.color).frame(minHeight: 36)
                    Spacer(minLength: 0)
                    Button("Back") { back() }.buttonStyle(.plain).font(ChatFont.small.weight(.medium)).frame(minHeight: 36).padding(.horizontal, 10)
                        .keyboardShortcut(.escape, modifiers: [])
                    let canGo = wizard.canAdvance(typedEmpty: wizard.currentTyped.allSatisfy(\.isWhitespace))
                    Button(wizard.primary) { advance() }
                        .buttonStyle(.plain)
                        .font(ChatFont.small.weight(.semibold))
                        .foregroundStyle(Palette.onText.color)
                        .padding(.horizontal, 14)
                        .frame(minHeight: 36)
                        .background(Palette.text.color, in: Capsule())
                        .opacity(canGo ? 1 : 0.35)
                        .disabled(!canGo)
                }
            }
        }
        .padding(Space.m)
        .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Palette.hairline.color))
        .animation(reduceMotion ? .easeOut(duration: 0.15) : .easeOut(duration: Wizard.autoAdvance), value: wizard.page)
        .onDisappear { advancing?.cancel() }
    }

    private func pick(_ option: Int) {
        Haptics.tap()
        var step = Wizard.Step.stay
        change { w in
            step = w.select(option)
            return .stay
        }
        guard step == .autoAdvance else { return }
        advancing?.cancel()
        advancing = Task { @MainActor in
            try? await Task.sleep(for: .seconds(Wizard.autoAdvance))
            guard !Task.isCancelled else { return }
            advance()
        }
    }

    private func advance() {
        advancing?.cancel()
        change { $0.advance() }
    }

    private func back() {
        var went = false
        change { w in
            went = w.back()
            return .stay
        }
        if !went { skip() }
    }
}

/// One option: its mark, label and description, and its number.
@MainActor
struct OptionCard: View {
    let n: Int
    let choice: Choice
    let multi: Bool
    let picked: Bool
    /// Number keys pick only while nothing is typed, so they don't steal the free text's digits.
    let keys: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(alignment: .top, spacing: 10) {
                Group {
                    if multi {
                        RoundedRectangle(cornerRadius: 5).strokeBorder((picked ? Palette.text : Palette.text5).color, lineWidth: 1.5)
                            .overlay { if picked { Image(systemName: "checkmark").font(.system(size: 9, weight: .bold)) } }
                    } else {
                        Circle().strokeBorder((picked ? Palette.text : Palette.text5).color, lineWidth: picked ? 5 : 1.5)
                    }
                }
                .frame(width: 16, height: 16)
                .padding(.top, 1)
                VStack(alignment: .leading, spacing: 2) {
                    Text(choice.label).font(.system(size: 13.5, weight: .medium)).foregroundStyle(Palette.text.color)
                    if !choice.description.isEmpty { Text(choice.description).font(ChatFont.caption).foregroundStyle(Palette.text3.color) }
                }
                Spacer(minLength: 0)
                if n <= 9 {
                    Text("\(n)").font(.system(size: 10.5, weight: .semibold)).monospacedDigit().foregroundStyle(Palette.text3.color)
                        .frame(width: 18, height: 18).background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: 5))
                }
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background((picked ? Palette.fill3 : Palette.fill1).color, in: RoundedRectangle(cornerRadius: 11))
            .overlay(RoundedRectangle(cornerRadius: 11).strokeBorder((picked ? Palette.separatorStrong : Palette.hairline).color))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .modifier(NumberKey(n: keys ? n : 0, action: action))
    }
}

/// Number keys pick on a hardware keyboard.
private struct NumberKey: ViewModifier {
    let n: Int
    let action: () -> Void

    func body(content: Content) -> some View {
        if (1...9).contains(n), let key = "\(n)".first {
            content.keyboardShortcut(KeyEquivalent(key), modifiers: [])
        } else {
            content
        }
    }
}
