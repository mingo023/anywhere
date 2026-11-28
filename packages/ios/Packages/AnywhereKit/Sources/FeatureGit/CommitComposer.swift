import DesignSystem
import DiffModel
import Stores
import SwiftUI

/// The bar under Changes: the message and Commit, with Amend and Commit & Push held on it. Its own view, so typing redraws only it.
@MainActor
struct CommitComposer: View {
    let store: GitStore
    let changes: TreeChanges
    @State private var message = ""
    @State private var amend = false
    @State private var note: String?
    @FocusState private var focused: Bool

    /// The desktop's `git.commit_all`, as pocketd reads it.
    private var commitAll: Bool { changes.head?.commitAll ?? true }

    var body: some View {
        let _ = BodyCount.hit("CommitComposer")
        let totals = changes.head?.totals ?? .init(files: 0, added: 0, removed: 0, staged: 0)
        let busy = changes.busy
        let label = Composer.label(totals: totals, busy: busy, commitAll: commitAll)
        let shape = RoundedRectangle(cornerRadius: Radius.composer, style: .continuous)
        VStack(alignment: .leading, spacing: Space.xs) {
            if let note {
                Text(note).font(Typography.caption).foregroundStyle(Palette.waitingText.color).padding(.horizontal, Space.m)
            }
            HStack(spacing: Space.s) {
                TextField(amend ? "Message (empty keeps the last)" : "Commit message", text: $message, axis: .vertical)
                    .lineLimit(1...6)
                    .textFieldStyle(.plain)
                    .focused($focused)
                    .font(Typography.body)
                    .padding(.leading, 10)
                Menu {
                    Toggle(isOn: $amend) { Label("Amend Last Commit", lucide: .pencil) }
                    if !amend { Button { commit(.commit) } label: { Label(label, lucide: .commit) } }
                } label: {
                    HStack(spacing: 2) {
                        Text(GitText.commitButton(label: label, amend: amend, busy: busy != nil))
                        LucideIcon(.chevronDown, size: 16)
                    }
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle((busy == nil ? Palette.onText : Palette.text3).color)
                    .lineLimit(1)
                    .padding(.leading, Space.l)
                    .padding(.trailing, Space.s)
                    .frame(height: 40)
                    .background((busy == nil ? Palette.text : Palette.fill3).color, in: Capsule())
                } primaryAction: {
                    commit(amend ? .amend : .push)
                }
                .fixedSize()
                .disabled(busy != nil)
            }
            .padding(.vertical, Space.s)
            .padding(.leading, 10)
            .padding(.trailing, Space.s)
            .background(Palette.surface.color, in: shape)
            .overlay(shape.strokeBorder(Palette.separator.color, lineWidth: 0.5))
        }
        .padding(.horizontal, Space.s)
        .padding(.bottom, Space.xs)
    }

    private func commit(_ kind: CommitKind) {
        let totals = changes.head?.totals ?? .init(files: 0, added: 0, removed: 0, staged: 0)
        switch Composer.step(totals: totals, kind: kind, message: message, busy: changes.busy != nil, commitAll: commitAll) {
        case .skip:
            return
        case .askMessage:
            focused = true
        case .stageFirst:
            note = "Stage the changes to commit first"
        case .run(let all, _):
            note = nil
            focused = false
            let text = message
            let tree = changes.tree
            Haptics.tap()
            Task {
                // The message goes once the commit lands, so a failed push leaves Push N as the retry, not a second commit.
                guard await store.commit(tree, message: text, kind: kind == .amend ? .amend : .commit, all: all) else { return Haptics.warning() }
                message = ""
                amend = false
                if kind == .push, !(await store.push(tree)) { return Haptics.warning() }
                Haptics.success()
            }
        }
    }
}
