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

    /// The desktop's `git.commit_all` default: with nothing staged, a commit takes every change.
    static let commitAll = true

    var body: some View {
        let _ = BodyCount.hit("CommitComposer")
        let totals = changes.head?.totals ?? .init(files: 0, added: 0, removed: 0, staged: 0)
        let busy = changes.busy
        VStack(alignment: .leading, spacing: Space.s) {
            TextField(amend ? "Message (empty keeps the last)" : "Message", text: $message, axis: .vertical)
                .lineLimit(1...6)
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
                    Text(amend && busy == nil ? "Amend" : Composer.label(totals: totals, busy: busy, commitAll: Self.commitAll))
                        .frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                .disabled(!(amend || Composer.ready(totals: totals, message: message, busy: busy != nil, commitAll: Self.commitAll)))
                if !amend {
                    Button {
                        commit(.push)
                    } label: {
                        Text("Commit & Push").frame(maxWidth: .infinity)
                    }
                    .buttonStyle(.bordered)
                    .disabled(!Composer.ready(totals: totals, message: message, busy: busy != nil, commitAll: Self.commitAll))
                }
            }
            .disabled(busy != nil)
        }
        .padding(.vertical, Space.xs)
    }

    private func commit(_ kind: CommitKind) {
        let totals = changes.head?.totals ?? .init(files: 0, added: 0, removed: 0, staged: 0)
        switch Composer.step(totals: totals, kind: kind, message: message, busy: changes.busy != nil, commitAll: Self.commitAll) {
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
            Haptics.tap()
            Task {
                if await store.commit(changes.tree, message: text, kind: kind, all: all) {
                    message = ""
                    amend = false
                    Haptics.success()
                } else {
                    Haptics.warning()
                }
            }
        }
    }
}
