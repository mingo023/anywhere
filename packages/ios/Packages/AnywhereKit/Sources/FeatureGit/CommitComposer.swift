import DesignSystem
import DiffModel
import Stores
import SwiftUI

/// The message field with Amend, Commit and Commit & Push. Its own view, so typing redraws only it.
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
        VStack(alignment: .leading, spacing: Space.s) {
            TextField(amend ? "Message (empty keeps the last)" : "Message", text: $message, axis: .vertical)
                .lineLimit(1...6)
                .textFieldStyle(.plain)
                .focused($focused)
                .font(Typography.body)
            Toggle("Amend last commit", isOn: $amend).font(Typography.subheadline)
            if let note {
                Text(note).font(Typography.caption).foregroundStyle(Palette.waitingText.color)
            }
            HStack(spacing: Space.s) {
                Button {
                    commit(amend ? .amend : .commit)
                } label: {
                    Text(amend && busy == nil ? "Amend" : Composer.label(totals: totals, busy: busy, commitAll: commitAll))
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(!(amend || Composer.ready(totals: totals, message: message, busy: busy != nil, commitAll: commitAll)))
                if !amend {
                    Button {
                        commit(.push)
                    } label: {
                        Text("Commit & Push").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    .disabled(!Composer.ready(totals: totals, message: message, busy: busy != nil, commitAll: commitAll))
                }
            }
            .disabled(busy != nil)
        }
        .padding(.vertical, Space.xs)
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
