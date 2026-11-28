import Client
import DesignSystem
import SwiftUI

/// "Pair with <Mac>?": what the phone will be able to do, the attempt, and why it failed.
@MainActor
struct ConfirmView: View {
    let model: PairingModel
    let link: PairingLink
    @Environment(\.closePairing) private var close

    private var failure: PairingFailure.Copy? {
        if case .failed(let failure) = model.phase { failure.copy(mac: link.name) } else { nil }
    }

    var body: some View {
        Group {
            if let failure, failure.suggestsTailscale {
                TailscaleView(model: model, link: link, copy: failure)
            } else {
                confirm
            }
        }
        .animation(.snappy, value: model.phase)
        .screenBackground()
        .pairingBar()
        .navigationBarBackButtonHidden(true)
        .toolbar { ToolbarItem(placement: .cancellationAction) { CloseButton() } }
    }

    private var confirm: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                MarkTile(icon: .laptop)
                Text("Pair with \(link.name)?")
                    .font(.system(size: 28, weight: .bold)).foregroundStyle(Palette.text.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 20)
                Text(link.host).font(.system(size: 13, design: .monospaced)).foregroundStyle(Palette.text2.color).textSelection(.enabled).padding(.top, Space.s)
            }
            .padding(.horizontal, 28)
            .padding(.top, 52)
            .frame(maxWidth: .infinity, alignment: .leading)
            GroupHeader("This iPhone will be able to")
            VStack(spacing: 0) {
                Grant(icon: .eye, title: "See sessions", first: true)
                Grant(icon: .pointer, title: "Drive them")
                Grant(icon: .unlink, title: "Remove it any time")
            }
            .insetGroup()
        }
        .safeAreaInset(edge: .bottom) { actions }
    }

    private var actions: some View {
        VStack(spacing: Space.xs) {
            if let failure {
                FailureCard(copy: failure).padding(.bottom, Space.l).transition(.opacity.combined(with: .move(edge: .bottom)))
            } else if model.phase == .pairing {
                Text("Checking the code with \(link.name)")
                    .font(.system(size: 13)).foregroundStyle(Palette.text2.color).padding(.bottom, Space.s)
            }
            Button {
                Task { await model.pair(link) }
            } label: {
                switch model.phase {
                case .pairing:
                    HStack(spacing: 10) {
                        RingSpinner(size: 16)
                        Text("Pairing…").foregroundStyle(Palette.text2.color)
                    }
                case .paired:
                    HStack(spacing: Space.s) {
                        LucideIcon(.check, size: 17)
                        Text("Paired")
                    }
                case .failed: Text("Try again")
                case .idle: Text("Pair")
                }
            }
            .buttonStyle(LargeButtonStyle(primary: model.phase != .pairing))
            .disabled(model.phase == .pairing)
            .padding(.horizontal, 4)
            Button(failure == nil ? "Cancel" : "Enter code instead") {
                if failure == nil { close() } else { model.path = [.enter] }
            }
            .buttonStyle(QuietButtonStyle())
        }
        .padding(.horizontal, Inset.margin)
        .padding(.bottom, Space.s)
        .frame(maxWidth: 560)
        .frame(maxWidth: .infinity)
    }
}

/// An icon on a quiet tile, over a pairing screen's headline.
@MainActor
struct MarkTile: View {
    let icon: Lucide

    var body: some View {
        LucideIcon(icon, size: 34, weight: 1.75)
            .foregroundStyle(Palette.textBody.color)
            .frame(width: 64, height: 64)
            .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// A line of what pairing grants: an icon and a sentence.
@MainActor
struct Grant: View {
    let icon: Lucide
    let title: String
    var first = false

    var body: some View {
        HStack(spacing: 0) {
            LucideIcon(icon, size: 17).foregroundStyle(Palette.text2.color).frame(width: Inset.textColumn)
            Text(title).font(.system(size: 17)).foregroundStyle(Palette.text.color)
            Spacer(minLength: 0)
        }
        .frame(minHeight: HitTarget.minimum)
        .rowDivider(!first)
        .accessibilityElement(children: .combine)
    }
}

/// Why pairing failed and what to try.
@MainActor
struct FailureCard: View {
    let copy: PairingFailure.Copy

    var body: some View {
        HStack(alignment: .top, spacing: Space.m) {
            LucideIcon(.circleAlert, size: 17).foregroundStyle(Palette.failed.color)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(copy.title).font(.system(size: 15).weight(.semibold)).foregroundStyle(Palette.failedText.color)
                Text(copy.message).font(.system(size: 13)).foregroundStyle(Palette.textBody.color).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, Space.l)
        .padding(.vertical, 14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.failedBackground.color, in: RoundedRectangle(cornerRadius: Inset.radius, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

/// A Mac that can't be reached: pairing runs over Tailscale, so the way on is Tailscale.
@MainActor
struct TailscaleView: View {
    let model: PairingModel
    let link: PairingLink
    let copy: PairingFailure.Copy

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                MarkTile(icon: .shieldAlert)
                Text(copy.title)
                    .font(.system(size: 28, weight: .bold)).foregroundStyle(Palette.text.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 20)
                Text(copy.message)
                    .font(.system(size: 17)).foregroundStyle(Palette.textBody.color)
                    .fixedSize(horizontal: false, vertical: true)
                    .padding(.top, 10)
            }
            .padding(.horizontal, 28)
            .padding(.top, 52)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: Space.xs) {
                Link(destination: Tailscale.appStore) {
                    HStack(spacing: Space.s) {
                        Text("Get Tailscale on the App Store")
                        LucideIcon(.external, size: 17)
                    }
                }
                .buttonStyle(LargeButtonStyle())
                Button("Try again") { Task { await model.pair(link) } }
                    .buttonStyle(QuietButtonStyle())
            }
            .padding(.horizontal, 20)
            .padding(.bottom, Space.s)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
        }
    }
}
