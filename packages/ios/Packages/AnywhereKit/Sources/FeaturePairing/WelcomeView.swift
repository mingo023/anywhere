import DesignSystem
import SwiftUI

/// What Anywhere is, that it needs Tailscale, and the ways to pair.
@MainActor
struct WelcomeView: View {
    let model: PairingModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Space.xxl) {
                VStack(alignment: .leading, spacing: Space.m) {
                    AppMark()
                    Text("Anywhere").font(Typography.largeTitle).foregroundStyle(Palette.text.color)
                    Text("Watch and drive the coding agents on your Mac, from your iPhone")
                        .font(Typography.title3).foregroundStyle(Palette.text2.color)
                }
                VStack(alignment: .leading, spacing: Space.l + Space.xs) {
                    Point(title: "Know when a session needs you", detail: "Approvals, questions, failures and finished runs, as they happen.") { StatusGlyph(.needsYou) }
                    Point(systemImage: "terminal", title: "The terminal", detail: "Open the shells running on your Mac and type into them.")
                    Point(systemImage: "lock.shield", title: "Private link over Tailscale", detail: "Your iPhone reaches your Mac across your own tailnet.")
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
                .buttonStyle(CapsuleButtonStyle())
                Button { model.path.append(.enter) } label: { Text("Enter code instead").frame(maxWidth: .infinity) }
                    .buttonStyle(CapsuleButtonStyle(primary: false))
                HStack(spacing: Space.xs) {
                    Text("No Tailscale yet?").foregroundStyle(Palette.text3.color)
                    Link("Get Tailscale", destination: Tailscale.appStore).fontWeight(.semibold).foregroundStyle(Palette.text.color)
                }
                .font(Typography.footnote)
                .padding(.top, Space.xs)
            }
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
struct Point<Icon: View>: View {
    let title: String
    let detail: String
    @ViewBuilder let icon: Icon

    var body: some View {
        HStack(alignment: .top, spacing: Space.m) {
            icon
                .font(.footnote.weight(.medium))
                .foregroundStyle(Palette.text.color)
                .frame(width: 28, height: 28)
                .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(title).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color)
                Text(detail).font(Typography.subheadline).foregroundStyle(Palette.text3.color).fixedSize(horizontal: false, vertical: true)
            }
        }
        .accessibilityElement(children: .combine)
    }
}

extension Point where Icon == Image {
    init(systemImage: String, title: String, detail: String) {
        self.init(title: title, detail: detail) { Image(systemName: systemImage) }
    }
}

enum Tailscale {
    static let appStore = URL(string: "https://apps.apple.com/app/tailscale/id1470499037")!
}
