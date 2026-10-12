import SwiftUI

// The island is black in light mode too, so it keeps macOS's dark colours.
extension Color {
    init(hex: UInt32) {
        self.init(red: Double(hex >> 16 & 0xff) / 255, green: Double(hex >> 8 & 0xff) / 255, blue: Double(hex & 0xff) / 255)
    }

    static let ink = Color(hex: 0xf5f5f7)
    static let ink2 = Color(hex: 0x98989d)
    static let ink3 = Color(hex: 0x6e6e73)
    static let fill = Color(hex: 0x1c1c1e)
    static let push = Color(hex: 0x3a3a3c)
    static let pushHover = Color(hex: 0x48484a)
    static let button = Color(hex: 0xebebeb)
    static let buttonHover = Color(hex: 0xffffff)
    static let onButton = Color(hex: 0x171717)
    static let allowed = Color(hex: 0x30d158)
    static let stopped = Color(hex: 0xff453a)
    static let waiting = Color(hex: 0xffb224)
    static let claude = Color(hex: 0xd97757)
}
