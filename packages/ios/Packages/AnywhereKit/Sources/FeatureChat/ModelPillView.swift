import ChatModel
import DesignSystem
import SwiftUI
import Wire

/// The model and its effort, "Opus 4.5 · Extra high"; a menu of both once pocketd takes `agent.settings`.
@MainActor
struct ModelPillView: View {
    let agent: AgentSummary
    let efforts: [String]
    let editable: Bool
    let pick: (_ model: String?, _ effort: String?) -> Void

    var body: some View {
        if editable {
            Menu {
                ModelChoices(agent: agent, efforts: efforts, side: nil, pick: pick)
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
        return HStack(spacing: 5) {
            BrandMark(agent.provider, size: 11)
            Text(model).foregroundStyle(Palette.text2.color)
            if let effort { Text(effort).foregroundStyle(Palette.text4.color) }
            if editable { Image(systemName: "chevron.up.chevron.down").font(.system(size: 8, weight: .semibold)).foregroundStyle(Palette.text4.color) }
        }
        .font(ChatFont.caption)
        .lineLimit(1)
        .padding(.horizontal, 8)
        .frame(height: 28)
        .contentShape(Rectangle())
    }
}

/// The model menu's two lists; `side` shows only one, as `/model` and `/effort` open them.
@MainActor
struct ModelChoices: View {
    let agent: AgentSummary
    let efforts: [String]
    let side: ChatModel.Local?
    let pick: (_ model: String?, _ effort: String?) -> Void

    var body: some View {
        if side != .effort {
            Section("Model") {
                ForEach(ModelPill.aliases(agent.provider), id: \.self) { alias in
                    Button {
                        pick(alias, nil)
                    } label: {
                        if ModelPill.runs(agent, alias) { Label(modelName(alias), systemImage: "checkmark") } else { Text(modelName(alias)) }
                    }
                }
            }
        }
        if side != .model {
            Section("Effort") {
                if efforts.isEmpty {
                    Text("Set by \(who(agent.provider))")
                } else {
                    ForEach(efforts, id: \.self) { effort in
                        Button {
                            pick(nil, effort)
                        } label: {
                            if agent.effort == effort { Label(ModelPill.effortLabel(effort), systemImage: "checkmark") } else { Text(ModelPill.effortLabel(effort)) }
                        }
                    }
                }
            }
        }
    }
}
