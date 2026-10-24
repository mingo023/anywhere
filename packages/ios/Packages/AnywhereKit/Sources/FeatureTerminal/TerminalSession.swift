import Foundation
import Observation
import Stores
import SwiftTerm

#if os(iOS)
    import UIKit

    /// The phone's terminal view. pocketd's own emulator already answers a program's queries, so this one's replies are
    /// dropped: answering twice would type garbage. Keystrokes still go out through `send(data:)`.
    final class PhoneTerminalView: TerminalView {
        override func send(source: Terminal, data: ArraySlice<UInt8>) {}
    }
#endif

/// Drops the headless emulator's replies, as `PhoneTerminalView` does.
final class Quiet: TerminalDelegate {
    func send(source: Terminal, data: ArraySlice<UInt8>) {}
}

/// One open terminal's emulator, fed by the store's frames: a `TerminalView` on the phone, a headless `Terminal` on the Mac
/// (tests, gallery).
@MainActor
@Observable
public final class TerminalSession: TerminalSink {
    public let id: String
    public private(set) var cols: Int
    public private(set) var rows: Int
    /// Bumped per feed where nothing redraws on its own: the Mac's text rendering.
    public private(set) var revision = 0
    /// The key bar's Ctrl and Alt, waiting for the next key.
    public var sticky = StickyMods()

    #if os(iOS)
        @ObservationIgnored let view: PhoneTerminalView
        public var terminal: Terminal { view.getTerminal() }
    #else
        @ObservationIgnored public let terminal: Terminal
        @ObservationIgnored private let quiet = Quiet()
    #endif

    public init(id: String, cols: Int = 80, rows: Int = 24) {
        self.id = id
        self.cols = cols
        self.rows = rows
        #if os(iOS)
            view = PhoneTerminalView(frame: .zero, font: UIFont.monospacedSystemFont(ofSize: Fit.defaultFontSize, weight: .regular))
            view.inputAccessoryView = nil
            view.getTerminal().resize(cols: cols, rows: rows)
        #else
            terminal = Terminal(delegate: quiet, options: TerminalOptions(cols: cols, rows: rows, scrollback: 1000))
        #endif
    }

    public var applicationCursor: Bool { terminal.applicationCursor }
    public var bracketedPaste: Bool { terminal.bracketedPasteMode }

    public func replace(cols: Int, rows: Int, with bytes: Data) {
        terminal.resetToInitialState()
        resize(cols: cols, rows: rows)
        feed(bytes)
    }

    public func feed(_ bytes: Data) {
        let array = [UInt8](bytes)
        #if os(iOS)
            view.feed(byteArray: array[...])
        #else
            terminal.feed(byteArray: array)
            revision &+= 1
        #endif
    }

    public func resize(cols: Int, rows: Int) {
        guard cols != self.cols || rows != self.rows || terminal.cols != cols || terminal.rows != rows else { return }
        self.cols = cols
        self.rows = rows
        terminal.resize(cols: cols, rows: rows)
        revision &+= 1
    }

    /// The visible screen, a line per row with trailing blanks trimmed.
    public func lines() -> [String] {
        (0..<terminal.rows).map { terminal.getLine(row: $0)?.translateToString(trimRight: true) ?? "" }
    }
}
