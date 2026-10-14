# Changelog

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
