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

    init(model: PairingModel) {
        self.model = model
        _host = State(initialValue: model.knownHost ?? "")
    }

    private var link: PairingLink? { PairingEntry.link(host: host, code: code) }

    var body: some View {
        Form {
            Section {
                TextField("100.101.102.103", text: $host)
                    .plainEntry()
                    .accessibilityLabel("Mac address")
            } header: {
                Text("Mac address")
            } footer: {
                Text("The Mac's Tailscale address or name. Port 4517 is added if you leave it out.")
            }
            Section {
                TextField("22 letters and digits", text: $code)
                    .font(Typography.code)
                    .plainEntry()
                    .onSubmit(next)
                    .accessibilityLabel("Pairing code")
            } header: {
                Text("Code")
            } footer: {
                Text("Your Mac shows it under the QR code.")
            }
            Section {
                PasteButton(payloadType: String.self) { strings in
                    Task { @MainActor in pasteFound = strings.first.map { model.open($0) } ?? false }
                }
                .labelStyle(.titleAndIcon)
            } footer: {
                Text(pasteFound ? "Paste the pairing link or the line pocketd pair prints." : "There's no pairing code in what you copied.")
                    .foregroundStyle(pasteFound ? Palette.text3.color : Palette.failedText.color)
            }
        }
        .formStyle(.grouped)
        .navigationTitle("Enter code")
        .toolbar {
            ToolbarItem(placement: .confirmationAction) {
                Button("Next", action: next).disabled(link == nil)
            }
        }
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
