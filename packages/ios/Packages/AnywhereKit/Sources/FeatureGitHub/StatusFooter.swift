import ChatModel
import DesignSystem
import Stores
import SwiftUI
import Wire

/// The end of the Status block: where the merge stands, and the one thing to do about it: send what's wrong to the
/// agent, resolve the conflict, mark ready, merge, or, once merged, delete the worktree or keep it.
@MainActor
struct StatusFooter: View {
    /// The summary and the merge sit at the end of the Status group; handing work to the agent is pinned over the
    /// screen's bottom edge.
    enum Part {
        case summary, actions
    }

    struct SendAll {
        let title: String
        /// Nil when this phone can't send.
        let send: (() -> Void)?
    }

    struct Resolve {
        let title: String
        /// What the merge says while the agent resolves the conflict.
        let working: String?
        let run: () -> Void
    }

    let store: GitHubStore
    let model: TreePullRequest
    let pr: PullRequest
    let mac: PrMac
    let cleanup: WorktreeCleanup?
    @Binding var deleteAfter: Bool
    let fix: FixContext
    let sendAll: SendAll?
    let resolve: Resolve?
    let keep: () -> Void
    let delete: () -> Void
    let merged: () -> Void
    let part: Part

    var body: some View {
        switch part {
        case .summary:
            VStack(alignment: .leading, spacing: 14) {
                summaryRow
                actions
            }
            .padding(.top, 14)
            .padding(.bottom, Inset.margin)
            .padding(.horizontal, Inset.margin)
            .frame(maxWidth: .infinity, alignment: .leading)
        case .actions:
            VStack(alignment: .leading, spacing: 14) { agentActions }
                .padding(.horizontal, Inset.margin)
                .padding(.vertical, Space.s)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(Palette.background.color)
        }
    }

    private var summaryRow: some View {
        let summary = PullRequestText.summary(pr)
        return HStack(alignment: .top, spacing: Space.m) {
            LucideIcon(summary.icon, size: 20).foregroundStyle(summary.tone.icon.color)
            VStack(alignment: .leading, spacing: 2) {
                Text(summary.title).font(.system(size: 15, weight: .semibold)).foregroundStyle(Palette.text.color)
                detail(summary).font(.system(size: 13)).lineSpacing(2).foregroundStyle(Palette.text2.color)
            }
        }
        .accessibilityElement(children: .combine)
    }

    /// The agent's work on a conflict, or the worktree that's left once merged, else what the summary says.
    private func detail(_ summary: PrSummary) -> Text {
        if let working = resolve?.working { return Text(working) }
        if pr.state == "MERGED", let cleanup, !model.kept {
            return Text("The ") + Text(cleanup.name).font(.system(size: 12, design: .monospaced)) + Text(" worktree is still on \(mac.name).")
        }
        return Text(summary.detail)
    }

    @ViewBuilder
    private var actions: some View {
        switch pr.state {
        case "OPEN": openActions
        case "MERGED": mergedActions
        default: EmptyView()
        }
    }

    @ViewBuilder
    private var openActions: some View {
        if pr.draft {
            Button {
                Haptics.tap()
                Task { await store.markReady(model.tree) }
            } label: {
                primary(model.busy ?? "Ready for review")
            }
            .buttonStyle(PillButtonStyle(.primary))
            .disabled(model.busy != nil || mac.writesWait)
        } else if pr.gate == .ready {
            MergeControls(store: store, model: model, pr: pr, offline: mac.writesWait, cleanup: cleanup, deleteAfter: $deleteAfter, merged: merged)
        }
    }

    @ViewBuilder
    private var agentActions: some View {
        if pr.state == "OPEN", !pr.draft {
            if let sendAll {
                if let send = sendAll.send {
                    Button {
                        Haptics.tap()
                        send()
                    } label: {
                        primary(sendAll.title)
                    }
                    .buttonStyle(PillButtonStyle(.primary))
                    .disabled(mac.writesWait)
                } else if !fix.canDrive {
                    Text(Composer.viewOnly(mac: mac.name)).font(.system(size: 13)).foregroundStyle(Palette.text3.color)
                }
            }
            if let resolve {
                Button {
                    Haptics.tap()
                    resolve.run()
                } label: {
                    primary(resolve.title)
                }
                .buttonStyle(PillButtonStyle(.primary))
                .disabled(resolve.working != nil || mac.writesWait)
            }
        }
    }

    @ViewBuilder
    private var mergedActions: some View {
        if cleanup != nil, !model.kept {
            HStack(spacing: Space.s) {
                Button { delete() } label: { Text("Delete worktree").frame(maxWidth: .infinity) }
                    .buttonStyle(PillButtonStyle(.destructive))
                    .disabled(mac.writesWait)
                Button { keep() } label: { Text("Keep").frame(maxWidth: .infinity) }
                    .buttonStyle(PillButtonStyle(.secondary))
            }
        }
    }

    private func primary(_ title: String) -> some View {
        Text(title).font(.system(size: 17, weight: .semibold)).frame(maxWidth: .infinity)
    }
}

/// The method to merge with and Merge, and whether the worktree goes once merged.
@MainActor
private struct MergeControls: View {
    let store: GitHubStore
    let model: TreePullRequest
    let pr: PullRequest
    let offline: Bool
    let cleanup: WorktreeCleanup?
    @Binding var deleteAfter: Bool
    let merged: () -> Void

    @AppStorage private var method: MergeMethod
    @State private var merging = false

    init(store: GitHubStore, model: TreePullRequest, pr: PullRequest, offline: Bool, cleanup: WorktreeCleanup?, deleteAfter: Binding<Bool>, merged: @escaping () -> Void) {
        self.store = store
        self.model = model
        self.pr = pr
        self.offline = offline
        self.cleanup = cleanup
        _deleteAfter = deleteAfter
        self.merged = merged
        _method = AppStorage(wrappedValue: .squash, "github.mergeMethod.\(PullRequestText.repo(pr.url) ?? "")")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: Space.s) {
                Button { merging = true } label: {
                    Text(model.busy ?? method.label).font(.system(size: 17, weight: .semibold)).frame(maxWidth: .infinity)
                }
                .buttonStyle(PillButtonStyle(.primary))
                .disabled(model.busy != nil || offline)
                Menu {
                    Picker("Merge method", selection: $method) {
                        ForEach(MergeMethod.allCases, id: \.self) { Text($0.label).tag($0) }
                    }
                } label: {
                    LucideIcon(.settings, size: 20)
                        .foregroundStyle(Palette.text.color)
                        .frame(width: 44, height: 44)
                        .background(Palette.fill3.color, in: Circle())
                        .contentShape(Circle())
                }
                .accessibilityLabel("Merge method")
            }
            if let cleanup {
                Toggle(isOn: $deleteAfter) {
                    Text("Delete the worktree after merging").font(.system(size: 15)).foregroundStyle(Palette.text.color)
                }
                .toggleStyle(.switch)
                .tint(Palette.text.color)
                .accessibilityHint(cleanup.closes)
            }
        }
        .confirmationDialog("Merge #\(pr.number) into \(PullRequestText.baseBranch(pr.base))?", isPresented: $merging, titleVisibility: .visible) {
            let chosen = method
            Button(chosen.label) {
                Task {
                    await store.merge(model.tree, method: chosen)
                    if model.pr?.state == "MERGED" {
                        Haptics.success()
                        merged()
                    } else {
                        Haptics.warning()
                    }
                }
            }
        } message: {
            Text(deleteAfter && cleanup != nil ? "The branch stays; then the worktree is deleted." : "The branch stays.")
        }
    }
}
