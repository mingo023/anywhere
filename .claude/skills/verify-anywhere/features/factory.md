# Factory

The Factory moves a project's Board tickets through seven stations (To do, Spec, Plan, Implement, Checks, PR review, Merged). Settings › Factory turns it on per project; the Board's Factory layout shows the stations; a ticket page's footer bar asks for your decision at the Plan gate, on an ask, and at the PR review gate.

## Sub-features

- `factory-settings` turns the Factory on for a project and changes a config row; each change is sent as `factory.config.save` after 400ms.
- `factory-board` shows the seven stations in the Factory layout.
- `factory-plan-approve` approves a proposed plan from the footer bar, which sends `team.start`.
- `factory-ask` answers a worker's ask from the footer bar with `permission.resolve`.
- `factory-merge` merges the PR at the PR review gate with `gh.pr.merge`; a ticket not at that gate has no merge action.

## How to get to it (user POV)

- Settings: Cmd+, then Factory in the section list.
- Board: click Board in the sidebar, then Factory in the layout tabs (Factory, Status, List).
- Ticket page: click a ticket card in the Factory layout.

## Driving it with drive.sh

Preconditions:

- `sessions` scenario. `S=.claude/skills/verify-anywhere/scripts`.
- Add `pause` steps before the first click: the sidebar's Board row only appears once pocketd has connected (use 4 pauses). Capture mode drops teams between specs, which is why the fixture sends the plan when t15 is opened.
- Fixture knobs (header of `fixture.ts`): `FIXTURE_FACTORY_OFF=1` (Factory starts off, saves are kept), `FIXTURE_FACTORY_TEAM=1` (t15 gets a proposed Team of three tasks), `FIXTURE_FACTORY_PR=1` (t22's PR #203 is mergeable through a `gh` shim the login shell finds under the scratch HOME).
- Click points below are window points for the sessions scenario at 1440×900; re-measure if the layout changed.

- **Settings.** `FIXTURE_FACTORY_OFF=1 CAPTURE_KEYS="cmd-,;cmd-," CAPTURE_CLICKS="66,402;66,402;1146,392" $S/drive.sh sessions $OUT before=key,click,pause on=key,click,click,pause,pause`. `impl-on.png` shows the Factory switch on; `factory.config.save` carries `"enabled":true`. Clicking the Spec station switch at `1146,503` instead sends `"spec":false`. Slot limit is below the fold.
- **Board.** `OPEN="pause,pause,pause,pause,click,pause,click,pause"`, `CAPTURE_CLICKS="59,123;322,65"`: Board row, then the Factory tab. `impl-*.png` shows the stations; To do and Merged are collapsed rails.
- **Plan gate.** `FIXTURE_FACTORY_TEAM=1`, `OPEN` plus `click,pause,pause,pause` with clicks `59,123;322,65;653,168` (APP-15), then `1029,869` (Approve and start 3 workers). Frame: `team.start` with `"lead":"plan-lead"`.
- **Ask.** Clicks `59,123;322,65;864,175` (APP-18), then `989,869` (Deny) or `1061,869` (Allow once), one per run. Frame: `permission.resolve` with `requestId` `ask-ios` and `deny` or `allow`.
- **Merge.** `FIXTURE_FACTORY_PR=1`, 8 pauses before the first click so `gh` is polled, clicks `59,123;322,65;1281,165` (APP-22), then `1013,868` (Squash and merge). Frame: `gh.pr.merge` with `number` 203 and `method` `squash`. APP-23 at `1281,339` is at PR review without the gate and has no bar.

## Gotchas

- The Board row is missing from a screenshot taken before pocketd connects, so an early click lands on nothing.
- The fixture never answers `team.start`, `permission.resolve` or `gh.pr.merge`: the bar's own state change (plan locked, bar gone, "Merging…") is the app's, not pocketd's.
- Settings saves are debounced 400ms; end the spec with two `pause` steps.
- Run Deny and Allow once in separate runs; each is a different decision for the same request.
