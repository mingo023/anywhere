import CoreGraphics
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
    public private(set) var fontSize: CGFloat
    /// The screen drawn at cols × rows, in points; the phone scrolls across it.
    public private(set) var surface = CGSize.zero
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

    public init(id: String, cols: Int = 80, rows: Int = 24, fontSize: CGFloat = Fit.defaultFontSize) {
        self.id = id
        self.cols = cols
        self.rows = rows
        self.fontSize = fontSize
        #if os(iOS)
            view = PhoneTerminalView(frame: .zero, font: UIFont.monospacedSystemFont(ofSize: fontSize, weight: .regular))
            view.inputAccessoryView = nil
            view.getTerminal().resize(cols: cols, rows: rows)
        #else
            terminal = Terminal(delegate: quiet, options: TerminalOptions(cols: cols, rows: rows, scrollback: 1000))
        #endif
        measure()
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
        if terminal.cols != cols || terminal.rows != rows {
            terminal.resize(cols: cols, rows: rows)
        }
        guard cols != self.cols || rows != self.rows else { return }
        self.cols = cols
        self.rows = rows
        measure()
        revision &+= 1
    }

    /// Draws at a new font size. On the phone the view reflows its screen to the size it had, so the caller attaches
    /// again for a fresh snapshot.
    public func setFontSize(_ size: CGFloat) {
        let size = min(max(size, Fit.fontSizes.lowerBound), Fit.fontSizes.upperBound)
        guard size != fontSize else { return }
        fontSize = size
        #if os(iOS)
            view.font = UIFont.monospacedSystemFont(ofSize: size, weight: .regular)
        #endif
        measure()
    }

    /// One cell, as the emulator lays it out.
    public var cell: CGSize {
        #if os(iOS)
            let optimal = view.getOptimalFrameSize().size
            return CGSize(width: optimal.width / CGFloat(max(terminal.cols, 1)), height: optimal.height / CGFloat(max(terminal.rows, 1)))
        #else
            Fit.cell(fontSize: fontSize)
        #endif
    }

    private func measure() {
        let cell = cell
        // Half a point over, so the view's own floor(width / cell) lands on cols, not one short.
        surface = CGSize(width: cell.width * CGFloat(cols) + 0.5, height: cell.height * CGFloat(rows) + 0.5)
    }

    /// The visible screen, a line per row with trailing blanks trimmed; cells a tab skipped read as spaces.
    public func lines() -> [String] {
        (0..<terminal.rows).map { row in
            terminal.getLine(row: row)?.translateToString(trimRight: true, skipNullCellsFollowingWide: true) { cell in
                let c = cell.getCharacter()
                return c == "\0" ? " " : c
            } ?? ""
        }
    }
}
