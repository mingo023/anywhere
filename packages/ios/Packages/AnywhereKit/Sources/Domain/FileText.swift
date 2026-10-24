/// How the Files screens name sizes, languages and paths, as the desktop's Explorer does (`explorer/preview.rs`).
public enum FileText {
    /// "612 B", "2.0 KB", "3.0 MB".
    public static func size(_ bytes: Int) -> String {
        switch bytes {
        case ..<1024: "\(bytes) B"
        case ..<(1024 * 1024): String(format: "%.1f KB", Double(bytes) / 1024)
        default: String(format: "%.1f MB", Double(bytes) / (1024 * 1024))
        }
    }

    /// The language's display name for a path: "TypeScript", or "Plain text".
    public static func language(_ path: String) -> String {
        switch ext(path) {
        case "ts", "tsx": "TypeScript"
        case "js", "jsx", "mjs", "cjs": "JavaScript"
        case "rs": "Rust"
        case "py": "Python"
        case "go": "Go"
        case "swift": "Swift"
        case "kt", "kts": "Kotlin"
        case "java": "Java"
        case "json": "JSON"
        case "md": "Markdown"
        case "toml": "TOML"
        case "yml", "yaml": "YAML"
        case "sh", "zsh", "bash": "Shell"
        case "css": "CSS"
        case "html": "HTML"
        case "svg": "SVG"
        default: "Plain text"
        }
    }

    public static func isMarkdown(_ path: String) -> Bool { ["md", "markdown"].contains(ext(path).lowercased()) }

    /// The extension after the last dot of the base name, as written; "" for none or a dotfile.
    public static func ext(_ path: String) -> String {
        let name = baseName(path)
        guard let dot = name.lastIndex(of: "."), dot != name.startIndex else { return "" }
        return String(name[name.index(after: dot)...])
    }

    public static func baseName(_ path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }

    /// The folder holding `path`, "" at the root.
    public static func folder(_ path: String) -> String {
        guard let slash = path.lastIndex(of: "/") else { return "" }
        return String(path[..<slash])
    }

    /// `rel` inside `tree`, as an absolute path to copy or hand to an agent.
    public static func absolute(tree: String, _ rel: String) -> String {
        rel.isEmpty ? tree : tree.hasSuffix("/") ? tree + rel : tree + "/" + rel
    }

    /// `rel` inside `dir`, both relative to the tree.
    public static func join(_ dir: String, _ name: String) -> String { dir.isEmpty ? name : dir + "/" + name }

    /// Where a link in the file at `file` points, relative to the tree; nil when it leads out of it.
    public static func linked(_ link: String, from file: String, tree: String) -> String? {
        let root = tree.hasSuffix("/") ? tree : tree + "/"
        var parts = folder(file).split(separator: "/")
        var rest = Substring(link)
        if link.hasPrefix("/") {
            guard link.hasPrefix(root) else { return nil }
            parts = []
            rest = link.dropFirst(root.count)
        }
        for part in rest.split(separator: "/") {
            switch part {
            case ".": continue
            case "..":
                guard !parts.isEmpty else { return nil }
                parts.removeLast()
            default: parts.append(part)
            }
        }
        return parts.isEmpty ? nil : parts.joined(separator: "/")
    }
}
