import DesignSystem
import SwiftUI

/// What Zoes is, that it needs Tailscale, and the ways to pair.
@MainActor
struct WelcomeView: View {
    let model: PairingModel

    var body: some View {
        GeometryReader { screen in
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    AppMark()
                    Text("Zoes").font(.system(size: 34, weight: .bold)).foregroundStyle(Palette.text.color).padding(.top, Space.xl)
                    Text("Watch and drive the coding agents on your Mac from your iPhone")
                        .font(.system(size: 17)).foregroundStyle(Palette.textBody.color)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, Space.s)
                    VStack(alignment: .leading, spacing: Space.l) {
                        Point(icon: .bell, text: "Know when a session needs you")
                        Point(icon: .lock, text: "Private link over Tailscale")
                    }
                    .padding(.top, Space.xxl)
                    Text("On your Mac, open Zoes → Settings → Phone → Pair a phone.")
                        .font(.system(size: 15)).foregroundStyle(Palette.textBody.color)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.horizontal, Space.l)
                        .padding(.vertical, 14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: Inset.radius, style: .continuous))
                        .padding(.top, Space.xxl)
                }
                .padding(.horizontal, 28)
                .frame(maxWidth: 560)
                .frame(maxWidth: .infinity, minHeight: screen.size.height, alignment: .center)
            }
            .scrollBounceBehavior(.basedOnSize)
        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: Space.xs) {
                Button {
                    Haptics.tap()
                    model.path.append(.scan)
                } label: {
                    HStack(spacing: Space.s) {
                        LucideIcon(.scanQrCode, size: 17)
                        Text("Scan pairing code")
                    }
                }
                .buttonStyle(LargeButtonStyle())
                Button("Enter code instead") { model.path.append(.enter) }
                    .buttonStyle(QuietButtonStyle())
                HStack(spacing: Space.xs) {
                    Text("No Tailscale yet?").foregroundStyle(Palette.text3.color)
                    Link("Get Tailscale", destination: Tailscale.appStore).fontWeight(.semibold).foregroundStyle(Palette.text.color)
                }
                .font(.system(size: 13))
            }
            .padding(.horizontal, 28)
            .padding(.bottom, Space.s)
            .frame(maxWidth: 560)
            .frame(maxWidth: .infinity)
        }
        .screenBackground()
        .toolbar(.hidden)
    }
}

/// The app's mark: a Mac and a phone on a light tile, as the design draws it in both appearances.
@MainActor
struct AppMark: View {
    var size: CGFloat = 64

    var body: some View {
        LucideIcon(.monitorSmartphone, size: size * 34 / 64, weight: 1.75)
            .foregroundStyle(Token.fixed(0x171717ff).color)
            .frame(width: size, height: size)
            .background(Token.fixed(0xebebebff).color, in: RoundedRectangle(cornerRadius: size / 4, style: .continuous))
            .accessibilityHidden(true)
    }
}

/// A line of the welcome's short list: an icon and a sentence.
@MainActor
struct Point: View {
    let icon: Lucide
    let text: String

    var body: some View {
        HStack(spacing: 14) {
            LucideIcon(icon, size: 17).foregroundStyle(Palette.text2.color)
            Text(text).font(.system(size: 17)).foregroundStyle(Palette.text.color)
        }
        .accessibilityElement(children: .combine)
    }
}

enum Tailscale {
    static let appStore = URL(string: "https://apps.apple.com/app/tailscale/id1470499037")!
}
