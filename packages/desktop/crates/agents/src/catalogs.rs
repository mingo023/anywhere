use crate::{ListedModel, ModelSetting};
use crate::providers::{GROK_EFFORTS, THINKING};
use serde::Deserialize;
use serde_json::Value;

fn listed(value: &str, name: &str, efforts: Vec<String>) -> ListedModel {
    ListedModel { value: value.into(), name: name.into(), resolved: value.into(), efforts, ..ListedModel::default() }
}

/// `model` under who serves it.
fn served_by(upstream: &str, model: ListedModel) -> ListedModel {
    ListedModel { upstream: upstream.into(), ..model }
}

/// The name `v` gives itself under one of `keys`, else the one its `id` reads as.
fn name_of(v: &Value, keys: &[&str], id: &str) -> String {
    Some(text(v, keys)).filter(|n| !n.is_empty()).map_or_else(|| crate::model_name(id), str::to_string)
}

/// The line of `output` that `answers`, as a server prints among its other replies.
fn reply(output: &str, answers: fn(&Value) -> bool) -> Option<Value> {
    output.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).find(answers)
}

/// What a server's answer `line` says instead of a list: ACP's `error.message`, Pi's `error`.
pub(crate) fn refusal(line: &str) -> Option<String> {
    let line = serde_json::from_str::<Value>(line).ok()?;
    let error = &line["error"];
    (!error.is_null()).then(|| error["message"].as_str().or(error.as_str()).unwrap_or_default().to_string())
}

fn owned(efforts: &[&str]) -> Vec<String> {
    efforts.iter().map(|e| e.to_string()).collect()
}

/// The first of `keys` under which `v` holds text.
fn text<'a>(v: &'a Value, keys: &[&str]) -> &'a str {
    keys.iter().find_map(|k| v[*k].as_str().map(str::trim).filter(|s| !s.is_empty())).unwrap_or_default()
}

fn items(v: &Value) -> &[Value] {
    v.as_array().map_or(&[], Vec::as_slice)
}

/// `models` without the later ones of the same value.
fn unique(models: Vec<ListedModel>) -> Vec<ListedModel> {
    let mut out: Vec<ListedModel> = Vec::new();
    for m in models {
        if !out.iter().any(|o| o.value == m.value) {
            out.push(m);
        }
    }
    out
}

fn by_name(mut models: Vec<ListedModel>) -> Vec<ListedModel> {
    models.sort_by_cached_key(|m| m.name.to_lowercase());
    models
}

/// `line` without the colour codes a CLI prints around it.
fn plain(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some((before, code)) = rest.split_once("\x1b[") {
        out.push_str(before);
        rest = code.trim_start_matches(|c: char| c.is_ascii_digit() || c == ';').get(1..).unwrap_or_default();
    }
    out.push_str(rest);
    out.trim().to_string()
}

#[derive(Deserialize)]
struct CodexModel {
    slug: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    visibility: String,
    #[serde(default)]
    supported_reasoning_levels: Vec<CodexLevel>,
    #[serde(default)]
    default_reasoning_level: String,
}

#[derive(Deserialize)]
struct CodexLevel {
    effort: String,
}

/// The models `codex debug models` lists; hidden ones are left out.
pub fn codex(json: &str) -> Vec<ListedModel> {
    #[derive(Deserialize)]
    struct Catalog {
        models: Vec<CodexModel>,
    }
    let Ok(c) = serde_json::from_str::<Catalog>(json) else { return Vec::new() };
    c.models
        .into_iter()
        .filter(|m| m.visibility == "list")
        .map(|m| ListedModel {
            name: if m.display_name.is_empty() { m.slug.clone() } else { m.display_name },
            resolved: m.slug.clone(),
            value: m.slug,
            efforts: m.supported_reasoning_levels.into_iter().map(|l| l.effort).collect(),
            default_effort: m.default_reasoning_level,
            ..ListedModel::default()
        })
        .collect()
}

