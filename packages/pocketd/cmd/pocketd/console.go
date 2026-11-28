package main

// needsWindowless says whether pocketd must switch to a console with no window,
// from how many processes share its console and whether that console has a
// window. A console program pocketd runs, like git, inherits pocketd's console,
// and opens a window of its own when pocketd has none.
func needsWindowless(procs int, window bool) bool {
	return procs == 0 || procs == 1 && window
}
