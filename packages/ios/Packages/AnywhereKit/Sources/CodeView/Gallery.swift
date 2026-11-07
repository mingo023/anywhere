import DesignSystem
import DiffModel
import SwiftUI
import Wire

/// Scenes for the gallery: source text, unwrapped and wrapped, a wrapped diff, and an agent's edit as a chat shows it
/// inline.
public enum CodeGallery {
    static let source = """
        /// Lays out the next commit, newest first.
        pub fn push(&mut self, sha: &str, parents: &[String]) -> GraphRow {
            let input = std::mem::take(&mut self.open);
            let column = input.iter().position(|l| l.sha == sha);
            // A commit no lane waits for starts a new one.
            let lane = column.unwrap_or(input.len());
            GraphRow { lane, edges: Vec::new(), color: self.next_color() }
        }
        """

    static let edit = FileDiff(
        lines: [
            DiffLine(number: 30, kind: .context, text: "    /// Lays out the next commit, newest first."),
            DiffLine(number: 31, kind: .context, text: "    public mutating func push(_ sha: String, parents: [String]) -> GraphRow {"),
            DiffLine(number: 32, kind: .del, text: "        let input = std::mem::take(&mut self.open)"),
            DiffLine(number: 32, kind: .add, text: "        let input = open"),
            DiffLine(number: 33, kind: .add, text: "        var output: [Lane] = []"),
            DiffLine(number: 34, kind: .context, text: "        var placed = false"),
            DiffLine(number: 62, kind: .del, text: "        next += 1; return .lane(next - 1)"),
            DiffLine(number: 63, kind: .add, text: "        defer { next += 1 }"),
        ], additions: 3, deletions: 2)

    static let diff = DiffDocument(
        GitDiff(
            tree: "/r", path: "src/graph.rs", language: "rust", added: 2, removed: 1,
            lines: [
                Wire.DiffRow(k: .hunk, t: "@@ -30,4 +30,5 @@ impl Lanes"),
                Wire.DiffRow(k: .context, o: 30, n: 30, t: "    /// Lays out the next commit, newest first."),
                Wire.DiffRow(k: .deleted, o: 31, t: "    pub fn push(&mut self, sha: &str, parents: &[String]) -> GraphRow {"),
                Wire.DiffRow(k: .added, n: 31, t: "    pub fn push(&mut self, sha: &str, parents: &[String], label: impl Fn(&str) -> Option<LaneColor>) -> GraphRow {"),
                Wire.DiffRow(k: .added, n: 32, t: "        let input = std::mem::take(&mut self.open);"),
                Wire.DiffRow(k: .context, o: 32, n: 33, t: "        let column = input.iter().position(|l| l.sha == sha);"),
            ]))

    @MainActor public static var scenes: [GalleryScene] {
        [false, true].flatMap { dark in
            let suffix = dark ? "-dark" : ""
            return [
                GalleryScene("code-text\(suffix)", size: CGSize(width: 393, height: 240), dark: dark) {
                    AnyView(CodeTextView(text: source, language: "rust"))
                },
                GalleryScene("code-text-wrapped\(suffix)", size: CGSize(width: 393, height: 300), dark: dark) {
                    AnyView(CodeTextView(text: source, language: "rust", wrap: true))
                },
                GalleryScene("code-diff-wrapped\(suffix)", size: CGSize(width: 393, height: 240), dark: dark) {
                    AnyView(DiffView(diff, wrap: true))
                },
                GalleryScene("code-edit-inline\(suffix)", size: CGSize(width: 393, height: 240), dark: dark) {
                    AnyView(EditDiffView(path: "Sources/DiffModel/Lanes.swift", diff: edit).padding(16).frame(maxHeight: .infinity, alignment: .top)
                        .background(Palette.background.color))
                },
            ]
        }
    }
}
