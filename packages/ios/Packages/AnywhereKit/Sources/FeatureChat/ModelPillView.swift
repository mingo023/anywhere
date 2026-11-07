import ChatModel
import DesignSystem
import SwiftUI
import Wire

/// The model and its effort, "Opus 4.5 High"; a menu of both once pocketd takes `agent.settings`.
@MainActor
struct ModelPillView: View {
    let agent: AgentSummary
    let efforts: [String]
    let editable: Bool
    let pick: (_ model: String?, _ effort: String?) -> Void

    private var opens: Bool { editable && !(efforts.isEmpty && ModelPill.aliases(agent.provider).isEmpty) }

    var body: some View {
        if opens {
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
        return PillLabel(opens: opens) {
            BrandMark(agent.provider, size: 13)
            Text(model).foregroundStyle(Palette.text.color)
            if let effort { Text(effort).fontWeight(.regular).foregroundStyle(Palette.text3.color) }
        }
    }
}

/// The model menu, as the desktop's: the effort the model runs on, named as its provider names it, over the models.
@MainActor
struct ModelChoices: View {
    let agent: AgentSummary
    let efforts: [String]
    let pick: (_ model: String?, _ effort: String?) -> Void

    var body: some View {
        if !efforts.isEmpty {
            Section(ModelPill.effortWord(agent.provider)) {
                ForEach(efforts, id: \.self) { effort in
                    Button {
                        pick(nil, effort)
                    } label: {
                        if agent.effort == effort { Label(ModelPill.effortLabel(effort), systemImage: "checkmark") } else { Text(ModelPill.effortLabel(effort)) }
                    }
                }
            }
        }
        let models = ModelPill.aliases(agent.provider)
        if !models.isEmpty {
            Section("Model") {
                ForEach(models, id: \.self) { alias in
                    Button {
                        pick(alias, nil)
                    } label: {
                        if ModelPill.runs(agent, alias) { Label(modelName(alias), systemImage: "checkmark") } else { Text(modelName(alias)) }
                    }
                }
            }
        }
    }
}
