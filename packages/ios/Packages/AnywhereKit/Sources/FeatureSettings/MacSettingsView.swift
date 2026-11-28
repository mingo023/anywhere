import DesignSystem
import Stores
import SwiftUI
import Wire

/// One Mac: its link and the way back when it's gone, Tailscale and keep-awake, what this phone may do on it, its alerts,
/// and forgetting it.
@MainActor
struct MacSettingsView: View {
    let app: AppModel
    let host: HostModel
    /// Shows the pairing flow, which the shell owns.
    var pair: (@MainActor () -> Void)?
    var center = PushCenter.shared
    @State private var forgetting = false
    @State private var testing = false
    @State private var tested: String?
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    private static let appStore = URL(string: "itms-apps://")!

    var body: some View {
        // Redrawn at the 4 s and 30 s marks of a drop, when the Mac's reading changes on its own.
        TimelineView(.explicit(host.reachabilityMarks)) { _ in content(host.reachability(at: .now)) }
            .screenBackground()
            .pushedScreen(host.name)
            .confirmationDialog("Forget \(host.name)?", isPresented: $forgetting, titleVisibility: .visible) {
                Button("Forget", role: .destructive) {
                    dismiss()
                    app.forget(host.id)
                }
            } message: {
                Text("This iPhone stops connecting to it. To remove it on the Mac too, revoke it in Zoes → Settings → Phone.")
            }
    }

