import DesignSystem
import Foundation
import Stores
import SwiftTerm
import SwiftUI

#if os(iOS)
    import UIKit
#else
    import AppKit
#endif

/// One terminal, attached: drawn at its own size and scrolled across, or fit to the phone on request.
@MainActor
public struct TerminalScreenView: View {
    let store: TerminalStore
    let terminalId: String
    let title: String
    @State private var session: TerminalSession?
    @State private var fit = false
    @State private var pinch: CGFloat = 1
    @State private var area = CGSize.zero
    @State private var confirmClose = false
    @State private var pendingPaste: String?
    @State private var problem: String?
    @AppStorage("terminal.fontSize") private var savedFontSize = Double(Fit.defaultFontSize)
    @Environment(\.dismiss) private var dismiss

    public init(store: TerminalStore, terminalId: String, title: String) {
        self.store = store
        self.terminalId = terminalId
        self.title = title
    }

    /// A screen already fed, for the gallery.
    init(store: TerminalStore, session: TerminalSession, title: String) {
        self.store = store
        terminalId = session.id
        self.title = title
        _session = State(initialValue: session)
    }

    public var body: some View {
        VStack(spacing: 0) {
            if let banner = Banner(state: store.attachments[terminalId]?.state, writesOff: store.writesOff, problem: problem) {
                banner
            }
            if let session {
                surface(session)
            } else {
                Color.clear
            }
        }
        .background(Palette.surface.color.ignoresSafeArea())
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let session {
                KeyBar(held: session.sticky.held, enabled: writable, press: { press($0, session) }, toggle: { session.sticky.toggle($0) },
                       paste: paste)
            }
        }
        .navigationTitle(title)
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar { menu }
        .confirmationDialog("Close this terminal?", isPresented: $confirmClose, titleVisibility: .visible) {
            Button("Close Terminal", role: .destructive, action: close)
        } message: {
            Text("Its shell and whatever runs in it end, on your Mac too.")
        }
        .confirmationDialog("Paste \(pendingPaste.map(Self.lineCount) ?? 0) lines?", isPresented: pasting, titleVisibility: .visible) {
            Button("Paste") { if let text = pendingPaste { send(Paste.bytes(text, bracketed: false)) } }
        } message: {
            Text("Each line runs as a command as it arrives.")
        }
        .onAppear(perform: open)
        .onDisappear { store.detach(terminalId) }
    }

    private var writable: Bool {
        !store.writesOff && store.attachments[terminalId]?.state == .live
    }

    private var pasting: Binding<Bool> {
        Binding(get: { pendingPaste != nil }, set: { if !$0 { pendingPaste = nil } })
    }

    private func surface(_ session: TerminalSession) -> some View {
        GeometryReader { geo in
            ScrollView([.horizontal, .vertical]) {
                Screen(session: session) { bytes in send(session.sticky.typed(bytes)) }
                    .frame(width: session.surface.width, height: session.surface.height, alignment: .topLeading)
                    .scaleEffect(pinch, anchor: .topLeading)
                    .padding(Space.xs)
            }
            .scrollIndicators(.hidden)
            .defaultScrollAnchor(.topLeading)
            .simultaneousGesture(
                MagnifyGesture()
                    .onChanged { pinch = $0.magnification }
                    .onEnded { value in
                        pinch = 1
                        zoom(to: Fit.zoom(session.fontSize, by: value.magnification), session)
                    }
            )
            .onAppear { area = geo.size }
            .onChange(of: geo.size) { _, size in
                area = size
                if fit { fitNow(session) }
            }
        }
    }

    @ToolbarContentBuilder
    private var menu: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            Menu {
                Toggle(isOn: Binding(get: { fit }, set: { on in
                    fit = on
                    if on, let session { fitNow(session) }
                })) {
                    Label("Fit to Phone", systemImage: "arrow.down.right.and.arrow.up.left")
                }
                Button("Paste", systemImage: "doc.on.clipboard", action: paste).disabled(!writable)
                if let session {
                    Button("Larger Text", systemImage: "textformat.size.larger") { zoom(to: session.fontSize + 1, session) }
                    Button("Smaller Text", systemImage: "textformat.size.smaller") { zoom(to: session.fontSize - 1, session) }
                }
                Divider()
                Button("Close Terminal", systemImage: "xmark.circle", role: .destructive) { confirmClose = true }
                    .disabled(store.writesOff)
            } label: {
                Label("More", systemImage: "ellipsis.circle")
            }
        }
    }

    private func open() {
        let session = session ?? {
            let listed = store.terminal(terminalId)
            return TerminalSession(id: terminalId, cols: listed?.cols ?? 80, rows: listed?.rows ?? 24, fontSize: CGFloat(savedFontSize))
        }()
        self.session = session
        // Already open (the gallery's, or one a reconnect will attach again) keeps its screen.
        if store.attachments[terminalId] == nil { store.attach(terminalId, sink: session) }
    }

    private func press(_ key: TerminalKey, _ session: TerminalSession) {
        send(session.sticky.press(key, applicationCursor: session.applicationCursor))
    }

    private func send(_ bytes: [UInt8]) {
        guard writable else { return }
        store.input(terminalId, Data(bytes))
    }

    private func paste() {
        guard let text = Self.clipboard(), !text.isEmpty, let session else { return }
        if Paste.asks(confirm: true, bracketed: session.bracketedPaste, text) {
            pendingPaste = text
        } else {
            send(Paste.bytes(text, bracketed: session.bracketedPaste))
        }
    }

    private func fitNow(_ session: TerminalSession) {
        guard area.width > 0, area.height > 0 else { return }
        let grid = Fit.grid(CGSize(width: area.width - 2 * Space.xs, height: area.height - 2 * Space.xs), cell: session.cell)
        guard grid.cols != session.cols || grid.rows != session.rows else { return }
        store.resize(terminalId, cols: grid.cols, rows: grid.rows)
    }

    private func zoom(to size: CGFloat, _ session: TerminalSession) {
        session.setFontSize(size)
        savedFontSize = Double(session.fontSize)
        #if os(iOS)
            // The view reflowed its screen to the new cell size; a fresh snapshot redraws it as pocketd has it.
            store.attach(terminalId, sink: session)
        #endif
        if fit { fitNow(session) }
    }

    private func close() {
        Task {
            do {
                try await store.close(terminalId)
                Haptics.success()
                dismiss()
            } catch {
                Haptics.warning()
                problem = "Couldn't close the terminal"
            }
        }
    }

    nonisolated static func lineCount(_ text: String) -> Int {
        text.split(separator: "\n", omittingEmptySubsequences: false).count
    }

    static func clipboard() -> String? {
        #if os(iOS)
            UIPasteboard.general.string
        #else
            NSPasteboard.general.string(forType: .string)
        #endif
    }
}

