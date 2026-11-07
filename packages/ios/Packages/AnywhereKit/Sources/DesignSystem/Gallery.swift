import SwiftUI

/// Scenes for the gallery: the shared pieces features build their states from.
public enum DesignSystemGallery {
    static let mac = "Mingo's MacBook Pro"

    @MainActor public static var scenes: [GalleryScene] {
        let pieces: [(String, CGFloat, @MainActor () -> AnyView)] = [
            ("design-toast", 240, {
                AnyView(Color.clear.toast(.constant(Toast("Mentioned in Fix the flaky login test", actionTitle: "Go to chat") {})))
            }),
            ("design-toast-text", 120, { AnyView(Color.clear.toast(.constant(Toast("Copied path")))) }),
            ("design-stale-strip", 160, {
                AnyView(VStack(spacing: Space.l) {
                    StaleStrip(mac: mac, asOf: 1_762_512_120_000)
                    StaleStrip(mac: mac)
                    Spacer(minLength: 0)
                })
            }),
            ("design-placeholder-action", 400, {
                AnyView(Placeholder("Can't reach \(mac)", systemImage: "wifi.slash", detail: "Changes show here once it's back.") {})
            }),
            ("design-hit-target", 120, {
                AnyView(HStack(spacing: Space.s) {
                    ForEach(["xmark", "arrow.up", "plus", "ellipsis"], id: \.self) { glyph in
                        Button {} label: { Image(systemName: glyph).imageScale(.small).hitTarget() }
                            .buttonStyle(.borderless)
                            .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: Radius.small))
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity))
            }),
        ]
        return pieces.flatMap { name, height, piece in
            [false, true].map { dark in
                GalleryScene(dark ? "\(name)-dark" : name, size: CGSize(width: 393, height: height), dark: dark) {
                    AnyView(piece().frame(maxWidth: .infinity, maxHeight: .infinity).screenBackground().preferredColorScheme(dark ? .dark : .light))
                }
            }
        }
    }
}
