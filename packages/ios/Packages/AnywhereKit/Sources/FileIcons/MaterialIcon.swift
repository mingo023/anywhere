import Foundation
import ImageIO
import SwiftUI

/// Material Icon Theme's names, as the desktop's `icons.json` maps them.
public enum MaterialIcons {
    private struct Manifest: Decodable, Sendable {
        var names: [String: String] = [:]
        var extensions: [String: String] = [:]
        var folders: [String: String] = [:]
        var foldersOpen: [String: String] = [:]
    }

    private static let manifest: Manifest = {
        guard let url = Bundle.module.url(forResource: "icons", withExtension: "json", subdirectory: "material"),
            let data = try? Data(contentsOf: url), let manifest = try? JSONDecoder().decode(Manifest.self, from: data)
        else { return Manifest() }
        return manifest
    }()

    /// The icon for the last component of `path`, matched like VS Code: the whole name, then its extensions longest first.
    public nonisolated static func name(_ path: String, folder: Bool = false, open: Bool = false) -> String {
        let base = (path.split(separator: "/").last.map(String.init) ?? path).lowercased()
        if folder { return open ? manifest.foldersOpen[base] ?? "folder-open" : manifest.folders[base] ?? "folder" }
        if let named = manifest.names[base] { return named }
        for dot in base.indices where base[dot] == "." {
            if let icon = manifest.extensions[String(base[base.index(after: dot)...])] { return icon }
        }
        return "file"
    }

    static var everyName: Set<String> {
        Set([manifest.names, manifest.extensions, manifest.folders, manifest.foldersOpen].flatMap(\.values))
    }

    /// Decodes the manifest ahead of the first row: at 370 KB it's too much to read on the main actor while a list opens.
    public nonisolated static func preload() { _ = manifest }

    /// Read once per icon: a tree redraws its rows far more often than it meets a new kind of file.
    @MainActor private static var images: [String: CGImage] = [:]

    @MainActor static func image(_ name: String) -> CGImage? {
        if let image = images[name] { return image }
        guard let url = Bundle.module.url(forResource: name, withExtension: "png", subdirectory: "material"),
            let source = CGImageSourceCreateWithURL(url as CFURL, nil), let image = CGImageSourceCreateImageAtIndex(source, 0, nil)
        else { return nil }
        images[name] = image
        return image
    }
}

/// A file's or folder's Material icon.
@MainActor
public struct MaterialIcon: View {
    let name: String
    let size: CGFloat

    public init(_ path: String, folder: Bool = false, open: Bool = false, size: CGFloat = 16) {
        name = MaterialIcons.name(path, folder: folder, open: open)
        self.size = size
    }

    public var body: some View {
        Group {
            if let image = MaterialIcons.image(name) {
                // 3× at the common 16pt.
                Image(decorative: image, scale: 3).resizable().interpolation(.high)
            } else {
                Color.clear
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
