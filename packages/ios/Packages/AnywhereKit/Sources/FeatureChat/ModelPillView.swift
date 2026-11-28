import ChatModel
import DesignSystem
import Domain
import SwiftUI
import Wire

/// The model and its effort as two pills, "Opus 4.5" "High", shortened to what `fit` leaves room for; a menu of both once
/// pocketd takes `agent.settings`.
@MainActor
struct ModelPillView: View {
    let agent: AgentSummary
    let efforts: [String]
    let editable: Bool
    var fit = PillFit.full
    let pick: (_ model: String?, _ effort: String?) -> Void

    var body: some View {
        if editable {
            Menu {
                ModelChoices(agent: agent, efforts: efforts, pick: pick)
            } label: {
                label
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .fixedSize()
        } else {
            label
        }
    }

    private var label: some View {
        let (model, effort) = ModelPill.label(agent)
        return HStack(spacing: 6) {
            PillLabel {
                if fit == .icons { BrandMark(agent.provider, size: 13) } else { Text(model) }
            }
            if fit == .full, let effort { PillLabel { Text(effort) } }
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel([model, effort].compactMap(\.self).joined(separator: " "))
    }
}

/// The model menu, as the desktop's: the effort the model runs on, named as its provider names it, over the models; the
/// agent's own when the phone has neither to offer.
@MainActor
struct ModelChoices: View {
    let agent: AgentSummary
    let efforts: [String]
    let pick: (_ model: String?, _ effort: String?) -> Void

    var body: some View {
        if !efforts.isEmpty {
            Section(AgentName.effort(agent.provider)) {
                ForEach(efforts, id: \.self) { effort in
                    Button {
                        pick(nil, effort)
                    } label: {
                        if agent.effort == effort { Label(ModelPill.effortLabel(effort), lucide: .check) } else { Text(ModelPill.effortLabel(effort)) }
                    }
                }
            }
        }
        let models = ModelPill.aliases(agent.provider)
        if efforts.isEmpty && models.isEmpty {
            Text("Set by \(AgentName.who(agent.provider))")
        }
        if !models.isEmpty {
            Section("Model") {
                ForEach(models, id: \.self) { alias in
                    Button {
                        pick(alias, nil)
                    } label: {
                        if ModelPill.runs(agent, alias) { Label(modelName(alias), lucide: .check) } else { Text(modelName(alias)) }
                    }
                }
            }
        }
    }
}
