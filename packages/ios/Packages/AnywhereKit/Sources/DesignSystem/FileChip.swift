import SwiftUI

/// A file's language badge: two letters on its colour, blank for a language the desktop doesn't mark.
public struct LanguageBadge: Hashable, Sendable {
    public let letters: String
    public let rgb: UInt32

    public init(path: String) {
        let ext = path.split(separator: ".", omittingEmptySubsequences: false).last.map { $0.lowercased() } ?? ""
        switch ext {
        case "rs": (letters, rgb) = ("RS", 0xC2410C)
        case "ts", "tsx": (letters, rgb) = ("TS", 0x3178C6)
        case "md": (letters, rgb) = ("MD", 0x3E8EF7)
        case "toml": (letters, rgb) = ("TM", 0x9C4221)
        case "json": (letters, rgb) = ("JS", 0x8B8B94)
        case "swift": (letters, rgb) = ("SW", 0xF05138)
        default: (letters, rgb) = ("", 0x8B8B94)
        }
    }

    public var color: Color { Token.fixed(rgb << 8 | 0xff).color }
}

/// A file named in a row: its badge and its base name in mono.
@MainActor
public struct FileChip: View {
    let path: String
    let failed: Bool

    public init(_ path: String, failed: Bool = false) {
        self.path = path
        self.failed = failed
    }

    public nonisolated static func name(_ path: String) -> String {
        path.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? path
    }

    public var body: some View {
        let badge = LanguageBadge(path: path)
        HStack(spacing: 5) {
            Text(badge.letters)
                .font(.system(size: 7.5, weight: .heavy, design: .rounded))
                .foregroundStyle(.white)
                // Wide pairs like "SW" and "MD" outgrow the square at this size and would truncate to "…".
                .lineLimit(1)
                .minimumScaleFactor(0.5)
                .frame(width: 13, height: 13)
                .background(badge.color, in: RoundedRectangle(cornerRadius: 3))
            Text(Self.name(path)).lineLimit(1).truncationMode(.middle)
        }
        .font(Typography.codeSmall)
        .foregroundStyle((failed ? Palette.failed : Palette.textBody).color)
        .padding(.leading, 4)
        .padding(.trailing, 5)
        .padding(.vertical, 2)
        .background(Palette.fill4.color, in: RoundedRectangle(cornerRadius: Radius.chip))
        .accessibilityLabel(path)
    }
}
