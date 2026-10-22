# Changelog

## 0.1.12

- Spinners turn in even steps and the chat follows its height at once (#116): (05ca0f9)
- Chat shows a session's prompt at once when it starts in a Local (#115): (34c4dcc)
- Chat keeps a sent prompt steady when a row under it closes (#114): (4904516)
- The sidebar lists each project's worktrees as they land at launch (#113): (5c94c52)
- Changes and the graph compare a branch with the branch it was created from (#112): (effeead)

## 0.1.11

- A new worktree runs the repo's setup when the project settings name none (#111): (bb3fd16)
- Chat maps the sub-agents an agent spawns, for Claude and Codex (#110): (fc2a49a)
- Chat picks the agent's MCP servers with /mcp and signs in to them (#109): (286ccfe)
- Chat shows a steer Claude hasn't read yet, with Send now (#107): (e84975b)
- Settings sits at the sidebar's foot and dark glass opacity is adjustable (#108): (ebfae26)
- Chat highlights every skill a prompt names (#106): (499ec2d)

## 0.1.10

- Chat handles steers, stops, sub-agents and background work as Claude does (#105): (ffdfd52)
- Chat keeps Claude's text in view when its fold is shut (#103): (6c95e9e)
- Chat no longer flashes the previous prompt over a message just sent (#104): (4b2b762)
- Chat shows what Claude writes between commands in full (#102): (97d4668)
- Chat shows a sent message at once and keeps it steady when the agent echoes it (#101): (afc9878)
- Composer takes files dropped anywhere on its box and copied files pasted into it (#100): (190bcec)
- Release retries stapling when Apple's ticket lookup drops (#99): (51a8754)

## 0.1.9

- Chat streams its text and stays live while sub-agents work in the background (#98): (b7372b3)
- Chat's pinned prompt is a one-line capsule that opens into a card (#97): (cb68a57)
- Chat ends the turn of a command Claude runs itself, like /goal clear (#96): (a399691)

## 0.1.8

- Chat stream keeps background chats current, holds a scroll up, and clears on send (#93): (5301cb7)
- A new worktree reuses the main checkout's build and third_party (#92): (65e6a1e)
- Chat sends a message typed mid-turn into the turn, as the CLI does (#91): (28d1a07)
- A chat table too wide to fit sits in a frame and keeps its scrollbar showing (#95): (9822711)
- Chat pins the prompt being answered as a floating card, as VS Code does (#94): (8c54d4f)
- Settings segmented controls take a click when another one sits above them (#90): (895b60a)
- Chat asks a question on a card, with Skip (#89): (86a6b74)
- Chat grows a double-click selection word by word as it drags, and a prompt's text is selectable (#88): (47dd990)
- Chat shows a prompt Claude took mid-turn, so its send stops reading Sending… (#87): (0779666)
- The / list opens for a slash past the draft's start (#86): (84e1b82)
- A long paste scrolls the composer to its end (#85): (71c66bc)

## 0.1.7

- Chat ends a background task the agent stopped (#84): (d827cc1)
- Chat ends a background agent whose notification comes mid-turn (#83): (2a2f662)
- Renaming a Worktree or Local renames its Claude conversations too (#82): (8e76a2b)

## 0.1.6

- Experimental chat for Claude Code and Codex: New chat in a tab's + menu, or Settings › Agents › Start as a chat for new sessions (#80): (d7c9adc)
- Desktop releases on Linux and Windows, started from a Release workflow (#79): (312b948)
- Terminal banner drops "Resumed" and gets a close button (#77): (95b9900)
- Terminal paints text that vanishes into its background white or black (#75): (bd7e33e)
- Dock badge shows the bell's count, asking macOS for the Badge permission (#74): (fea2562)
- Pinned row rename shows its text as you type, and its name stays while hovered (#73): (7ec6388)
- Terminal shows a pointing hand over a link a click would open (#72): (aad3dc0)
- Needs you and Done show a haloed dot, and a Done row's name stands out until viewed (#71): (870c69f)
- Inputs take dropped files, and the composer attaches dropped images (#70): (9e17d35)

## 0.1.5

- Agents you start in a terminal keep their account and don't resume on restart (#69): (74f174d)
- Sidebar loads every project's pull requests in the background (#68): (01a7337)
- CI builds Anywhere Dev.app on every push to main (#67): (e64e99a)
- Projects drop the default Local and open the composer on an idle main checkout (#66): (6656cad)
- Group rename shows its text as you type, and group heads pick an emoji (#65): (c9c3ac2)
- Repo scripts get ANYWHERE_* paths and fall back to .anywhere/setup.sh and teardown.sh (#64): (4b17080)

## 0.1.4

- Changelog draft lists commits with their hash: (5bc0c0c)
- New session composer picks the project and keeps what you typed (#63): (a2a640d)
- Changes badge no longer has a black ring in dark mode (#62): (de7c092)
- Sidebar filters, groups and pins projects, and clones from a URL in its own dialog (#61): (8e5fe7a)
- Terminal shows dim text dim (#60): (70f6f27)
- Projects read setup and teardown from .anywhere/config.json in the repo (#59): (e0ad6a6)

## 0.1.3

- Run projects on remote hosts (Mac, Linux, Windows) over SSH; add, connect, retry and remove hosts in Settings, and sign in once with a password
- Use several Claude Code and Codex accounts per machine, picked per session; each project's pull requests and terminals use its own gh account
- Set each agent's label and launch args, fork sessions, and run shell-defined agent commands
- Answer Claude's questions and permission asks from the notch
- Follow a pull request in Changes: a PR card folds to its verdict, and the agent fixes failing checks
- Show a worktree's changes in the pane strip and the changes count on the git rail icon
- Start sessions from a main-area page; the new-session picker offers a new Worktree or a new Local
- Open a project's Local home from its +, rename a Local and add more beside it
- Add a project with only a name, icon and the two scripts; worktrees default to `~/.anywhere/worktrees`
- Give projects custom icons, emoji or image
- Add a file to a chat from a searchable session panel, also from markdown
- Render markdown previews like pluk-inc/markdown-preview
- Colour Explorer files and folders by git status, and add a right-click menu to the file tree
- Right-click a terminal to copy, paste, clear, close or kill it
- Paste images into the Local home prompt, with one composer for both prompts
- Pick a notification sound from a list, or add your own
- Zoom the whole window with ⌘= ⌘- ⌘0
- Take icons from Lucide, with their own Sessions and Explorer icons on the right rail
- Show a hover card on sidebar rows, with a working sweep and a PR-state icon
- Draw a dot field behind the prompt pages; slide toggles and unfold projects and the PR card into place
- Name new worktrees from petname, skipping origin's branches
- Name every Claude session from its first prompt, always in English
- Count commits to push against origin/main, not a stale local main
- Keep the view where it is when a new worktree's terminal arrives in another workspace
- Forget worktrees deleted outside the app
- Close a terminal's tab on any exit except a failed setup
- Let Claude Code open links on click in terminals
- Focus a pane on its first click
- Fall back to a login shell that runs when the configured one isn't installed
- Keep a pasted portrait image inside its thumbnail
- Scroll the Explorer smoothly and fill it at once on a project or worktree switch
- Stop the PR card showing a check as fixing once its rerun passes, and dim a merge button that can't merge
- Fix the PR card's callout spacing and wrapping in a narrow panel
- Answer Claude's question from text typed in the notch instead of rejecting it
- Remove the faint box inside the composer in dark mode

## 0.1.2

- Pick a session's model and permission mode beside its agent; a project remembers the permission it last used
- Empty the project sidebar down to projects, and spin working agents and running automations in orange
- Move Sessions, Explorer and Changes to a right-hand panel behind an icon rail, toggled by `⌘\`
- Start an agent from an empty pane, with the agent picker, starter tasks and a Terminal link
- Rebuild Settings on the dark design, with every section wired up
- Add automations: scheduled agent runs with a list, runs feed and editor
- Rework the new-session sheet around a checkout picker, branch search and PR link
- Resize the Changes graph by dragging its top edge, remembered across launches
- Confirm quitting with the native alert
- Show sidebar status on the left of each project and worktree
- Open the session when an inbox row is clicked
- Count failed and done sessions in the Dock badge
- Read Claude's status from its own reports, more reliably than hooks
- Wait for an outdated service to update, then offer a restart
- Stop opening then closing a panel from the rail from zooming the window
- Recover pocketd's socket when its file is removed
- Show the app icon in prompts on macOS 26
- Bind a prompt-launched Codex session to its own thread
- Redraw less: cap the working spinner at 10 fps
- Make the file preview glass like the other panes
- Switch tabs with ⌘1–9; jump to sessions with ⌃1–9
- Let ⌘↵ submit text fields; open the inbox's session only from the inbox
- Start Mac sessions with the agent's own permission settings
- Keep the dark glass window translucent behind modals
- Slim the sidebar's project and worktree rows to Apple Notes' type
- Count the worktree's sessions on the Sessions tab
- Halve the release DMG: 81 MB to 40 MB
- Insert a newline on cmd-enter and shift-enter in the terminal, no setup needed

## 0.1.0

The first release of Anywhere: run Claude Code and Codex sessions on your Mac, and follow them from your phone.

- Projects and worktrees in one sidebar: add any folder, open branches and PRs as worktrees, name sessions from their prompt.
- Terminals in your login shell, with tabs, splits, mouse support and image paste.
- Agent sessions that show when they're working or need you, with banners, sounds and a Dock badge. Allow or deny permissions from the Mac or the phone.
- Changes, diffs, a commit graph, file preview and editing, and a built-in browser per worktree.
- The background service keeps terminals running when the window is closed, and updates in place without interrupting them.
- Updates arrive automatically. "Restart to update" installs them.
