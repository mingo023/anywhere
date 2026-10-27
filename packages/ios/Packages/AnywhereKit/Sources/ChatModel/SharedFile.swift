import Foundation

/// Files an agent shares with the user through Anywhere's `share_file` tool (desktop `chat/src/shared.rs`).
public enum SharedFile {
    /// What Claude names `share_file`: the tool of the `anywhere` server in pocketd's `anywhere` plugin.
    public static let tool = "mcp__plugin_anywhere_anywhere__share_file"

    /// Files the file preview can't show, which only an app opens.
    private static let app: Set<String> = [
        "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "key", "pages", "numbers", "rtf", "heic", "mp4", "mov", "m4v", "webm", "mp3",
        "wav", "m4a", "aac", "zip",
    ]

    /// Whether the phone's file preview shows the file at `path`: an image, text, code, markdown or SVG.
    public static func previews(_ path: String) -> Bool {
        let name = name(path)
        guard let dot = name.lastIndex(of: ".") else { return true }
        return !app.contains(name[name.index(after: dot)...].lowercased())
    }

    /// Where Anywhere keeps the copy the user sees: what the tool answered, once the share went through.
    public static func kept(_ call: Call) -> String? {
        guard call.status == .ok, let out = call.output?.trimmingCharacters(in: .whitespacesAndNewlines), !out.isEmpty else { return nil }
        return out
    }

    /// What the card reads after the file's name: its caption, that it's on its way, or why it didn't go through.
    public static func detail(_ call: Call) -> String {
        guard case .shared(_, let caption) = call.kind else { return "" }
        switch call.status {
        case .running: return "Sharing…"
        case .failed:
            let out = call.output?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            return out.isEmpty ? "Couldn't share it" : out
        case .ok: return caption.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        }
    }

    /// The file's own name, as the card shows it.
    public static func name(_ path: String) -> String {
        path.split(whereSeparator: { $0 == "/" || $0 == "\\" }).last.map(String.init) ?? path
    }

    /// The folder `fs.read` serves a kept copy from, and its name there.
    public static func place(_ kept: String) -> (tree: String, path: String) {
        guard let slash = kept.lastIndex(of: "/") else { return ("", kept) }
        return (String(kept[..<slash]), String(kept[kept.index(after: slash)...]))
    }
}