/// The models Claude Code's answer to `initialize` lists, but "default", which only stands for one of the others.
pub fn claude(output: &str) -> Vec<ListedModel> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct ClaudeModel {
        value: String,
        display_name: String,
        resolved_model: String,
        #[serde(default)]
        supported_effort_levels: Vec<String>,
    }
    #[derive(Deserialize)]
    struct Init {
        models: Vec<ClaudeModel>,
    }
    output
        .lines()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|l| l["type"] == "control_response")
        .and_then(|l| Init::deserialize(&l["response"]["response"]).ok())
        .map_or_else(Vec::new, |init| {
            init.models
                .into_iter()
                .filter(|m| m.value != "default")
                .map(|m| ListedModel { value: m.value, name: m.display_name, resolved: m.resolved_model, efforts: m.supported_effort_levels, ..ListedModel::default() })
                .collect()
        })
}

/// The models Cursor's ACP server answers `cursor/list_available_models` with. It keeps one model for all its variants:
/// what tells them apart, as effort or "Fast", are the model's own `configOptions`, each set by its id once the model is.
pub fn cursor(output: &str) -> Vec<ListedModel> {
    let Some(reply) = reply(output, is_acp_session) else { return Vec::new() };
    let models = items(&reply["result"]["models"]).iter().map(|m| {
        let id = text(m, &["value"]);
        ListedModel { settings: items(&m["configOptions"]).iter().filter_map(setting).collect(), ..listed(id, &name_of(m, &["name"], id), Vec::new()) }
    });
    unique(models.filter(|m| !m.value.is_empty()).collect())
}

/// The models a CLI prints as "id - Name" lines, among headings and tips.
fn lines(output: &str) -> Vec<ListedModel> {
    let models = output.lines().filter_map(|l| {
        let line = plain(l);
        let (id, name) = line.split_once(" - ").or_else(|| line.split_once(" \u{2014} "))?;
        let (id, name) = (id.trim(), name.trim());
        let name = name.strip_suffix("(default)").or_else(|| name.strip_suffix("(Default)")).unwrap_or(name).trim();
        (!id.is_empty() && !id.contains(char::is_whitespace) && !name.is_empty()).then(|| listed(id, name, Vec::new()))
    });
    unique(models.collect())
}

/// The ids `grok models` bullets. It names no efforts, so each takes the ones Grok Build does.
pub fn grok(output: &str) -> Vec<ListedModel> {
    let models = output.lines().filter_map(|l| {
        let line = plain(l);
        let id = line.strip_prefix(['*', '+', '-'])?.split_whitespace().next()?.to_string();
        line[1..].starts_with(char::is_whitespace).then(|| listed(&id, &crate::model_name(&id), owned(GROK_EFFORTS)))
    });
    unique(models.collect())
}

/// Lowest effort first, as OpenCode's own picker sorts them; ones it doesn't know go last, by name.
const VARIANTS: [&str; 9] = ["none", "minimal", "low", "medium", "high", "xhigh", "extra-high", "max", "ultra"];

/// The agents OpenCode runs its own turns as; a session can't be switched to one.
const OPENCODE_OWN_AGENTS: [&str; 3] = ["compaction", "summary", "title"];

/// The agent a session runs as, of the ones `opencode agent list` heads "name (primary)" or "name (all)". "plan" is
/// left to plan mode; ACP calls the rest the session's "mode".
fn opencode_agent(output: &str) -> Option<ModelSetting> {
    let agents = output.lines().filter_map(|l| l.strip_suffix(" (primary)").or_else(|| l.strip_suffix(" (all)"))).filter(|a| *a != "plan" && !OPENCODE_OWN_AGENTS.contains(a));
    let choices: Vec<(String, String)> = agents.map(|a| (a.to_string(), crate::model_name(a))).collect();
    (choices.len() > 1).then(|| ModelSetting { id: "mode".into(), name: "Agent".into(), default: "build".into(), choices })
}

