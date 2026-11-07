import DesignSystem
import Domain
import Foundation
import Highlight
import Stores
import Wire

/// What the preview screen draws for a file, decided from its read alone (desktop `PreviewState::body`).
enum PreviewBody: Equatable {
    case loading
    case code(String, language: String)
    case markdown(String)
    case svg(String)
    case image(Data)
    case note(String, systemImage: String, detail: String? = nil)
    /// The read failed; the screen offers Try Again.
    case failed(String)
    case folder

    /// Markdown and SVG render unless `source` asks for their text.
    static func of(_ phase: FilePreview.Phase, path: String, source: Bool) -> PreviewBody {
        switch phase {
        case .loading: return .loading
        case .failed(let message): return .failed(message)
        case .folder: return .folder
        case .loaded(let read): return of(read, path: path, source: source)
        }
    }

    static func of(_ read: FsRead, path: String, source: Bool) -> PreviewBody {
        switch read.kind {
        case .text:
            let text = read.text ?? ""
            if !source, read.mime == "image/svg+xml" { return .svg(text) }
            if !source, read.language == "markdown" || FileText.isMarkdown(path) { return .markdown(text) }
            return .code(text, language: language(read, path: path))
        case .image:
            guard let data = read.data else { return .note(unreadable, systemImage: "photo") }
            return .image(data)
        case .binary: return .note("Binary file · \(FileText.size(read.size))", systemImage: "doc.zipper")
        case .tooLarge: return .note("Too large to preview · \(FileText.size(read.size))", systemImage: "doc.badge.ellipsis")
        default: return .note(unreadable, systemImage: "eye.slash", detail: unreadableWhy)
        }
    }

    static let unreadable = "Can't read this file"
    /// pocketd says only that it couldn't.
    static let unreadableWhy = "It may be a broken link, a special file, or not readable."

    /// The language id the preview highlights and names: the Mac's, else the highlighter's for the path. pocketd says
    /// "text" for every name it doesn't know, so that counts as not knowing.
    static func language(_ read: FsRead, path: String) -> String {
        read.language.flatMap { $0 == "text" ? nil : $0 } ?? Highlighter.language(for: path)
    }

    /// Whether the file has a rendered look and a source look to switch between.
    static func hasSource(_ read: FsRead?, path: String) -> Bool {
        guard let read, read.kind == .text else { return false }
        return read.mime == "image/svg+xml" || read.language == "markdown" || FileText.isMarkdown(path)
    }

    /// The line over a cut file: "Showing the first 512.0 KB of 3.0 MB".
    static func truncation(_ read: FsRead) -> String? {
        guard read.truncated == true else { return nil }
        return "Showing the first \(FileText.size(read.text?.utf8.count ?? 0)) of \(FileText.size(read.size))"
    }

    /// "src/chat · Rust · 2.0 KB", under the file's name; the folder alone until the file is read.
    static func subtitle(_ read: FsRead?, path: String) -> String? {
        let folder = FileText.folder(path)
        var parts = folder.isEmpty ? [] : [folder]
        if let read, read.kind != .unreadable {
            if read.kind == .text { parts.append(FileText.language(language(read, path: path))) }
            parts.append(FileText.size(read.size))
        }
        return parts.isEmpty ? nil : parts.joined(separator: " · ")
    }
}

/// How a git letter reads: its colour and word, as the Explorer's `status_word`.
enum GitLetter {
    static func color(_ letter: String) -> Token {
        switch letter {
        case "A", "?": Palette.success
        case "D", "U": Palette.failed
        default: Palette.waiting
        }
    }

    static func word(_ letter: String) -> String {
        switch letter {
        case "A": "Added"
        case "?": "Untracked"
        case "D": "Deleted"
        case "R": "Renamed"
        case "U": "Conflict"
        default: "Modified"
        }
    }
}

/// A go-to-file result: the base name with the matched letters marked, and the folder it is in.
struct SearchHit: Equatable {
    let name: String
    let folder: String
    /// Offsets into `name`'s characters that matched.
    let marked: [Int]

    init(path: String, query: String) {
        name = FileText.baseName(path)
        folder = FileText.folder(path)
        let utf8 = Array(path.utf8)
        let nameStart = utf8.count - name.utf8.count
        let offsets = Set(FuzzyMatch.positions(FuzzyMatch.normalize(query), in: path).filter { $0 >= nameStart }.map { $0 - nameStart })
        var marked: [Int] = []
        var byte = 0
        for (i, c) in name.enumerated() {
            if offsets.contains(byte) { marked.append(i) }
            byte += c.utf8.count
        }
        self.marked = marked
    }
}
