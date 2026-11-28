import Client
import DesignSystem
import SwiftUI

/// The Mac's address and code, typed or pasted.
@MainActor
struct EnterCodeView: View {
    let model: PairingModel
    @State private var host: String
    @State private var code = ""
    @State private var pasteFound = true

    init(model: PairingModel, code: String = "") {
        self.model = model
        _host = State(initialValue: model.knownHost ?? "")
        _code = State(initialValue: code)
    }

    private var link: PairingLink? { PairingEntry.link(host: host, code: code) }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                GroupHeader("Machine address")
                field(TextField("100.101.102.103", text: $host).font(.system(size: 16, design: .monospaced)).accessibilityLabel("Machine address"))
                Footnote("Tailscale address or name. Port 4517 is added automatically.")
                GroupHeader("Code")
                field(
                    TextField("22 letters and digits", text: $code).font(.system(size: 17, design: .monospaced)).onSubmit(next).accessibilityLabel("Pairing code"))
                Footnote("Shown under the QR code on your Mac.")
                PasteButton(payloadType: String.self) { strings in
                    Task { @MainActor in pasteFound = strings.first.map { model.open($0) } ?? false }
                }
                .labelStyle(.titleAndIcon)
                .padding(.horizontal, Inset.margin)
                .padding(.top, Space.xl)
                Footnote(pasteFound ? "Paste the pairing link or the line pocketd pair prints." : "There's no pairing code in what you copied.", failed: !pasteFound)
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .safeAreaInset(edge: .bottom) {
            Button("Next", action: next)
                .buttonStyle(LargeButtonStyle())
                .disabled(link == nil)
                .padding(.horizontal, 20)
                .padding(.bottom, Space.s)
        }
        .screenBackground()
        .pairingBar("Enter code")
    }

    private func field(_ field: some View) -> some View {
        field
            .textFieldStyle(.plain)
            .plainEntry()
            .foregroundStyle(Palette.text.color)
            .padding(.horizontal, Inset.margin)
            .frame(height: HitTarget.minimum)
            .insetGroup()
    }

    private func next() {
        guard let link else { return }
        Haptics.tap()
        model.confirm(link)
    }
}

extension View {
    /// An entry field for addresses and codes: no capitals, corrections or smart punctuation.
    @ViewBuilder
    func plainEntry() -> some View {
        #if os(iOS)
            textInputAutocapitalization(.never).autocorrectionDisabled().keyboardType(.asciiCapable).submitLabel(.next)
        #else
            autocorrectionDisabled()
        #endif
    }
}