/// The models `opencode models --verbose` prints, each a "provider/model" line above its JSON, by name; a model's
/// variants are its efforts. Each runs as any of the agents `opencode agent list` prints after them.
pub fn opencode(output: &str) -> Vec<ListedModel> {
    let agent = opencode_agent(output);
    let mut models = Vec::new();
    let mut lines = output.lines().peekable();
    while let Some(line) = lines.next() {
        let slug = line.trim_end();
        if line.starts_with(|c: char| c.is_whitespace() || c == '{') || slug.contains(char::is_whitespace) || !slug.contains('/') {
            continue;
        }
        let mut json = String::new();
        while let Some(body) = lines.next_if(|l| l.starts_with(|c: char| c.is_whitespace() || c == '{' || c == '}')) {
            json.push_str(body);
        }
        let Ok(model) = serde_json::from_str::<Value>(&json) else { continue };
        let mut efforts: Vec<String> = model["variants"].as_object().into_iter().flat_map(|v| v.keys().cloned()).collect();
        efforts.sort_by_cached_key(|e| (VARIANTS.iter().position(|v| *v == e.to_lowercase()).unwrap_or(VARIANTS.len()), e.clone()));
        let upstream = slug.split_once('/').map_or("", |(who, _)| who);
        models.push(ListedModel { settings: agent.iter().cloned().collect(), ..served_by(upstream, listed(slug, &name_of(&model, &["name"], slug), efforts)) });
    }
    by_name(unique(models))
}

fn is_pi_models(line: &Value) -> bool {
    line["command"] == "get_available_models"
}

/// Whether `line` is Pi's or omp's answer to `get_available_models`.
pub fn pi_answered(line: &str) -> bool {
    reply(line, is_pi_models).is_some()
}

/// The models Pi's and omp's `get_available_models` answers, as the "provider/id" `--model` takes, by name; the ones
/// that reason take the thinking levels.
pub fn pi(output: &str) -> Vec<ListedModel> {
    let Some(reply) = reply(output, is_pi_models) else { return Vec::new() };
    let models = items(&reply["data"]["models"]).iter().filter_map(|m| {
        let (id, provider) = (text(m, &["id"]), text(m, &["provider"]));
        let offered = !id.is_empty() && !provider.is_empty() && !(provider == "github-copilot" && copilot_hides(id));
        offered.then(|| served_by(provider, listed(&format!("{provider}/{id}"), text(m, &["name", "id"]), if m["reasoning"] == true { owned(THINKING) } else { Vec::new() })))
    });
    by_name(unique(models.collect()))
}

/// omp's models, each with its "Fast": the priority tier of who serves the model. Its list doesn't say which models
/// have one, so every model offers it and omp says when one can't.
pub fn omp(output: &str) -> Vec<ListedModel> {
    let fast = ModelSetting { id: "fast".into(), name: "Fast".into(), choices: vec![("false".into(), "Off".into()), ("true".into(), "On".into())], default: "false".into() };
    pi(output).into_iter().map(|m| ListedModel { settings: vec![fast.clone()], ..m }).collect()
}

/// Copilot lists models that only its own tools run, and dated twins of the ones it offers.
fn copilot_hides(id: &str) -> bool {
    let dated = |prefix: &str| id.strip_prefix(prefix).is_some_and(|rest| rest.len() > 4 && rest.as_bytes()[4] == b'-' && rest[..4].bytes().all(|b| b.is_ascii_digit()));
    ["exec-agent-", "copilot-search-", "gpt-3.5-turbo", "gpt-4o-mini"].iter().any(|p| id.starts_with(p))
        || ["trajectory-compaction", "gpt-4", "gpt-4-0613", "gpt-4-o-preview"].contains(&id)
        || dated("gpt-4o-")
        || dated("gpt-4.1-")
}

fn is_acp_session(line: &Value) -> bool {
    line["id"] == 2
}

/// Whether `line` is an ACP agent's reply to the `session/new` that `providers::ACP_INPUT` sends.
pub fn acp_answered(line: &str) -> bool {
    reply(line, is_acp_session).is_some()
}

/// One of an ACP session's `availableModels`. Hermes names each "<who serves it> · <id>", which a list cuts short to
/// the same words for every model of one upstream: that one reads as its id does, under who serves it.
fn acp_model(m: &Value) -> ListedModel {
    let id = text(m, &["modelId"]);
    let name = text(m, &["name", "modelId"]);
    let served = name.strip_suffix(id.split_once(':').map_or(id, |(_, model)| model)).and_then(|who| who.strip_suffix(" \u{b7} "));
    served.map_or_else(|| listed(id, name, Vec::new()), |who| served_by(who, listed(id, &crate::model_name(id), Vec::new())))
}

/// A config option's choices, with the ones it groups.
fn choices(option: &Value) -> Vec<(&str, &str)> {
    items(&option["options"]).iter().flat_map(|c| if c["options"].is_array() { choices(c) } else { vec![(text(c, &["value"]), text(c, &["name", "label", "value"]))] }).filter(|(value, _)| !value.is_empty()).collect()
}

