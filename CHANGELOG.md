# Changelog

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