/// What stands between the phone and a live screen, if anything.
@MainActor
struct Banner: View {
    enum Tone { case neutral, waiting, failed }

    let text: String
    let systemImage: String
    let tone: Tone
    let busy: Bool

    init?(state: TerminalAttachment.State?, writesOff: Bool, problem: String?) {
        let shown: (String, String, Tone, Bool)
        switch state {
        case .attaching?: shown = ("Connecting…", "bolt.horizontal", .neutral, true)
        case .offline?: shown = ("Reconnecting to your Mac…", "wifi.exclamationmark", .waiting, true)
        case .exited(let code)?:
            shown = (code == 0 ? "The shell exited" : "Exited with code \(code)", "stop.circle", code == 0 ? .neutral : .failed, false)
        case .gone?: shown = ("This terminal has ended", "stop.circle", .neutral, false)
        case .live?, nil:
            if let problem {
                shown = (problem, "exclamationmark.triangle", .failed, false)
            } else if writesOff {
                shown = ("View only: terminals are off for phones on this Mac", "eye", .waiting, false)
            } else {
                return nil
            }
        }
        (text, systemImage, tone, busy) = shown
    }

    private var colors: (text: Token, fill: Token) {
        switch tone {
        case .neutral: (Palette.text2, Palette.fill2)
        case .waiting: (Palette.waitingText, Palette.waitingBackground)
        case .failed: (Palette.failedText, Palette.failedBackground)
        }
    }

    var body: some View {
        HStack(spacing: Space.s) {
            if busy { ProgressView().controlSize(.small) } else { Image(systemName: systemImage) }
            Text(text).font(Typography.footnote).lineLimit(2)
            Spacer(minLength: 0)
        }
        .foregroundStyle(colors.text.color)
        .padding(.horizontal, Space.l)
        .padding(.vertical, Space.s)
        .background(colors.fill.color)
    }
}

/// The emulator's drawing: SwiftTerm's view on the phone, its screen as text on the Mac.
@MainActor
struct Screen: View {
    let session: TerminalSession
    let typed: ([UInt8]) -> Void

    var body: some View {
        #if os(iOS)
            PhoneScreen(session: session, typed: typed)
        #else
            let _ = session.revision
            VStack(alignment: .leading, spacing: 0) {
                ForEach(Array(session.lines().enumerated()), id: \.offset) { _, line in
                    Text(line.isEmpty ? " " : line).lineLimit(1).fixedSize()
                }
            }
            .font(.system(size: session.fontSize, design: .monospaced))
            .foregroundStyle(Palette.text.color)
        #endif
    }
}

#if os(iOS)
    @MainActor
    struct PhoneScreen: UIViewRepresentable {
        let session: TerminalSession
        let typed: ([UInt8]) -> Void

        func makeCoordinator() -> Coordinator { Coordinator(typed: typed) }

        func makeUIView(context: Context) -> PhoneTerminalView {
            let view = session.view
            view.terminalDelegate = context.coordinator
            view.nativeBackgroundColor = .clear
            view.nativeForegroundColor = .label
            view.backgroundColor = .clear
            return view
        }

        func updateUIView(_ view: PhoneTerminalView, context: Context) {
            context.coordinator.typed = typed
        }

        final class Coordinator: NSObject, TerminalViewDelegate {
            var typed: ([UInt8]) -> Void
            init(typed: @escaping ([UInt8]) -> Void) { self.typed = typed }

            func send(source: TerminalView, data: ArraySlice<UInt8>) { typed(Array(data)) }
            // pocketd keeps the size; a phone changes it only through Fit to Phone.
            func sizeChanged(source: TerminalView, newCols: Int, newRows: Int) {}
            func setTerminalTitle(source: TerminalView, title: String) {}
            func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}
            func scrolled(source: TerminalView, position: Double) {}
            func requestOpenLink(source: TerminalView, link: String, params: [String: String]) {
                if let url = URL(string: link) { UIApplication.shared.open(url) }
            }
            func clipboardCopy(source: TerminalView, content: Data) {
                if let text = String(data: content, encoding: .utf8) { UIPasteboard.general.string = text }
            }
        }
    }
#endif