/// A config option as a setting, when it has a choice to make.
fn setting(option: &Value) -> Option<ModelSetting> {
    let id = text(option, &["id"]);
    let choices: Vec<(String, String)> = choices(option).into_iter().map(|(value, name)| (value.to_string(), name.to_string())).collect();
    (!id.is_empty() && choices.len() > 1).then(|| ModelSetting { id: id.into(), name: text(option, &["name", "id"]).into(), default: text(option, &["currentValue"]).into(), choices })
}

/// The models an ACP agent's reply to `session/new` offers: its model option's choices, each taking the thought levels
/// when it has two or more, else its `models` with the one it's on first.
pub fn acp(output: &str) -> Vec<ListedModel> {
    let Some(reply) = reply(output, is_acp_session) else { return Vec::new() };
    let result = &reply["result"];
    let options = items(&result["configOptions"]);
    let picks = |o: &&Value| o["type"] != "boolean" && (o["id"] == "model" || (o["category"] == "model" && o["id"] != "provider"));
    let model = options.iter().find(|o| picks(o) && o["id"] == "model").or_else(|| options.iter().find(picks));
    let efforts: Vec<String> = options.iter().filter(|o| o["category"] == "thought_level").map(|o| choices(o).into_iter().map(|(v, _)| v.to_string()).collect()).find(|e: &Vec<String>| e.len() > 1).unwrap_or_default();
    let offered: Vec<ListedModel> = model.map(choices).unwrap_or_default().into_iter().map(|(value, name)| listed(value, name, efforts.clone())).collect();
    if !offered.is_empty() {
        return unique(offered);
    }
    let mut models = unique(items(&result["models"]["availableModels"]).iter().map(acp_model).filter(|m| !m.value.is_empty()).collect());
    if let Some(current) = models.iter().position(|m| m.value == text(&result["models"], &["currentModelId"])) {
        models[..=current].rotate_right(1);
    }
    models
}

