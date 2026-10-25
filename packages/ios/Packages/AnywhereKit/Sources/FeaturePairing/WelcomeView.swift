import DesignSystem
import SwiftUI

/// What Anywhere is, that it needs Tailscale, and the ways to pair.
@MainActor
struct WelcomeView: View {
    let model: PairingModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Space.xl) {
                VStack(alignment: .leading, spacing: Space.m) {
                    AppMark()
                    Text("Anywhere").font(Typography.largeTitle).foregroundStyle(Palette.text.color)
                    Text("Watch and drive the coding agents on your Mac, from your iPhone.")
                        .font(Typography.body).foregroundStyle(Palette.text2.color)
                }
                VStack(alignment: .leading, spacing: Space.l) {
                    Point(systemImage: "bell.badge", title: "Know when a session needs you", detail: "Approve, answer and reply without going back to your desk.")
                    Point(systemImage: "terminal", title: "Everything the Mac shows", detail: "Chats, terminals, changes, pull requests and files.")
                    Point(
                        systemImage: "lock.shield", title: "Private over Tailscale",
                        detail: "Your iPhone talks straight to your Mac on your own tailnet. Install Tailscale on both and sign in to the same account.")
                }
                Text("On your Mac, open Anywhere → Settings → Phone → Pair a phone.")
                    .font(Typography.footnote).foregroundStyle(Palette.text3.color)
            }
            .padding(.horizontal, Space.xl)
            .padding(.top, Space.xxl)
            .frame(maxWidth: 560, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: Space.s) {
                Button {
                    Haptics.tap()
                    model.path.append(.scan)
                } label: {
                    Label("Scan pairing code", systemImage: "qrcode.viewfinder").frame(maxWidth: .infinity)
                }
                .buttonStyle(.borderedProminent)
                Button("Enter code instead") { model.path.append(.enter) }
                    .buttonStyle(.borderless)
                    .padding(.vertical, Space.xs)
                Link("Get Tailscale", destination: Tailscale.appStore)
                    .font(Typography.footnote)
            }
            .controlSize(.large)
            .padding(.horizontal, Space.xl)
            .padding(.vertical, Space.m)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
            .background(Palette.background.color)
        }
        .screenBackground()
        .toolbar(.hidden)
    }
}

/// The app's mark: a Mac and a phone on an ink tile.
@MainActor
struct AppMark: View {
    var size: CGFloat = 64

    var body: some View {
        Image(systemName: "laptopcomputer.and.iphone")
            .font(.system(size: size * 0.42, weight: .medium))
            .foregroundStyle(Palette.onText.color)
            .frame(width: size, height: size)
            .background(Palette.text.color, in: RoundedRectangle(cornerRadius: size * 0.24, style: .continuous))
            .accessibilityHidden(true)
    }
}

@MainActor
struct Point: View {
    let systemImage: String
    let title: String
    let detail: String

    var body: some View {
        HStack(alignment: .top, spacing: Space.m) {
            Image(systemName: systemImage)
                .font(.system(.title3))
                .foregroundStyle(Palette.text2.color)
                .frame(width: 32)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(title).font(Typography.headline).foregroundStyle(Palette.text.color)
                Text(detail).font(Typography.subheadline).foregroundStyle(Palette.text3.color).fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

enum Tailscale {
    static let appStore = URL(string: "https://apps.apple.com/app/tailscale/id1470499037")!
}
