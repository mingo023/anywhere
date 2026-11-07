import Testing
import Wire
@testable import ChatModel

private let pr = PullRequest(
    number: 12, title: "t", url: "https://github.com/acme/app/pull/12", state: "OPEN", draft: false, base: "main", head: "aaa",
    checks: Checks(passed: 0, failed: 1, pending: 0), runs: [], reviews: [], requested: [], gate: .checksFailed, threads: [])

@Test("the fix prompt numbers each failed check with its log, then each thread with its code and what was said")
func theFixPromptNumbersChecksThenThreads() {
    let thread = ReviewThread(id: "t1", path: "src/a.rs", line: 42, outdated: false, resolved: false, comments: [
        ThreadComment(author: "mira", body: "Cap it.\n", at: 0, hunk: "@@ -1,4 +1,4 @@\n a\n b\n-c\n+d"),
        ThreadComment(author: "bo", body: "Agreed.", at: 1),
    ])
    let prompt = FixPrompt.fix(pr, checks: [CheckRun(name: "test", outcome: "failed", url: "https://ci/1")], threads: [thread])
    #expect(prompt == """
        Fix what holds back PR #12 (https://github.com/acme/app/pull/12):

        1. The check "test" failed. Its log: https://ci/1

        2. A review comment on src/a.rs:42:
        ```
         b
        -c
        +d
        ```
        mira: Cap it.
        bo: Agreed.

        Then commit and push.
        """)
}

@Test("a failed check with no log is named alone")
func aCheckWithNoLogIsNamedAlone() {
    #expect(FixPrompt.fix(pr, checks: [CheckRun(name: "lint", outcome: "failed")], threads: []).contains("1. The check \"lint\" failed.\n"))
}

@Test("the conflict prompt merges origin's copy of the base")
func theConflictPromptMergesOriginsBase() {
    #expect(FixPrompt.conflict(base: "main") == "Merge origin/main into this branch and resolve the conflicts, then push.")
    #expect(FixPrompt.conflict(base: "origin/main") == "Merge origin/main into this branch and resolve the conflicts, then push.")
}

@Test("a hunk tail skips its header")
func aHunkTailSkipsItsHeader() {
    #expect(FixPrompt.hunkTail("@@ -1 +1 @@\n+a") == ["+a"])
    #expect(FixPrompt.hunkTail("@@ -1,4 +1,4 @@\n a\n b\n-c\n+d\n e") == ["-c", "+d", " e"])
    #expect(FixPrompt.hunkTail("").isEmpty)
    #expect(FixPrompt.hunkTail(nil).isEmpty)
}

@Test("a thread's place is its path and line, or its path on a whole file")
func aThreadsPlaceIsItsPathAndLine() {
    let file = ReviewThread(id: "f", path: "README.md", outdated: false, resolved: false, comments: [])
    #expect(FixPrompt.place(file) == "README.md")
    var line = file
    line.line = 7
    #expect(FixPrompt.place(line) == "README.md:7")
}
