import Client
import DesignSystem
import SwiftUI

/// "Pair with <Mac>?": what the phone will be able to do, the attempt, and why it failed.
@MainActor
struct ConfirmView: View {
    let model: PairingModel
    let link: PairingLink

    var body: some View {
        ScrollView {
            VStack(spacing: Space.xl) {
                VStack(spacing: Space.s) {
                    Image(systemName: "laptopcomputer")
                        .font(.system(size: 52, weight: .light))
                        .foregroundStyle(Palette.text2.color)
                        .padding(.bottom, Space.xs)
                    Text("Pair with \(link.name)?")
                        .font(Typography.title).foregroundStyle(Palette.text.color).multilineTextAlignment(.center)
                    Text(link.host).font(Typography.codeSmall).foregroundStyle(Palette.text3.color).textSelection(.enabled)
                }
                VStack(alignment: .leading, spacing: Space.m) {
                    Point(systemImage: "eye", title: "See sessions", detail: "Chats, terminals, changes and files on this Mac.")
                    Point(systemImage: "hand.tap", title: "Drive them", detail: "Send prompts, answer permissions and start new sessions.")
                    Point(systemImage: "xmark.shield", title: "Remove it any time", detail: "On the Mac, in Anywhere → Settings → Phone.")
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(Space.l)
                .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
                if case .failed(let failure) = model.phase {
                    FailureCard(copy: failure.copy(mac: link.name))
                        .transition(.opacity.combined(with: .move(edge: .bottom)))
                }
            }
            .padding(.horizontal, Space.xl)
            .padding(.top, Space.xl)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
        }
        .safeAreaInset(edge: .bottom) {
            Button {
                Task { await model.pair(link) }
            } label: {
                ZStack {
                    switch model.phase {
                    case .pairing: ProgressView().controlSize(.regular)
                    case .paired: Label("Paired", systemImage: "checkmark")
                    case .failed: Text("Try again")
                    case .idle: Text("Pair")
                    }
                }
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .disabled(model.phase == .pairing)
            .padding(.horizontal, Space.xl)
            .padding(.vertical, Space.m)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
            .background(Palette.background.color)
        }
        .animation(.snappy, value: model.phase)
        .screenBackground()
        .navigationTitle("Pair")
        .inlineTitle()
    }
}

/// Why pairing failed and what to try, with Tailscale's App Store page when the Mac can't be reached.
@MainActor
struct FailureCard: View {
    let copy: PairingFailure.Copy

    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            Label(copy.title, systemImage: "exclamationmark.triangle.fill")
                .font(Typography.headline)
                .foregroundStyle(Palette.failedText.color)
            Text(copy.message).font(Typography.subheadline).foregroundStyle(Palette.textBody.color).fixedSize(horizontal: false, vertical: true)
            if copy.suggestsTailscale {
                Link(destination: Tailscale.appStore) {
                    Label("Tailscale in the App Store", systemImage: "arrow.up.forward.app")
                }
                .font(Typography.subheadline)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(Space.l)
        .background(Palette.failedBackground.color, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
        .accessibilityElement(children: .combine)
    }
}

extension View {
    /// A small title in the bar on iPhone; macOS has no inline display mode.
    @ViewBuilder
    func inlineTitle() -> some View {
        #if os(iOS)
            navigationBarTitleDisplayMode(.inline)
        #else
            self
        #endif
    }
}
