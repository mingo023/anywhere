# Antigravity questions

PR 1 of `docs/orchestrators/research/22-antigravity-parity.md` (§2.1, §2.6, §4 item 1). pocketd only. `dk/` is `packages/desktop/crates/`.

## Why

Antigravity (`agy acp`) asks the user questions in two ways, and Anywhere mishandles both:

1. **Decisions** come as `session/request_permission` with a `toolCall.toolCallId` that starts with `interaction_`. Each choice is an option, and every option has kind `allow_once`. Under auto and full access, `auto()` (`flavor.go`) answers with the first option, so the agent's question is answered without the user ever seeing it. Under ask and edits access the user gets an Allow/Deny card, and Allow means the first choice.
2. **Form questions** use `elicitation/create`, but agy only sends it to a client that advertises `clientCapabilities.elicitation.form`. Ours doesn't, so agy writes the question into the chat as plain text, and the user has to type a reply.

Both should open the question card that Claude's AskUserQuestion, Cursor's and Grok's questions already use.

## Evidence and confidence

Tiers are the research doc's: [B] the agy 1.5.16 binary, read with `strings`; [C] a third party's adapter code; [CT] that adapter's test fixtures, which are mocks and not captured traffic. Nothing here was run against a live agy.

In code, comments, tests and commits, cite evidence by tier only and never name the third party.

| Claim | Tier | Confidence |
|---|---|---|
| Method name `elicitation/create` | [B] | high |
| Capability `{"elicitation":{"form":{}}}` turns it on; without it, or after a rejection, agy asks in chat | [B] strings, [C] | high |
| Request shape: `mode:"form"`, `requestedSchema.properties` keyed `q1`…`qN`, choices in `oneOf:[{const,title}]`, optional `qN_other` free-text companion | [CT] | medium |
| Multi-select is `type:"array"` whose `items` hold `enum` or `anyOf` | [CT] | medium |
| Accept reply sends the option's `const`, not its title: `{"action":"accept","content":{"q1":"2"}}`; free text is `{"q1_other":"Peach"}` with no `q1`; skip is `{"action":"decline"}` | [CT] | medium |
| Multi-select reply is an array of `const`s: `{"q2":["r","b"]}` | none (assumed from the schema) | low, LIVE |
| `{"action":"cancel"}` when the question is withdrawn | none (the elicitation action set) | low, LIVE |
| Decisions arrive as `session/request_permission` with an `interaction_` id; question is `toolCall.title`; choices are options (`optionId`, `name`), all of kind `allow_once`; reply `selected`, skip `cancelled` | [C], [CT]; no [B] hit for the prefix | medium-low, LIVE |
| Option names can repeat (the fixture has two named "Continue") | [CT] | medium |

## Design

### Gate on the flavor

`answerer` and `permission()` serve every ACP agent. Cursor today answers an unknown request with `{}` (`lenient`) and the rest with "Method not found". Neither must change. Add one field to `flavor` (`flavor.go`):

```go
// elicits asks the user through elicitation/create, which initialize
// offers it, and through permission requests for its interaction_ calls.
elicits bool
```

Set it for Antigravity only, and add `"elicitation": map[string]any{"form": map[string]any{}}` to Antigravity's `caps` beside `session`.

### Share the question card

