import Client
import DesignSystem
import Domain
import Stores
import SwiftUI

/// Pairs this phone with a Mac: scan its code, paste its link or type its code, then confirm.
@MainActor
public struct PairingView: View {
    @State private var model: PairingModel
    let onPaired: (@MainActor () -> Void)?
    @Environment(\.dismiss) private var dismiss
    @Environment(\.isPresented) private var isPresented

    /// `link` opens straight at the confirm screen; `knownHost` at the code entry, for a Mac that revoked this phone.
    public init(app: AppModel, link: URL? = nil, knownHost: String? = nil, onPaired: (@MainActor () -> Void)? = nil) {
        _model = State(initialValue: PairingModel(app: app, link: link, knownHost: knownHost))
        self.onPaired = onPaired
    }

    init(model: PairingModel) {
        _model = State(initialValue: model)
        onPaired = nil
    }

    public var body: some View {
        NavigationStack(path: $model.path) {
            WelcomeView(model: model)
                .navigationDestination(for: PairingStep.self) { step in
                    switch step {
                    case .scan: ScanView(model: model)
                    case .enter: EnterCodeView(model: model)
                    case .confirm(let link): ConfirmView(model: model, link: link)
                    }
                }
        }
        .environment(\.closePairing, ClosePairing(owner: ObjectIdentifier(model), presented: isPresented) { if isPresented { dismiss() } else { model.path = [] } })
        .onChange(of: model.phase) { _, phase in
            if case .paired = phase { onPaired?() }
        }
    }
}
