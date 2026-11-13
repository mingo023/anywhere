# Settings

Cmd+, opens Settings: a section list (General, Appearance, Notifications & Sounds, Keyboard, Agents, Accounts, Automations, Phone & Remote, Hosts, Projects, Sidebar & Inbox, Terminal, Files & Editor, Browser, Git & GitHub, Diff & Graph), a search box, and switches and segmented controls that apply at once.

## Sub-features

- `settings-open` opens on General with the section list.
- `settings-toggle` flips a switch, which changes its state immediately.
- `settings-section` switches to another section from the list.
- `settings-search` filters settings by text (Cmd+F inside Settings).
- `settings-back` returns to the previous screen with Back.

## How to get to it (user POV)

- Press Cmd+,.
- Click Settings at the bottom of the sidebar.

## Driving it with drive.sh

Preconditions:

- Any scenario. Defaults: "Restore tabs and splits" on, "Open at login" off.
- `S=.claude/skills/verify-anywhere/scripts; OUT=.ui-review/shots/verify/settings-$(date +%Y%m%d-%H%M%S)`.

- **Open.** Run `CAPTURE_KEYS="cmd-,;cmd-," CAPTURE_CLICKS="1146,546" $S/drive.sh sessions $OUT open=key toggled=key,click`.
- **Open result.** `impl-open.png` shows the "General" header, "Updates, startup, quitting…", and "Restore tabs and splits" switched on (green).
- **Toggle result.** `impl-toggled.png` shows the same page with "Restore tabs and splits" off (grey). Nothing else changed.
- **Search.** `cmd-,`, `cmd-f`, type `worktree`. Only matching settings remain (not yet run).
- **Proof.** Cite both screenshots side by side.

## Gotchas

- Capture mode never writes `desktop.json`, so a change can't be proved to survive a restart here. Report persistence as unverified.
- Whether a toggle survives the between-spec reset isn't established. Flip a given switch in only one spec per run.
- Settings is a full screen, not an overlay. Back returns to Sessions.
