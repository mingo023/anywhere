import Testing
@testable import ChatModel

@Test("a github pull request url names its owner, repo and number, whatever follows")
func aPullRequestUrlNamesItsPr() {
    let pr = PrLink("https://github.com/mingo/anywhere/pull/198/files?w=1")
    #expect(pr?.owner == "mingo" && pr?.repo == "anywhere" && pr?.number == 198)
    #expect(PrLink("http://www.github.com/mingo/anywhere/pull/7#discussion")?.number == 7)
}

@Test("a link that isn't a github pull request is none")
func aLinkThatIsntAPullRequestIsNone() {
    #expect(PrLink("https://github.com/mingo/anywhere/issues/198") == nil)
    #expect(PrLink("https://gitlab.com/mingo/anywhere/pull/198") == nil)
    #expect(PrLink("github.com/mingo/anywhere/pull/198") == nil)
    #expect(PrLink("https://github.com/mingo/anywhere/pull/19a") == nil)
    #expect(PrLink("https://github.com//anywhere/pull/1") == nil)
}

@Test("the same pull request matches whatever the case of its names")
func theSamePullRequestMatchesWhateverTheCase() {
    let a = PrLink("https://github.com/Mingo/Anywhere/pull/198")!
    #expect(a.same(PrLink("https://github.com/mingo/anywhere/pull/198")!))
    #expect(!a.same(PrLink("https://github.com/mingo/anywhere/pull/199")!))
}
