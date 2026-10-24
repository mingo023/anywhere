import Testing
@testable import Domain

private func score(_ query: String, _ path: String) -> Int? { FuzzyMatch.score(FuzzyMatch.normalize(query), in: path) }

@Suite("Fuzzy match")
struct FuzzyMatchTests {
    @Test("a query matches its letters in order anywhere in the path, ignoring case and spaces")
    func subsequence() {
        let index = FuzzyIndex(["packages/desktop/crates/pocket/src/explorer.rs", "README.md", "docs/mobile/plan.md"])
        #expect(index.search("pcexp", limit: 10) == ["packages/desktop/crates/pocket/src/explorer.rs"])
        #expect(index.search("xyz", limit: 10).isEmpty)
        #expect(index.search("READ me", limit: 10) == ["README.md"])
    }

    @Test("a base name match beats one spread over folders")
    func basename() {
        #expect(FuzzyIndex(["src/main/actions/time.go", "src/mat.go", "lib/format.go"]).search("mat", limit: 3).first == "src/mat.go")
    }

    @Test("matches at word starts, camel humps and in runs score higher")
    func boundaries() {
        for (query, better, worse) in [("sl", "a/status_letters.go", "a/xsxxxlx.go"), ("sr", "a/StatusRow.go", "a/xsxxrxxxx.go"), ("abc", "x/abcd.go", "x/axbxc.go")] {
            #expect(score(query, better)! > score(query, worse)!, "\(query): \(better) vs \(worse)")
        }
    }

    @Test("an exact base name comes first")
    func exact() {
        #expect(FuzzyIndex(["docs/plan-old.md", "a/b/c/plan.md", "planner/main.go"]).search("plan", limit: 3).first == "a/b/c/plan.md")
    }

    @Test("changed files win ties, then shorter paths, then the given order")
    func ties() {
        let index = FuzzyIndex(["bb/view.rs", "a/view.rs", "c/view.rs", "long/folder/view.rs"])
        #expect(index.search("view", limit: 4) { $0 == "long/folder/view.rs" } == ["long/folder/view.rs", "a/view.rs", "c/view.rs", "bb/view.rs"])
    }

    @Test("an empty query keeps changed files first, then the given order, up to the limit")
    func empty() {
        let index = FuzzyIndex(["z.go", "b.go", "a.go", "c.go"])
        #expect(index.search("", limit: 3) { $0 == "a.go" || $0 == "c.go" } == ["a.go", "c.go", "z.go"])
        #expect(index.search("", limit: 0).isEmpty)
    }

    @Test("the matched letters are the tight run in the base name, for highlighting")
    func positions() {
        #expect(FuzzyMatch.positions(FuzzyMatch.normalize("main"), in: "src/main/main.rs") == [9, 10, 11, 12])
        #expect(FuzzyMatch.positions(FuzzyMatch.normalize("q"), in: "src/main.rs").isEmpty)
    }

    @Test("scores agree with pocketd's for the same query and path")
    func agreesWithGo() {
        // fsview's TestScoresMatchThePhonesCopy pins the same numbers.
        let cases = [
            ("sl", "a/status_letters.go", 62), ("sl", "a/xsxxxlx.go", 50), ("abc", "x/abcd.go", 120), ("sr", "a/StatusRow.go", 74),
            ("plan", "a/b/c/plan.md", 198), ("pcexp", "packages/desktop/crates/pocket/src/explorer.rs", 112),
        ]
        for (query, path, want) in cases {
            #expect(score(query, path) == want, "\(query) in \(path)")
        }
    }

    @Test("a path ending in a slash or shorter than the query doesn't crash or match")
    func edges() {
        #expect(score("ab", "a/") == nil)
        #expect(score("a", "a/") != nil)
        #expect(score("", "x") == 0)
    }
}