/// The models `fx models --json` lists, as ids or as objects, under `ids`, `models` or `data` or as the array itself;
/// "id - Name" lines too, from an fx that prints no JSON. fx isn't installed where this was written, so it reads
/// every shape MonoCode's `fxProtocol.ts` does and no other.
pub fn fx(output: &str) -> Vec<ListedModel> {
    let json = output.find(['{', '[']).and_then(|at| serde_json::from_str::<Value>(output[at..].trim()).ok());
    let Some(json) = json else { return lines(output) };
    let list = ["models", "ids", "data"].iter().map(|k| &json[*k]).find(|v| v.is_array()).unwrap_or(&json);
    let models = items(list).iter().filter_map(|m| {
        let id = m.as_str().map(str::trim).unwrap_or_else(|| text(m, &["id", "modelId", "model_id", "model", "value"]));
        let name = name_of(m, &["name", "displayName", "title"], id);
        let efforts: Vec<String> = ["effortOptions", "effort_options", "efforts", "supportedEffort"].iter().find_map(|k| m[*k].as_array()).into_iter().flatten().map(|e| e.as_str().unwrap_or_else(|| text(e, &["value"])).to_string()).filter(|e| !e.is_empty()).collect();
        (!id.is_empty()).then(|| listed(id, &name, if efforts.len() > 1 { efforts } else { Vec::new() }))
    });
    unique(models.collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(models: &[ListedModel]) -> Vec<(&str, &str, Vec<&str>)> {
        models.iter().map(|m| (m.value.as_str(), m.name.as_str(), m.efforts.iter().map(String::as_str).collect())).collect()
    }

    #[test]
    fn codex_lists_its_shown_models_by_name_and_nothing_from_bad_output() {
        let json = r#"{"models":[{"slug":"gpt-6-sol","display_name":"GPT-6-Sol","visibility":"list","default_reasoning_level":"ultra","supported_reasoning_levels":[{"effort":"low","description":"Fast"},{"effort":"ultra","description":"Most"}]},{"slug":"gpt-reserve","display_name":"GPT-Reserve","visibility":"hide"},{"slug":"gpt-5.6-luna","visibility":"list"}]}"#;
        let listed: Vec<_> = codex(json).into_iter().map(|m| (m.value, m.name, m.resolved, m.efforts, m.default_effort)).collect();
        let s = |v: &str| v.to_string();
        assert_eq!(listed, [(s("gpt-6-sol"), s("GPT-6-Sol"), s("gpt-6-sol"), vec![s("low"), s("ultra")], s("ultra")), (s("gpt-5.6-luna"), s("gpt-5.6-luna"), s("gpt-5.6-luna"), vec![], s(""))]);
        assert!(codex("error: not logged in").is_empty());
    }

    #[test]
    fn claude_lists_what_initialize_answers_but_its_default() {
        let out = r#"{"type":"system","subtype":"init"}
{"type":"control_response","response":{"subtype":"success","request_id":"m","response":{"commands":[],"models":[{"value":"default","resolvedModel":"claude-opus-5-5","displayName":"Default (recommended)","supportedEffortLevels":["low"]},{"value":"fable","resolvedModel":"claude-fable-5-1","displayName":"Fable 5.1","supportsEffort":true,"supportedEffortLevels":["low","max"]},{"value":"claude-haiku-4-5-20251001","resolvedModel":"claude-haiku-4-5-20251001","displayName":"Haiku 4.5"}]}}}"#;
        let listed: Vec<_> = claude(out).into_iter().map(|m| (m.value, m.name, m.resolved, m.efforts)).collect();
        let s = |v: &str| v.to_string();
        assert_eq!(listed, [(s("fable"), s("Fable 5.1"), s("claude-fable-5-1"), vec![s("low"), s("max")]), (s("claude-haiku-4-5-20251001"), s("Haiku 4.5"), s("claude-haiku-4-5-20251001"), vec![])]);
        assert!(claude("Error: not logged in").is_empty());
    }

    #[test]
    fn pi_lists_provider_slash_id_by_name_and_only_reasoning_models_think() {
        let out = r#"{"type":"response","command":"get_state","success":true,"data":{"model":{"id":"deepseek-v4-pro","provider":"deepseek"}}}
{"type":"response","command":"get_available_models","success":true,"data":{"models":[{"id":"amazon.nova-lite-v1:0","name":"Nova Lite","api":"bedrock-converse-stream","provider":"amazon-bedrock","reasoning":false,"contextWindow":300000,"type":"chat"},{"id":"deepseek-v4-pro","name":"DeepSeek V4 Pro","provider":"deepseek","reasoning":true,"thinkingLevelMap":{"off":null}},{"id":"deepseek-v4-pro","name":"Twin","provider":"deepseek","reasoning":true},{"id":"unnamed","provider":"deepseek","reasoning":false},{"id":"gpt-4o-2024-11-20","name":"GPT-4o","provider":"github-copilot","reasoning":false},{"id":"gpt-4o","name":"GPT-4o","provider":"github-copilot","reasoning":false},{"name":"No id","provider":"deepseek"}]}}"#;
        assert!(!pi_answered(out.lines().next().unwrap()) && pi_answered(out.lines().nth(1).unwrap()));
        assert_eq!(
            shown(&pi(out)),
            [
                ("deepseek/deepseek-v4-pro", "DeepSeek V4 Pro", THINKING.to_vec()),
                ("github-copilot/gpt-4o", "GPT-4o", vec![]),
                ("amazon-bedrock/amazon.nova-lite-v1:0", "Nova Lite", vec![]),
                ("deepseek/unnamed", "unnamed", vec![]),
            ]
        );
        assert_eq!(pi(out).iter().map(|m| m.upstream.as_str()).collect::<Vec<_>>(), ["deepseek", "github-copilot", "amazon-bedrock", "deepseek"]);
        assert!(pi("Error: no such mode").is_empty());
    }

    #[test]
    fn hermes_lists_its_session_s_models_by_their_ids_under_who_serves_each_with_the_one_it_is_on_first() {
        let out = r#"{"jsonrpc":"2.0","id":1,"result":{"agentCapabilities":{"loadSession":true},"agentInfo":{"name":"hermes-agent","version":"0.22.5"},"protocolVersion":1}}
{"jsonrpc":"2.0","id":2,"result":{"_meta":{"hermes":{"sessionProvenance":{"sessionKind":"root"}}},"models":{"availableModels":[{"description":"Provider: Mixture of Agents","modelId":"moa:default","name":"Mixture of Agents · default"},{"description":"Provider: ChatGPT or Codex Subscription","modelId":"openai-codex:gpt-6.1-sol","name":"ChatGPT or Codex Subscription · gpt-6.1-sol"},{"description":"Provider: ChatGPT or Codex Subscription • current","modelId":"openai-codex:gpt-6-sol","name":"ChatGPT or Codex Subscription · gpt-6-sol"},{"modelId":"openai-codex:gpt-6-sol","name":"Twin"},{"modelId":"bare"}],"currentModelId":"openai-codex:gpt-6-sol"},"modes":{"availableModes":[{"id":"default","name":"Default"}],"currentModeId":"default"},"sessionId":"local-13bf"}}
{"jsonrpc":"2.0","method":"session/update","params":{"sessionId":"local-13bf","update":{"availableCommands":[],"sessionUpdate":"available_commands_update"}}}"#;
        let answers: Vec<bool> = out.lines().map(acp_answered).collect();
        assert_eq!(answers, [false, true, false]);
        assert_eq!(
            shown(&acp(out)),
            [
                ("openai-codex:gpt-6-sol", "GPT 6 Sol", vec![]),
                ("moa:default", "Default", vec![]),
                ("openai-codex:gpt-6.1-sol", "GPT 6.1 Sol", vec![]),
                ("bare", "bare", vec![]),
            ]
        );
        assert_eq!(acp(out).iter().map(|m| m.upstream.as_str()).collect::<Vec<_>>(), ["ChatGPT or Codex Subscription", "Mixture of Agents", "ChatGPT or Codex Subscription", ""]);
        assert!(acp(r#"{"jsonrpc":"2.0","id":2,"error":{"code":-32000,"message":"Authentication required"}}"#).is_empty());
    }

    #[test]
    fn an_acp_model_named_in_its_own_words_keeps_them() {
        let out = r#"{"jsonrpc":"2.0","id":2,"result":{"models":{"availableModels":[{"modelId":"gemini-3.8-flash-high","name":"Gemini 3.8 Flash (High)"},{"modelId":"a:b","name":"Fast · cheap"}]}}}"#;
        assert_eq!(shown(&acp(out)), [("gemini-3.8-flash-high", "Gemini 3.8 Flash (High)", vec![]), ("a:b", "Fast · cheap", vec![])]);
        assert!(acp(out).iter().all(|m| m.upstream.is_empty()));
    }

    #[test]
    fn an_acp_agent_s_model_option_lists_its_choices_grouped_or_not_with_its_thought_levels() {
        let opencode = r#"{"jsonrpc":"2.0","id":2,"result":{"sessionId":"ses_e371","configOptions":[{"id":"model","name":"Model","category":"model","type":"select","currentValue":"opencode/big-pickle","options":[{"value":"opencode/beam-free","name":"OpenCode/Beam Free"},{"value":"opencode/big-pickle","name":"OpenCode/Big Pickle"}]},{"id":"mode","name":"Session Mode","category":"mode","type":"select","currentValue":"build","options":[{"value":"build","name":"build"},{"value":"plan","name":"plan"}]}]}}"#;
        assert_eq!(shown(&acp(opencode)), [("opencode/beam-free", "OpenCode/Beam Free", vec![]), ("opencode/big-pickle", "OpenCode/Big Pickle", vec![])]);
        let antigravity = r#"{"jsonrpc":"2.0","id":2,"result":{"configOptions":[{"id":"fast","category":"model","type":"boolean","currentValue":false},{"id":"provider","category":"model","options":[{"value":"google","name":"Google"}]},{"id":"llm","category":"model","options":[{"group":"provider","options":[{"value":"m1","name":"Model One"},{"value":"m2"}]}]},{"id":"depth","category":"thought_level","currentValue":"high","options":[{"value":"only","name":"Only"}]},{"id":"thinking","category":"thought_level","currentValue":"high","options":[{"value":"low","name":"Low"},{"value":"high","name":"High"}]}],"models":{"availableModels":[{"modelId":"legacy","name":"Legacy"}]}}}"#;
        assert_eq!(shown(&acp(antigravity)), [("m1", "Model One", vec!["low", "high"]), ("m2", "m2", vec!["low", "high"])]);
    }

    #[test]
    fn opencode_lists_each_slug_by_its_json_name_with_variants_sorted_low_to_high() {
        let out = r#"opencode/big-pickle
{
  "id": "big-pickle",
  "providerID": "opencode",
  "name": "Big Pickle",
  "api": {
    "url": "https://opencode.ai/inference/openai/v1"
  },
  "variants": {
    "max": {
      "reasoningEffort": "max"
    },
    "turbo": {},
    "none": {},
    "high": {
      "reasoningEffort": "high"
    },
    "xhigh": {}
  }
}
opencode/broken
{
  "id": "broken",
anthropic/claude-sonnet-4-5
{
  "id": "claude-sonnet-4-5",
  "variants": {}
}
openai/alpha
{ "id": "alpha", "name": "Alpha" }
"#;
        assert_eq!(
            shown(&opencode(out)),
            [("openai/alpha", "Alpha", vec![]), ("opencode/big-pickle", "Big Pickle", vec!["none", "high", "xhigh", "max", "turbo"]), ("anthropic/claude-sonnet-4-5", "Sonnet 4.5", vec![])]
        );
        assert_eq!(opencode(out).iter().map(|m| m.upstream.as_str()).collect::<Vec<_>>(), ["openai", "opencode", "anthropic"]);
        assert!(opencode("Error: unknown command models").is_empty());
    }

    type Shown<'a> = (&'a str, &'a str, &'a str, Vec<(&'a str, &'a str)>);

    fn settings(model: &ListedModel) -> Vec<Shown<'_>> {
        model.settings.iter().map(|s| (s.id.as_str(), s.name.as_str(), s.default.as_str(), s.choices.iter().map(|(v, n)| (v.as_str(), n.as_str())).collect())).collect()
    }

    #[test]
    fn cursor_lists_one_model_for_its_variants_with_what_tells_them_apart_as_its_settings() {
        let out = r#"{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1}}
{"jsonrpc":"2.0","id":2,"result":{"models":[{"value":"composer-2.5","name":"Composer 2.5","configOptions":[]},{"value":"gpt-5.4","name":"GPT-5.4","configOptions":[{"id":"reasoning","name":"Reasoning","category":"thought_level","type":"select","currentValue":"medium","options":[{"value":"low","name":"Low"},{"value":"medium","name":"Medium"},{"value":"high","name":"High"}]},{"id":"fast","name":"Fast","category":"model_config","type":"select","currentValue":"false","options":[{"value":"false","name":"Off"},{"value":"true","name":"On"}]},{"id":"context","name":"Context","category":"model_config","type":"select","currentValue":"272k","options":[{"value":"272k","name":"272K"}]}]},{"value":"gpt-5.4","name":"Twin"},{"value":"unnamed"},{"name":"No value"}]}}"#;
        let models = cursor(out);
        assert_eq!(shown(&models), [("composer-2.5", "Composer 2.5", vec![]), ("gpt-5.4", "GPT-5.4", vec![]), ("unnamed", "Unnamed", vec![])]);
        assert_eq!(settings(&models[0]), []);
        assert_eq!(settings(&models[1]), [("reasoning", "Reasoning", "medium", vec![("low", "Low"), ("medium", "Medium"), ("high", "High")]), ("fast", "Fast", "false", vec![("false", "Off"), ("true", "On")])], "a setting with one value is no choice");
        assert!(cursor("auto - Auto (default)\n").is_empty());
    }

    #[test]
    fn every_omp_model_offers_fast_and_pi_s_offer_none() {
        let out = r#"{"type":"response","command":"get_available_models","success":true,"data":{"models":[{"id":"gpt-5.5","name":"GPT-5.5","provider":"openai","reasoning":true},{"id":"glm-5","name":"GLM 5","provider":"zai"}]}}"#;
        let models = omp(out);
        assert_eq!(shown(&models), shown(&pi(out)));
        assert!(models.iter().all(|m| settings(m) == [("fast", "Fast", "false", vec![("false", "Off"), ("true", "On")])]));
        assert!(pi(out).iter().all(|m| m.settings.is_empty()));
    }

    #[test]
    fn opencode_models_run_as_any_agent_a_session_can_but_plan() {
        let models = "opencode/big-pickle\n{\n  \"name\": \"Big Pickle\"\n}\n";
        let agents = "build (primary)\n  [\n  {\n    \"permission\": \"*\"\n  }\n]\ncompaction (primary)\n]\nexplore (subagent)\nplan (primary)\nsummary (primary)\ntitle (primary)\ndocs-writer (all)\nhelper (subagent)\nreview (primary)\n]\n";
        let listed = opencode(&format!("{models}{agents}"));
        assert_eq!(shown(&listed), [("opencode/big-pickle", "Big Pickle", vec![])]);
        assert_eq!(settings(&listed[0]), [("mode", "Agent", "build", vec![("build", "Build"), ("docs-writer", "Docs Writer"), ("review", "Review")])]);
        let own = "build (primary)\nplan (primary)\ntitle (primary)\n";
        assert!(opencode(&format!("{models}{own}"))[0].settings.is_empty(), "its own agents alone leave nothing to pick");
        assert!(opencode(models)[0].settings.is_empty());
    }

    #[test]
    fn fx_without_json_lists_every_id_dash_name_line_without_its_colours_or_default_mark() {
        let out = "Available models\n\n\x1b[1mauto\x1b[0m - Auto (default)\ncomposer-2.5 - Composer 2.5\ngpt-5.4-high-fast - GPT-5.4 High Fast\ncomposer-2.5 - Twin\n\nTip: use --model <id> to pick one - or not\n";
        assert_eq!(shown(&fx(out)), [("auto", "Auto", vec![]), ("composer-2.5", "Composer 2.5", vec![]), ("gpt-5.4-high-fast", "GPT-5.4 High Fast", vec![])]);
    }

    #[test]
    fn grok_lists_its_bulleted_ids_with_grok_build_s_efforts() {
        let out = "Available models:\n  \x1b[32m*\x1b[0m grok-4.6 (default)\n  - grok-4.5\n  - grok-4.6\n--help for more\n";
        assert_eq!(shown(&grok(out)), [("grok-4.6", "Grok 4.6", GROK_EFFORTS.to_vec()), ("grok-4.5", "Grok 4.5", GROK_EFFORTS.to_vec())]);
    }

    #[test]
    fn fx_lists_ids_objects_or_lines() {
        assert_eq!(shown(&fx(r#"{"kind":"models","count":2,"ids":["zai/glm-5.2-fast","openai/gpt-5.2"]}"#)), [("zai/glm-5.2-fast", "GLM 5.2 Fast", vec![]), ("openai/gpt-5.2", "GPT 5.2", vec![])]);
        let objects = r#"warming up
{"models":[{"id":"zai/glm-5.2-fast","name":"GLM 5.2 Fast","contextWindow":202752,"efforts":["low",{"value":"high"}]},{"id":"zai/glm-4.7","effortOptions":["auto"]},{"name":"No id"}]}"#;
        assert_eq!(shown(&fx(objects)), [("zai/glm-5.2-fast", "GLM 5.2 Fast", vec!["low", "high"]), ("zai/glm-4.7", "GLM 4.7", vec![])]);
        assert_eq!(shown(&fx("zai/glm-5.2-fast - GLM 5.2 Fast (default)\nopenai/gpt-5.4 - GPT-5.4\n")), [("zai/glm-5.2-fast", "GLM 5.2 Fast", vec![]), ("openai/gpt-5.4", "GPT-5.4", vec![])]);
        assert!(fx("").is_empty());
    }

    #[test]
    fn fx_reads_each_key_monocode_reads_a_model_under() {
        let named = [("id", "name"), ("modelId", "displayName"), ("model_id", "title"), ("model", "name"), ("value", "name")];
        for list in ["models", "ids", "data"] {
            for (id, name) in named {
                let out = serde_json::json!({list: [{id: "zai/glm-4.7", name: "GLM"}]}).to_string();
                assert_eq!(shown(&fx(&out)), [("zai/glm-4.7", "GLM", vec![])], "{list} {id} {name}");
            }
        }
        assert_eq!(shown(&fx(r#"["zai/glm-4.7"]"#)), [("zai/glm-4.7", "GLM 4.7", vec![])]);
        for efforts in ["effortOptions", "effort_options", "efforts", "supportedEffort"] {
            let out = serde_json::json!([{"id": "m", "name": "M", efforts: ["low", "high"]}]).to_string();
            assert_eq!(shown(&fx(&out)), [("m", "M", vec!["low", "high"])], "{efforts}");
        }
    }
}