    private func content(_ reachability: Reachability) -> some View {
        let standing = MacFacts.standing(reachability)
        let dimmed = standing == .offline || standing == .removed
        return ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                header(reachability)
                connection(standing).opacity(dimmed ? 0.4 : 1).padding(.top, Space.l)
                if standing == .online || standing == .offline { settings(standing).opacity(standing == .offline ? 0.4 : 1) }
                Button("Forget this machine", role: .destructive) { forgetting = true }
                    .buttonStyle(.plain)
                    .font(.system(size: 17))
                    .foregroundStyle(Palette.failedText.color)
                    .frame(maxWidth: .infinity, minHeight: HitTarget.minimum)
                    .insetGroup()
                    .padding(.top, Space.l)
            }
            .padding(.bottom, Space.xl)
        }
    }

    private func header(_ reachability: Reachability) -> some View {
        let lastSeen = host.offlineSince.map { Int($0.timeIntervalSince1970 * 1000) }
        return VStack(alignment: .leading, spacing: 0) {
            Text(host.name).font(.system(size: 28, weight: .bold)).foregroundStyle(Palette.text.color).lineLimit(2)
            HStack(spacing: 6) {
                Circle().fill(MacFacts.dot(reachability).color).frame(width: 8, height: 8)
                Text(MacFacts.detailStatus(reachability, lastSeen: lastSeen))
            }
            .font(.system(size: 15))
            .foregroundStyle(Palette.text2.color)
            .padding(.top, Space.xs)
            if let message = MacFacts.endedMessage(reachability, mac: host.name, version: host.hello?.version) {
                Text(message)
                    .font(.system(size: 15))
                    .foregroundStyle(Palette.textBody.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, Space.m)
            }
            actions(reachability)
        }
        .padding(.horizontal, 20)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder
    private func actions(_ reachability: Reachability) -> some View {
        switch reachability {
        case .online:
            EmptyView()
        case .revoked:
            if let pair {
                Button {
                    Haptics.tap()
                    pair()
                } label: {
                    Text("Pair again").frame(maxWidth: .infinity)
                }
                .buttonStyle(CapsuleButtonStyle())
                .padding(.top, 14)
            }
        case .updateMac, .updatePhone:
            HStack(spacing: Space.s) {
                Button { retry(reachability) } label: { Text("Try again").frame(maxWidth: .infinity) }
                    .buttonStyle(PillButtonStyle(reachability == .updateMac ? .primary : .secondary))
                Button { openURL(Self.appStore) } label: { Text("Update this app").frame(maxWidth: .infinity) }
                    .buttonStyle(PillButtonStyle(reachability == .updatePhone ? .primary : .secondary))
            }
            .padding(.top, 14)
        default:
            Button { retry(reachability) } label: { Text("Reconnect").frame(maxWidth: .infinity) }
                .buttonStyle(CapsuleButtonStyle())
                .padding(.top, Space.m)
        }
    }

    private func retry(_ reachability: Reachability) {
        Haptics.tap()
        if reachability.isFinal { app.restart(host.id) } else { host.scene(active: true) }
    }

    private func connection(_ standing: MacFacts.Standing) -> some View {
        let state = host.hostState
        let offline = standing == .offline
        return VStack(spacing: 0) {
            HStack(spacing: Space.m) {
                Text("Address")
                Text(host.credentials.host)
                    .font(.system(size: 13, design: .monospaced))
                    .foregroundStyle(Palette.text2.color)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .textSelection(.enabled)
            }
            .font(.system(size: 17))
            .foregroundStyle(Palette.text.color)
            .padding(.horizontal, Inset.margin)
            .frame(minHeight: HitTarget.minimum)
            LabeledRow("Zoes version", value: host.hello?.version ?? "—")
            LabeledRow("Tailscale", value: offline ? "Not reachable" : state.map { $0.tailnet ? "Connected" : "Off" } ?? "—")
            if let state, !offline, standing != .removed {
                LabeledRow("Keeping awake", value: state.keepingAwake ? "On" : "Off")
                if let failed = state.upgradeFailed {
                    LabeledRow(title: "Last update") { Text("\(failed) failed").foregroundStyle(Palette.failedText.color) }
                }
            }
        }
        .insetGroup()
    }

    private func settings(_ standing: MacFacts.Standing) -> some View {
        let offline = standing == .offline
        let state = host.push.state
        let note = tested ?? MacFacts.alertsNote(state, permission: center.permission)
        return VStack(alignment: .leading, spacing: 0) {
            GroupHeader("This iPhone can")
            VStack(spacing: 0) {
                ForEach(Array(MacFacts.grants(host.hello?.scopes).enumerated()), id: \.element.id) { index, grant in
                    HStack(spacing: 0) {
                        LucideIcon(grant.allowed ? .check : .close, size: 17)
                            .foregroundStyle((grant.allowed ? Palette.text2 : Palette.text4).color)
                            .frame(width: 48)
                        Text(grant.title).foregroundStyle((grant.allowed ? Palette.text : Palette.text4).color)
                        Spacer(minLength: 0)
                    }
                    .font(.system(size: 17))
                    .frame(minHeight: HitTarget.minimum)
                    .rowDivider(index > 0, leading: 48)
                    .accessibilityElement(children: .combine)
                    .accessibilityValue(grant.allowed ? "Allowed" : "Not allowed")
                }
            }
            .insetGroup()
            GroupFooter(offline ? "Can't change while \(host.name) is offline." : "Change these on the Mac.")
            VStack(spacing: 0) {
                LabeledRow("Notifications for this Mac", value: MacFacts.alerts(state, permission: center.permission), first: true)
                if state == .registered, !offline {
                    Button(action: test) {
                        LabeledRow(title: "Send test notification") { if testing { ProgressView().controlSize(.small) } }
                    }
                    .buttonStyle(.plain)
                    .disabled(testing)
                }
            }
            .insetGroup()
            .padding(.top, Space.l)
            if let note { GroupFooter(note, failed: tested == nil && Self.failed(state)) }
        }
        .allowsHitTesting(!offline)
    }

    private func test() {
        testing = true
        Task {
            do {
                try await host.push.sendTest()
                tested = "Test notification sent."
            } catch {
                tested = PushStore.describe(error, mac: host.name)
            }
            testing = false
        }
    }

    private nonisolated static func failed(_ state: PushStore.State) -> Bool {
        if case .failed = state { true } else { false }
    }
}
