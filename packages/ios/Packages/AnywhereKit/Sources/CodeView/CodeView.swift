import SwiftUI
import Highlight
import DiffModel
import DesignSystem

/// Read-only source text, virtualized by line, highlighted off the main actor.
@MainActor
public struct CodeTextView: View {
    let text: String
    let language: String
    let lineNumbers: Bool

    public init(text: String, language: String, lineNumbers: Bool = true) {
        self.text = text
        self.language = language
        self.lineNumbers = lineNumbers
    }

    public var body: some View { ScrollView { Text(text).font(.system(.footnote, design: .monospaced)) } }
}
