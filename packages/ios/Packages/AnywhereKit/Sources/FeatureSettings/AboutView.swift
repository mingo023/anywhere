import DesignSystem
import Foundation
import SwiftUI

/// The app's version as its bundle names it.
enum AppVersion {
    nonisolated static func short(_ info: [String: Any]?) -> String {
        info?["CFBundleShortVersionString"] as? String ?? "Development"
    }

    /// "0.1.48 (212)": the version, then the build.
    nonisolated static func full(_ info: [String: Any]?) -> String {
        guard let short = info?["CFBundleShortVersionString"] as? String else { return "Development" }
        return (info?["CFBundleVersion"] as? String).map { "\(short) (\($0))" } ?? short
    }
}

/// The app's version and protocol, and where to read about Tailscale.
@MainActor
struct AboutView: View {
    var info = Bundle.main.infoDictionary
    private static let tailscale = URL(string: "https://tailscale.com/kb/1017/install")!

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                LucideIcon(.monitorSmartphone, size: 34, weight: 1.75)
                    .foregroundStyle(Palette.onText.color)
                    .frame(width: 64, height: 64)
                    .background(Palette.text.color, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                    .padding(.top, Space.xl)
                Text("Zoes").font(.system(size: 22, weight: .bold)).foregroundStyle(Palette.text.color).padding(.top, 14)
                VStack(spacing: 0) {
                    LabeledRow("Version", value: AppVersion.full(info), first: true)
                    LabeledRow("Protocol", value: "3")
                    Link(destination: Self.tailscale) {
                        LabeledRow(title: "Setting up Tailscale") { LucideIcon(.external, size: 17).foregroundStyle(Palette.text3.color) }
                    }
                    .buttonStyle(.plain)
                }
                .insetGroup()
                .padding(.top, 28)
                GroupFooter("Zoes talks straight to your Macs over Tailscale. Nothing passes through a server but encrypted notifications.")
            }
        }
        .screenBackground()
        .pushedScreen("About")
    }
}