`Session.question` (`request.go`) parses Cursor's and Grok's params and replies with answers only, so a deny (Skip, or the broker's timeout) and a `""` (a stop or a dismiss) look alike to it. Elicitation must tell them apart (decline vs cancel). Split it:

- `func (s *Session) questionCard(qs []question) proto.PermissionRequest` builds the AskUserQuestion request (the current body up to `req :=`), unchanged in output.
- `question(id, params, reply)` keeps its signature and behaviour: `qs := questions(params)`, then `s.ask(id, s.questionCard(qs), …)` as today. `cursorAnswers` and `grokAnswers` are untouched.
- The two new paths call `s.ask(id, s.questionCard(qs), func(a broker.Answer) {…})` directly, so they see `a.Decision`.

Add a field to `question`:

```go
// Other is the form field that takes a typed answer, "" for none.
Other string
```

### Decisions: `interaction_` permissions

In `permission()`, right after `asked := call(params)` and before the tool merge, add:

```go
if s.f.elicits && decision(asked, p.Options) {
	s.decide(id, asked.Title, p.Options)
	return
}
```

Order matters: the branch has to run before `auto()` and before `s.tools` would show the request as a tool row.

`decision(asked toolCall, options []option) bool` is a free function. It is true only when all of these hold:

- `strings.HasPrefix(asked.ID, "interaction_")`
- `strings.TrimSpace(asked.Title) != ""`
- `len(options) > 0`
- no option's kind starts with `reject`

Otherwise today's path runs unchanged. The reject check keeps any real approval that happens to carry the prefix on today's access policy.

`decisionQuestion(title string, options []option) question` is a free function. It builds one single-choice question with `Prompt: title` and an option per choice, `{ID: optionId, Label: name}`. Labels are matched by text, so a `name` that more than one option shares becomes `name + " (" + optionId + ")"` on every option that has it. A name that is empty becomes its `optionId`.

`decide` asks with `s.ask(id, s.questionCard([]question{q}), …)` and replies with `s.selected(id, …)`:

| User | Reply |
|---|---|
| Picks a choice (`a.Decision == "allow"` and `q.picked(a.Answers[q.Prompt])` is exactly one option) | `{"outcome":{"outcome":"selected","optionId":<id>}}` |
| Types text matching no label (request_permission has no free-text slot) | `{"outcome":{"outcome":"cancelled"}}` |
| Skip, timeout (`deny`), or stop/dismiss (`""`) | `{"outcome":{"outcome":"cancelled"}}` |

### Form questions: `elicitation/create`

In `answerer`, add:

```go
case "elicitation/create":
	if s.f.elicits {
		return func() { s.elicit(id, params) }
	}
```

Other flavors fall through to `nil` as today.

`formQuestions(params json.RawMessage) ([]question, bool)` is a free function. It accepts only this:

- `mode` is absent or `"form"`.
- `requestedSchema.properties` is a non-empty object.
- Properties are ordered by key length, then lexically, so `q2` comes before `q10`. A Go map loses document order, and the fixture's keys are `qN`.
- A key `K_other` whose `K` is also a property is not a question. It sets `Other: "K_other"` on `K`'s question.
- Each remaining property becomes a question with `ID: key` and `Prompt: cmp.Or(title, description, key)`:

  | Property | Question |
  |---|---|
  | `type:"string"` with `oneOf` or `anyOf` of `{const, title}` | single; option `{ID: const, Label: cmp.Or(title, const)}` |
  | `type:"string"` with `enum` | single; option `{ID: v, Label: v}` |
  | `type:"array"` whose `items` has `enum`, `oneOf` or `anyOf` | `Multi`, options as above |
  | `type:"string"` with none of those | free text, no options |
  | anything else (number, integer, boolean, object, an array without choices, a `const` or `enum` value that isn't a string) | the whole request is unreadable |

Unreadable (`false`) means `s.c.Refuse(id, invalidParams, "Anywhere can't show this form")`, with a new `const invalidParams = -32602` beside `methodNotFound` in `rpc.go`. Per the [B] string `"elicitation/create rejected … asking in c[hat]"`, agy then writes the question into the chat, which is today's behaviour.

`elicit` asks with `s.ask(id, s.questionCard(qs), …)` and replies with `formAnswer(qs, a)`, a free function:

| `a.Decision` | Reply |
|---|---|
| `"allow"` | `{"action":"accept","content":{…}}`, or `{"action":"decline"}` when `content` ends up empty |
| `"deny"` (Skip, or the broker's timeout) | `{"action":"decline"}` |
| `""` (stop or dismiss) | `{"action":"cancel"}` |

`content` holds one entry per question, built from `answer := a.Answers[q.Prompt]`:

- **A pick.** `opts := q.picked(answer)`. Both clients send picked labels joined with `", "` in the order the user picked them, and typed text in place of them, so the test has to ignore order:
  - a single question is a pick when `len(opts) == 1`; write `q.ID: opts[0].ID`;
  - a `Multi` one is a pick when `len(opts) > 0` and equals the number of `", "`-separated parts of `answer`; write `q.ID: [ids…]` in option order (the order `picked` returns).
- **Typed text.** Any other non-empty answer:
  - with `q.Other` set, write `q.Other: answer`;
  - on a free-text question (no options), write `q.ID: answer`;
  - otherwise leave the question out.
- **Empty.** Leave the question out.

Answers are keyed by question text on both clients, so two properties with the same title share one answer. That is accepted, and no fixture does it.

## Clients: no change

Checked against the code, not assumed:

- The card is the existing `AskUserQuestion` permission detail, `{"questions":[{"question","header"?,"options":[{"label","description"}],"multiSelect"}]}`. `questionCard` sends `options: []`, never null.
- Desktop parses it in `dk/agents/src/ask.rs:52` and `items.rs:116` (serde `default`, so header and description are optional), then `dk/chat/src/timeline.rs:292`.
- iOS parses it in `packages/ios/Packages/AnywhereKit/Sources/ChatModel/Timeline.swift:262`, where `text()` (`:290`) turns a missing header or description into `""` and accepts empty options.
- Answers are keyed by question text. Picks are labels joined with `", "`, and typed text wins over them. Desktop: `dk/pocket/src/chat/question.rs:145-153`. iOS: `ChatModel/Wizard.swift:83`.
- Skip sends a deny. Desktop: `question.rs:441` sends `Command::Deny`. iOS: `Stores/ChatStore.swift:670` sends `.deny`.
- A question with no options (free text) parses and can be submitted with typed text on both. Desktop: `Wizard::can_advance`. iOS: `Wizard.canAdvance(typedEmpty:)`.

So no protocol, golden, desktop or iOS change is needed. One cosmetic point, left as is: iOS's footer reads "Skip stops …'s turn", which is already inexact for Cursor and Grok.

## Files

| File | Change |
|---|---|
| `packages/pocketd/internal/acp/flavor.go` | `elicits` field; Antigravity gets `elicits: true` and the `elicitation.form` cap |
| `packages/pocketd/internal/acp/request.go` | `elicitation/create` case; `permission()` branch; `questionCard` split out of `question`; `question.Other`; new `decision`, `decisionQuestion`, `decide`, `formQuestions`, `elicit`, `formAnswer` |
| `packages/pocketd/internal/acp/rpc.go` | `invalidParams` const |
| `packages/pocketd/internal/acp/session_test.go` | tests below |

## Tests

All tests go in `session_test.go`, after `TestGroksQuestionIsAnsweredByItsText`. Each opens a session with `start(t, opts{provider: "antigravity", launch: state.Launch{Access: …}, answer: script(`{"sessionId":"s1"}`, says())})`, sends the request with `a.ask(id, method, params)`, reads the card with `asked(t, b)`, answers with `b.Resolve` or `b.Dismiss("a1\x00"+id, "")`, and checks `a.replied(id)`. Fixtures are the [CT] shapes above.

**Existing test to update.** In `TestEachAgentOpensItsSessionItsOwnWay` (`session_test.go:662`), Antigravity's expected `initialize` becomes:

```
{"clientCapabilities":{"elicitation":{"form":{}},"fs":{"readTextFile":false,"writeTextFile":false},"session":{"configOptions":{"boolean":{}}},"terminal":false},"clientInfo":{"name":"anywhere","title":"Anywhere","version":"0"},"protocolVersion":1}
```

**New tests.**

1. `TestAntigravitysFormQuestionIsAskedAsAChatQuestionAndAnsweredByItsConst`. A table over the cases below, with properties `q1` (single, `oneOf` Apple/`"1"` and Pear/`"2"`, plus `q1_other`), `q2` (multi, `items.anyOf` Red/`"r"`, Green/`"g"`, Blue/`"b"`) and `q10` (free-text string). The card's detail lists `q1`, `q2`, `q10` in that order with no `q1_other` page.
   - answered: `q1` "Pear", `q2` "Red, Blue", `q10` "No" → `{"action":"accept","content":{"q1":"2","q10":"No","q2":["r","b"]}}`
   - picked out of order: `q2` "Blue, Red" → `content` has `"q2":["r","b"]`
   - typed: `q1` "Peach" → `content` has `"q1_other":"Peach"` and no `q1`
   - skipped: `deny` → `{"action":"decline"}`
   - stopped: dismissed → `{"action":"cancel"}`
2. `TestAFormQuestionPocketdCannotShowIsRefusedSoTheAgentAsksInChat`. A `type:"boolean"` property is answered `!-32602 …`, and nothing is asked (`b.Open()` is empty).
3. `TestAnotherAgentsFormQuestionIsAnsweredAsBefore`. Cursor answers `{}`. Hermes answers `!-32601 Method not found: elicitation/create`.
4. `TestAntigravitysDecisionIsAskedInEveryAccessAndAnsweredByOptionID`. Run over `ask`, `edits`, `auto` and `full`. A `session/request_permission` with `toolCallId:"interaction_1"`, title "Which fruit?" and options `{"1","Apple","allow_once"}` and `{"2","Pear","allow_once"}` shows a single-choice card with those labels, never an auto-answer. Picking "Pear" replies `{"outcome":{"optionId":"2","outcome":"selected"}}`.
5. `TestASkippedOrTypedDecisionIsCancelled`. Both `deny` and an answer of "Peach" reply `{"outcome":{"outcome":"cancelled"}}`.
6. `TestADecisionsChoicesThatShareANameAreToldApartByID`. Two options named "Continue" (`a`, `b`) show as "Continue (a)" and "Continue (b)". Picking the second replies `selected` with `b`.
7. `TestAnInteractionRequestThatIsNoDecisionIsAnsweredByAccessAsBefore`. Run under `full`:
   - Antigravity, with an `interaction_` id and options `{"allow_once","Allow","allow_once"}` and `{"reject_once","Deny","reject_once"}`, replies `selected` with `allow_once` and asks nothing;
   - Hermes, with an `interaction_` id and the decision fixture, replies the first option and asks nothing.

**Existing tests that must stay green.** `TestCursorsQuestionIsAskedAsAChatQuestionAndAnsweredByOptionID` and `TestGroksQuestionIsAnsweredByItsText` cover the `questionCard` split. `TestAccessAnswersAPermissionWithoutAsking` and `TestARequestTheSessionDoesNotKnowIsRefusedButCursorsIsAnsweredEmpty` cover today's paths.

Run from `packages/pocketd`: `gofmt -l .`, `go vet ./...`, `staticcheck ./...`, then `go test ./internal/acp/`, then `go test ./...`.

## What only a live agy can settle, and how this fails safe

These need a signed-in `agy acp` ≥ 1.5.15 with raw JSON-RPC logged (research §3, checks 2 and 3). None can be checked here.

| Unknown | If the guess is wrong |
|---|---|
| Decisions use the `interaction_` prefix | Nothing matches `decision()`, so the code is dead and behaviour stays as today. |
| Decision options are all `allow_once` | A request with a `reject*` option falls through to today's policy. An approval that has the prefix and only allow options would now be **asked** in auto and full access. That is the safe direction: the user is asked, never answered for silently. |
| Elicitation schema shape | An unreadable schema is refused, and per [B] agy then asks in chat, which is today's behaviour. If agy instead fails the step on refusal, the user loses the chat fallback. Check it live. |
| Reply uses `const`, not title | If agy wants titles, a pick could be read as unknown. The code can't guard against this; it needs check 2. |
| Multi-select reply as an array of `const`s; the `cancel` action | Low confidence, no fixture. Confirm with check 2. A mismatch affects only those replies. |
| Whether agy also sends `tool_call` updates for `interaction_` ids | If it does, a generic "other" tool row may show beside the card. That is out of scope here; note it for the subagent/tool PR. |

Live check, once agy is available:

1. In ask access, prompt agy to ask a multiple-choice clarifying question. Confirm the card shows, pick an answer, and check that the `elicitation/create` reply was accepted and the agent used the answer.
2. Repeat with a typed answer and with Skip.
3. In full access, trigger a decision. Confirm the card shows rather than an auto-pick, and that the chosen `optionId` is what agy acts on.

## Out of scope

- `packages/pocketd/e2e/fakeacp` and e2e tests.
- URL-mode elicitation.
- Elicitation for any other flavor.
- Subagent cards, steering, quota (research PRs 2–7).
- iOS Skip footer copy.
