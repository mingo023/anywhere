import Testing

@testable import ChatModel

private func turn(_ id: String, _ at: Int, _ text: String, _ ended: Bool) -> Turn {
    Turn(id: id, prompt: Message(id: id, at: at, text: text), end: ended ? .done(durationMs: 1, usage: nil) : nil)
}

private func stopped(_ id: String, _ at: Int, _ text: String) -> Turn {
    var t = turn(id, at, text, false)
    t.end = .failed(error: "interrupted", durationMs: 0)
    return t
}

private func compaction(_ id: String) -> Turn { Turn(id: id, blocks: [.compacted(id: id, auto: false)]) }
private func tx(_ turns: Turn...) -> Transcript { Transcript(turns: turns) }
private func open(_ at: Int) -> Transcript { tx(turn("u1", at, "long job", false)) }
private func released(_ o: inout Outgoing) -> [String] { o.released().map(\.text) }
private func deliveries(_ o: Outgoing) -> [Delivery] { o.list.map(\.delivery) }
private func texts(_ o: Outgoing) -> [String] { o.list.map(\.text) }
private func id(_ o: Outgoing, _ text: String) -> Int { o.list.first { $0.text == text }!.id }
private func waiting(_ o: Outgoing) -> [String] { o.waiting.map(\.text) }

private func sent(_ o: inout Outgoing, _ text: String, _ request: String) {
    let id = o.push(text, working: false, now: 0)!
    o.sentAs(id, request)
}

@Test("a send while idle is sending and while working is queued")
func aSendWhileIdleIsSendingAndWhileWorkingIsQueued() {
    var o = Outgoing()
    o.push("a", working: false, now: 0)
    o.push("b", working: true, now: 0)
    #expect(deliveries(o) == [.sending, .queued])
}

@Test("a send settles when its prompt reaches the transcript")
func aSendSettlesWhenItsPromptReachesTheTranscript() {
    var o = Outgoing()
    o.push("fix  the\nbuild", working: false, now: 50_000)
    o.settle(tx(turn("u1", 1_000, "fix the build", true)), working: false, now: 50_500)
    #expect(o.list.count == 1, "a prompt from before the send is not this one")
    o.settle(tx(turn("u1", 1_000, "fix the build", true), turn("u2", 49_000, "fix the build", false)), working: false, now: 51_000)
    #expect(o.list.isEmpty)
}

@Test("a repeated send waits for its own prompt")
func aRepeatedSendWaitsForItsOwnPrompt() {
    var o = Outgoing()
    o.push("continue", working: false, now: 10_000)
    let first = tx(turn("u1", 10_100, "continue", true))
    o.settle(first, working: false, now: 10_200)
    o.push("continue", working: false, now: 11_000)
    o.settle(first, working: false, now: 11_100)
    #expect(o.list.count == 1)
    o.settle(tx(turn("u1", 10_100, "continue", true), turn("u2", 11_200, "continue", false)), working: false, now: 11_300)
    #expect(o.list.isEmpty)
}

@Test("a send not seen in two minutes is not delivered")
func aSendNotSeenInTwoMinutesIsNotDelivered() {
    var o = Outgoing()
    o.push("hello", working: false, now: 0)
    o.settle(Transcript(), working: false, now: 120_000)
    #expect(deliveries(o) == [.sending])
    o.settle(Transcript(), working: false, now: 120_001)
    #expect(deliveries(o) == [.failed])
}

@Test("a send queued while working is held until the turn ends")
func aSendQueuedWhileWorkingIsHeldUntilTheTurnEnds() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    o.settle(tx(turn("u1", 0, "long job", false)), working: false, now: 300_000)
    #expect(deliveries(o) == [.queued], "held past two minutes, never failed")
    #expect(released(&o).isEmpty)
    o.settle(tx(turn("u1", 0, "long job", true)), working: false, now: 400_000)
    #expect(released(&o) == ["next"])
    #expect(deliveries(o) == [.sending])
    #expect(released(&o).isEmpty, "handed over once")
}

@Test("queued sends go one per turn in the order queued")
func queuedSendsGoOnePerTurnInTheOrderQueued() {
    var o = Outgoing()
    o.push("first", working: true, now: 0)
    o.push("second", working: true, now: 1)
    let ended = tx(turn("u1", 0, "long job", true))
    o.settle(ended, working: false, now: 1_000)
    o.settle(ended, working: false, now: 1_100)
    #expect(released(&o) == ["first"], "the second waits while the first is on its way")
    o.settle(tx(turn("u1", 0, "long job", true), turn("u2", 1_050, "first", false)), working: false, now: 1_200)
    #expect(released(&o).isEmpty, "the agent works on the first")
    #expect(texts(o) == ["second"])
    o.settle(tx(turn("u1", 0, "long job", true), turn("u2", 1_050, "first", true)), working: false, now: 5_000)
    #expect(released(&o) == ["second"])
}

@Test("a send settles when text left in the terminal ran into its prompt")
func aSendSettlesWhenTextLeftInTheTerminalRanIntoItsPrompt() {
    var o = Outgoing()
    o.push("reply with just the word kappa", working: false, now: 0)
    o.push("next", working: true, now: 0)
    o.settle(tx(turn("u1", 100, "draftreply with just the word kappa", true)), working: false, now: 1_000)
    #expect(released(&o) == ["next"])
    #expect(texts(o) == ["next"])
}

@Test("a bash command sent from the chat settles by its echo")
func aBashCommandSentFromTheChatSettlesByItsEcho() {
    var o = Outgoing()
    o.push("!ls", working: false, now: 0)
    o.settle(tx(turn("u1", 100, "! ls", true)), working: false, now: 1_000)
    #expect(o.list.isEmpty)
}

@Test("a released send gets two minutes from its release")
func aReleasedSendGetsTwoMinutesFromItsRelease() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    let ended = tx(turn("u1", 0, "next", true))
    o.settle(tx(turn("u1", 0, "next", false)), working: false, now: 1_000)
    o.settle(ended, working: false, now: 400_000)
    #expect(deliveries(o) == [.sending], "an earlier prompt of the same text is not this one")
    o.settle(ended, working: false, now: 520_000)
    #expect(deliveries(o) == [.sending])
    o.settle(ended, working: false, now: 520_001)
    #expect(deliveries(o) == [.failed])
}

@Test("only a queued send holds the outgoing")
func onlyAQueuedSendHoldsTheOutgoing() {
    var o = Outgoing()
    o.push("sent", working: false, now: 0)
    #expect(!o.holds)
    o.push("next", working: true, now: 0)
    #expect(o.holds)
    o.settle(tx(turn("u1", 0, "sent", true)), working: false, now: 1_000)
    #expect(!o.holds, "released once the agent is idle")
}

@Test("a released send settles when its prompt arrives")
func aReleasedSendSettlesWhenItsPromptArrives() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    o.settle(tx(turn("u1", 0, "long job", true)), working: false, now: 400_000)
    o.settle(tx(turn("u1", 0, "long job", true), turn("u2", 400_500, "next", false)), working: false, now: 401_000)
    #expect(o.list.isEmpty)
}

@Test("withdrawing drops a queued send unsent and hands back its text")
func withdrawingDropsAQueuedSendUnsentAndHandsBackItsText() {
    var o = Outgoing()
    o.push("sent", working: false, now: 0)
    o.push("a", working: true, now: 0)
    o.push("b", working: true, now: 0)
    #expect(o.withdraw(id(o, "sent")) == nil, "what already went can't be taken back")
    #expect(o.withdraw(id(o, "a")) == "a")
    #expect(texts(o) == ["sent", "b"])
    o.settle(tx(turn("u1", 0, "sent", true)), working: false, now: 10)
    #expect(released(&o) == ["b"], "the next queued send moves up")
}

@Test("steering sends a queued send mid-turn")
func steeringSendsAQueuedSendMidTurn() {
    var o = Outgoing()
    o.push("a", working: true, now: 0)
    o.push("b", working: true, now: 0)
    #expect(o.steer(id(o, "b"), now: 50_000) == "b")
    #expect(o.steer(id(o, "b"), now: 50_000) == nil, "steered once")
    #expect(deliveries(o) == [.queued, .steered])
    o.settle(tx(turn("u1", 0, "long job", false)), working: false, now: 50_100)
    #expect(deliveries(o) == [.queued, .steered])
}

@Test("retry hands back a failed send")
func retryHandsBackAFailedSend() {
    var o = Outgoing()
    o.push("hello", working: false, now: 0)
    let hello = id(o, "hello")
    #expect(o.retry(hello) == nil)
    o.settle(Transcript(), working: false, now: 200_000)
    #expect(o.retry(hello) == "hello")
    #expect(o.list.isEmpty)
    #expect(o.retry(hello) == nil)
}

@Test("a turn holding only a compaction lets the queue go")
func aTurnHoldingOnlyACompactionLetsTheQueueGo() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    o.settle(tx(turn("u1", 0, "long job", true), compaction("c1")), working: false, now: 1_000)
    #expect(released(&o) == ["next"])
}

@Test("stopping holds the queue until resumed")
func stoppingHoldsTheQueueUntilResumed() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    let t = tx(stopped("u1", 0, "long job"))
    o.settle(t, working: false, now: 1_000)
    #expect(released(&o).isEmpty, "a stop holds what was queued behind it")
    #expect(o.paused(t))
    o.settle(t, working: false, now: 300_000)
    #expect(deliveries(o) == [.queued], "held, never failed")
}

@Test("resuming lets the next queued send go")
func resumingLetsTheNextQueuedSendGo() {
    var o = Outgoing()
    o.push("a", working: true, now: 0)
    o.push("b", working: true, now: 0)
    let t = tx(stopped("u1", 0, "long job"))
    o.settle(t, working: false, now: 1_000)
    o.resume(t)
    #expect(!o.paused(t))
    o.settle(t, working: false, now: 2_000)
    #expect(released(&o) == ["a"])
    #expect(deliveries(o) == [.sending, .queued])
}

@Test("a new failure after resuming pauses again")
func aNewFailureAfterResumingPausesAgain() {
    var o = Outgoing()
    o.push("a", working: true, now: 0)
    o.push("b", working: true, now: 0)
    let first = tx(stopped("u1", 0, "long job"))
    o.settle(first, working: false, now: 1_000)
    o.resume(first)
    o.settle(first, working: false, now: 2_000)
    #expect(released(&o) == ["a"])
    let second = tx(stopped("u1", 0, "long job"), stopped("u2", 2_100, "a"))
    o.settle(second, working: false, now: 3_000)
    #expect(o.paused(second))
    #expect(released(&o).isEmpty)
    #expect(texts(o) == ["b"])
}

@Test("a slash command never holds the queue")
func aSlashCommandNeverHoldsTheQueue() {
    var o = Outgoing()
    o.push("/compact", working: true, now: 0)
    o.push("next", working: true, now: 0)
    let ended = tx(turn("u1", 0, "long job", true))
    o.settle(ended, working: false, now: 1_000)
    #expect(released(&o) == ["/compact"])
    #expect(texts(o) == ["/compact", "next"])
    o.moved()
    o.settle(ended, working: false, now: 1_100)
    #expect(released(&o) == ["next"])
}

@Test("a slash command never turns failed")
func aSlashCommandNeverTurnsFailed() {
    var o = Outgoing()
    o.push(" /clear", working: false, now: 0)
    o.push("/model opus", working: true, now: 0)
    #expect(o.steer(id(o, "/model opus"), now: 0) == "/model opus")
    o.settle(Transcript(), working: false, now: 200_000)
    #expect(o.list.isEmpty)
}

@Test("a refused send fails at once and releases nothing")
func aRefusedSendFailsAtOnceAndReleasesNothing() {
    var o = Outgoing()
    sent(&o, "a", "prompt-1")
    o.push("a", working: true, now: 0)
    o.fail("prompt-1")
    #expect(deliveries(o) == [.failed, .queued], "a queued send of the same text never went")
    o.fail("prompt-1")
    #expect(deliveries(o) == [.failed, .queued])
    #expect(released(&o).isEmpty)
}

@Test("a refusal holds the queue until resumed")
func aRefusalHoldsTheQueueUntilResumed() {
    var o = Outgoing()
    sent(&o, "a", "prompt-1")
    o.push("b", working: true, now: 0)
    o.push("c", working: true, now: 0)
    o.fail("prompt-1")
    let idle = Transcript()
    o.settle(idle, working: false, now: 1_000)
    #expect(released(&o).isEmpty, "the next send would be refused too")
    #expect(o.paused(idle))
    o.resume(idle)
    o.settle(idle, working: false, now: 2_000)
    #expect(released(&o) == ["b"])
}

@Test("a send getting through after a refusal lets the queue go")
func aSendGettingThroughAfterARefusalLetsTheQueueGo() {
    var o = Outgoing()
    sent(&o, "a", "prompt-1")
    o.push("b", working: true, now: 0)
    o.fail("prompt-1")
    let retried = o.retry(id(o, "a"))!
    sent(&o, retried, "prompt-2")
    let done = tx(turn("u1", 100, "a", true))
    o.settle(done, working: false, now: 1_000)
    #expect(!o.paused(done))
    #expect(released(&o) == ["b"])
}

@Test("a refusal fails the send it names")
func aRefusalFailsTheSendItNames() {
    var o = Outgoing()
    sent(&o, "continue", "prompt-1")
    sent(&o, "continue", "prompt-2")
    o.fail("prompt-2")
    #expect(deliveries(o) == [.sending, .failed], "the first went through; the second was refused")
}

@Test("a prompt starting with a path is tracked like any other")
func aPromptStartingWithAPathIsTrackedLikeAnyOther() {
    var o = Outgoing()
    o.push("/Users/me/app/src/main.rs panics on start", working: false, now: 0)
    #expect(texts(o) == ["/Users/me/app/src/main.rs panics on start"])
    o.push("./run.sh fails", working: false, now: 0)
    #expect(o.list.count == 2)
}

@Test("draining returns queued and failed text in order")
func drainingReturnsQueuedAndFailedTextInOrder() {
    var o = Outgoing()
    o.push("lost", working: false, now: 0)
    o.settle(Transcript(), working: false, now: 200_000)
    o.push("sending", working: false, now: 200_000)
    o.push("next", working: true, now: 200_000)
    #expect(o.drain() == ["lost", "next"])
    #expect(texts(o) == ["sending"])
}

@Test("retrying one failed send leaves the others alone")
func retryingOneFailedSendLeavesTheOthersAlone() {
    var o = Outgoing()
    o.push("a", working: false, now: 0)
    o.push("b", working: false, now: 0)
    o.settle(Transcript(), working: false, now: 200_000)
    #expect(o.retry(id(o, "b")) == "b")
    #expect(texts(o) == ["a"])
    #expect(deliveries(o) == [.failed])
}

@Test("a steered send waits out a long turn without failing")
func aSteeredSendWaitsOutALongTurnWithoutFailing() {
    var o = Outgoing()
    o.push("b", working: true, now: 0)
    _ = o.steer(id(o, "b"), now: 50_000)
    o.settle(tx(turn("u1", 0, "long job", false)), working: false, now: 600_000)
    #expect(deliveries(o) == [.steered], "Claude holds it until its turn ends")
    let ended = tx(turn("u1", 0, "long job", true))
    o.settle(ended, working: false, now: 601_000)
    o.settle(ended, working: false, now: 721_000)
    #expect(deliveries(o) == [.steered], "two minutes from when the agent was seen idle")
    o.settle(ended, working: false, now: 721_001)
    #expect(deliveries(o) == [.failed], "an unsent prompt still fails after 120s idle")
}

@Test("a queued send waits while status says working and the transcript has no turns")
func aQueuedSendWaitsWhileStatusSaysWorkingAndTheTranscriptHasNoTurns() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    let refetching = Transcript()
    o.settle(refetching, working: true, now: 1_000)
    #expect(released(&o).isEmpty, "pocketd restarted mid-turn and its timeline is still on its way")
    #expect(busy(refetching, o, working: true))
    o.settle(refetching, working: false, now: 2_000)
    #expect(released(&o) == ["next"])
}

@Test("a command just sent reads as work before status shows it")
func aCommandJustSentReadsAsWorkBeforeStatusShowsIt() {
    var o = Outgoing()
    let ended = tx(turn("u1", 0, "long job", true))
    o.push("/compact", working: false, now: 0)
    #expect(busy(ended, o, working: false), "status trails it by a poll")
    o.settle(ended, working: true, now: 1_000)
    #expect(busy(ended, o, working: true))
    o.settle(ended, working: true, now: 300_000)
    #expect(o.list.isEmpty, "never Sending, never Failed")
    o.push("/compact", working: false, now: 300_000)
    o.moved()
    o.settle(ended, working: true, now: 300_001)
    #expect(!busy(ended, o, working: true), "once the timeline moves the transcript decides again")
}

@Test("a queued send waits behind a command until the agent shows what it did")
func aQueuedSendWaitsBehindACommandUntilTheAgentShowsWhatItDid() {
    var o = Outgoing()
    let ended = tx(turn("u1", 0, "long job", true))
    o.push("/compact", working: false, now: 0)
    o.push("next", working: true, now: 0)
    o.settle(ended, working: true, now: 1_000)
    #expect(released(&o).isEmpty)
    o.moved()
    o.settle(tx(turn("u1", 0, "long job", true), compaction("c1")), working: true, now: 5_000)
    #expect(released(&o) == ["next"], "the compaction shows; status trailing it holds nothing")
}

@Test("a command that shows nothing holds the queue only until status could have shown it")
func aCommandThatShowsNothingHoldsTheQueueOnlyUntilStatusCouldHaveShownIt() {
    var o = Outgoing()
    let ended = tx(turn("u1", 0, "long job", true))
    o.push("/model opus", working: false, now: 0)
    o.push("next", working: true, now: 0)
    o.settle(ended, working: false, now: 3_000)
    #expect(released(&o).isEmpty)
    o.settle(ended, working: false, now: 3_001)
    #expect(released(&o) == ["next"])
}

@Test("a steer waits until its prompt reaches the transcript, and a command never does")
func aSteerWaitsUntilItsPromptReachesTheTranscriptAndACommandNeverDoes() {
    var o = Outgoing()
    _ = o.steerText("also check the docs", now: 1_000)
    _ = o.steerText("/compact", now: 1_000)
    o.settle(open(0), working: true, now: 2_000)
    #expect(waiting(o) == ["also check the docs"])
    o.settle(tx(turn("u1", 0, "long job", false), turn("u2", 1_500, "also check the docs", false)), working: true, now: 3_000)
    #expect(waiting(o).isEmpty)
}

@Test("send now goes once for everything waiting until it shows")
func sendNowGoesOnceForEverythingWaitingUntilItShows() {
    var o = Outgoing()
    _ = o.steerText("ok?", now: 1_000)
    _ = o.steerText("run the store tests too", now: 1_100)
    let first = o.sendNow(now: 2_000)
    #expect(first)
    let again = o.sendNow(now: 2_100)
    #expect(o.sendingNow && !again, "one Send now covers every waiting send")
    o.settle(open(0), working: true, now: 2_500)
    #expect(o.sendingNow)
    o.settle(tx(turn("u1", 0, "long job", false), turn("u2", 1_000, "ok?", false), turn("u3", 1_100, "run the store tests too", false)), working: true, now: 3_000)
    #expect(waiting(o).isEmpty && !o.sendingNow)
}

@Test("a send now nothing came of can be pressed again")
func aSendNowNothingCameOfCanBePressedAgain() {
    var o = Outgoing()
    _ = o.steerText("ok?", now: 1_000)
    let first = o.sendNow(now: 2_000)
    #expect(first)
    o.settle(open(0), working: true, now: 2_000 + sendNowMs + 1)
    #expect(!o.sendingNow)
    let second = o.sendNow(now: 20_000)
    #expect(second)
}

@Test("send now has nothing to send for a queued send")
func sendNowHasNothingToSendForAQueuedSend() {
    var o = Outgoing()
    o.push("after this turn", working: true, now: 1_000)
    let first = o.sendNow(now: 2_000)
    #expect(!first)
    #expect(!o.sendingNow)
}

@Test("a stop pauses the queue before the turn shows it ended")
func aStopPausesTheQueueBeforeTheTurnShowsItEnded() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    o.settle(open(0), working: true, now: 1_000)
    #expect(!o.stopping && !o.paused(open(0)))
    o.stop(now: 2_000)
    #expect(o.stopping && o.paused(open(0)))
    o.settle(open(0), working: true, now: 3_500)
    #expect(o.stopping && o.paused(open(0)), "pocketd's interrupted result is on its way")
    let ended = tx(stopped("u1", 0, "long job"))
    o.settle(ended, working: true, now: 4_000)
    #expect(!o.stopping, "the turn ended")
    #expect(o.paused(ended), "the stop holds the queue as before")
    #expect(released(&o).isEmpty)
}

@Test("a stop the turn never shows lets go after ten seconds")
func aStopTheTurnNeverShowsLetsGoAfterTenSeconds() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    o.stop(now: 1_000)
    o.settle(open(0), working: true, now: 11_000)
    #expect(o.stopping)
    o.settle(open(0), working: true, now: 11_001)
    #expect(!o.stopping && !o.paused(open(0)))
}

@Test("a stop before the sent prompt echoes reads stopping until it does")
func aStopBeforeTheSentPromptEchoesReadsStoppingUntilItDoes() {
    var o = Outgoing()
    let sentId = o.push("hello", working: false, now: 0)!
    o.sentAs(sentId, "r1")
    o.stop(now: 100)
    o.settle(Transcript(), working: false, now: 200)
    #expect(busy(Transcript(), o, working: false) && o.stopping, "pocketd reads idle as soon as it sent the Esc")
    let t = tx(stopped("u1", 150, "hello"))
    o.settle(t, working: false, now: 1_000)
    #expect(!busy(t, o, working: false) && !o.stopping)
}

@Test("a steer the stop beat goes back to the queue, paused")
func aSteerTheStopBeatGoesBackToTheQueuePaused() {
    var o = Outgoing()
    _ = o.steerText("refactor the parser", now: 1_000)
    o.stop(now: 2_000)
    o.settle(open(0), working: true, now: 2_500)
    #expect(o.stopping && busy(open(0), o, working: true))
    let ended = tx(stopped("u1", 0, "long job"))
    o.settle(ended, working: false, now: 3_000)
    #expect(deliveries(o) == [.queued], "the agent never took it")
    #expect(!o.stopping && !busy(ended, o, working: false), "the composer is free as soon as the stop lands")
    #expect(o.paused(ended), "it waits for the user")
    #expect(released(&o).isEmpty)
    o.settle(ended, working: false, now: 300_000)
    #expect(deliveries(o) == [.queued], "never reads as not delivered")
}

@Test("a steer the stop beat waits for the user though the turn ended done")
func aSteerTheStopBeatWaitsForTheUserThoughTheTurnEndedDone() {
    var o = Outgoing()
    _ = o.steerText("refactor the parser", now: 1_000)
    o.stop(now: 2_000)
    let ended = tx(turn("u1", 0, "long job", true))
    o.settle(ended, working: false, now: 3_000)
    #expect(deliveries(o) == [.queued])
    #expect(o.paused(ended), "the user stopped; it waits for Resume")
    #expect(released(&o).isEmpty)
    o.resume(ended)
    o.settle(ended, working: false, now: 4_000)
    #expect(released(&o).count == 1)
}

@Test("a steer the agent took before the stop stays in the transcript")
func aSteerTheAgentTookBeforeTheStopStaysInTheTranscript() {
    var o = Outgoing()
    _ = o.steerText("refactor the parser", now: 1_000)
    o.stop(now: 2_000)
    o.settle(tx(turn("u1", 0, "long job", false), stopped("u2", 1_500, "refactor the parser")), working: false, now: 3_000)
    #expect(o.list.isEmpty)
}

@Test("a stopped steer the agent runs after all settles on its echo and is never sent again")
func aStoppedSteerTheAgentRunsAfterAllSettlesOnItsEchoAndIsNeverSentAgain() {
    var o = Outgoing()
    _ = o.steerText("refactor the parser", now: 1_000)
    o.stop(now: 2_000)
    o.settle(tx(stopped("u1", 0, "long job")), working: false, now: 3_000)
    #expect(deliveries(o) == [.queued])
    o.settle(tx(stopped("u1", 0, "long job"), turn("u2", 3_500, "refactor the parser", false)), working: true, now: 4_000)
    #expect(o.list.isEmpty, "its echo settles it")
    o.settle(tx(stopped("u1", 0, "long job"), turn("u2", 3_500, "refactor the parser", true)), working: false, now: 5_000)
    #expect(released(&o).isEmpty)
}

@Test("a queued send never sent ignores an echo of the same text")
func aQueuedSendNeverSentIgnoresAnEchoOfTheSameText() {
    var o = Outgoing()
    o.push("refactor the parser", working: true, now: 1_000)
    o.settle(tx(turn("u1", 0, "long job", true), turn("u2", 1_500, "refactor the parser", false)), working: true, now: 2_000)
    #expect(deliveries(o) == [.queued], "the user typed it in the terminal; this one still waits")
}

@Test("a refused stop lets go at once")
func aRefusedStopLetsGoAtOnce() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    o.stop(now: 1_000)
    o.stopRefused()
    #expect(!o.stopping && !o.paused(open(0)))
}

@Test("resuming while stopping lets the queue go once the turn ends")
func resumingWhileStoppingLetsTheQueueGoOnceTheTurnEnds() {
    var o = Outgoing()
    o.push("next", working: true, now: 0)
    o.stop(now: 1_000)
    o.resume(open(0))
    #expect(o.stopping && !o.paused(open(0)))
    o.settle(tx(stopped("u1", 0, "long job")), working: false, now: 2_000)
    #expect(released(&o) == ["next"])
}

@Test("a send queued behind a compact waits while status has yet to show it")
func aSendQueuedBehindACompactWaitsWhileStatusHasYetToShowIt() {
    var o = Outgoing()
    o.push("/compact", working: true, now: 0)
    o.push("next", working: true, now: 0)
    let ended = tx(turn("u1", 0, "long job", true))
    o.settle(ended, working: false, now: 1_000)
    #expect(released(&o) == ["/compact"])
    o.settle(ended, working: false, now: 1_500)
    #expect(released(&o).isEmpty, "pocketd's status trails the compaction by a poll")
    o.settle(ended, working: true, now: 2_500)
    o.settle(ended, working: true, now: 12_000)
    #expect(released(&o).isEmpty, "the compaction runs")
    o.settle(ended, working: false, now: 13_000)
    #expect(released(&o) == ["next"], "idle after working: the compaction is over")
}

@Test("a send queued behind a compact waits while status trails the turn before it")
func aSendQueuedBehindACompactWaitsWhileStatusTrailsTheTurnBeforeIt() {
    var o = Outgoing()
    o.push("/compact", working: true, now: 0)
    o.push("next", working: true, now: 0)
    o.settle(open(0), working: true, now: 500)
    let ended = tx(turn("u1", 0, "long job", true))
    o.settle(ended, working: true, now: 1_000)
    #expect(released(&o) == ["/compact"])
    o.settle(ended, working: true, now: 1_500)
    o.settle(ended, working: false, now: 2_000)
    #expect(released(&o).isEmpty, "the working status was the last turn's, not the compaction's")
    o.settle(ended, working: true, now: 2_500)
    o.settle(ended, working: false, now: 9_000)
    #expect(released(&o) == ["next"])
}

@Test("a command holds the queue a minute at most")
func aCommandHoldsTheQueueAMinuteAtMost() {
    var o = Outgoing()
    let ended = tx(turn("u1", 0, "long job", true))
    o.push("/compact", working: false, now: 0)
    o.push("next", working: true, now: 0)
    o.settle(ended, working: true, now: 60_000)
    #expect(released(&o).isEmpty)
    o.settle(ended, working: true, now: 60_001)
    #expect(released(&o) == ["next"], "status stuck on working holds nothing forever")
}

@Test("a steered command reads as work too")
func aSteeredCommandReadsAsWorkToo() {
    var o = Outgoing()
    o.push("/compact", working: true, now: 0)
    _ = o.steer(id(o, "/compact"), now: 0)
    #expect(deliveries(o) == [.steered])
    #expect(busy(tx(turn("u1", 0, "long job", true)), o, working: true))
}

@Test("a command shows as sending until its turn echoes it")
func aCommandShowsAsSendingUntilItsTurnEchoesIt() {
    var o = Outgoing()
    o.push("/goal tests pass", working: false, now: 0)
    o.settle(Transcript(), working: false, now: 1_000)
    #expect(deliveries(o) == [.sending], "a chat just made shows it while Claude starts")
    o.moved()
    o.settle(tx(turn("u1", 2_000, "/goal tests pass", false)), working: true, now: 2_500)
    #expect(o.list.isEmpty)
}

@Test("a steer shows as sent and settles by its echo")
func aSteerShowsAsSentAndSettlesByItsEcho() {
    var o = Outgoing()
    _ = o.steerText("also check the docs", now: 1_000)
    #expect(deliveries(o) == [.steered])
    o.settle(open(0), working: true, now: 2_000)
    #expect(o.echoed.count == 1)
    o.settle(tx(turn("u1", 0, "long job", true), turn("u2", 1_500, "also check the docs", false)), working: true, now: 3_000)
    #expect(o.list.isEmpty)
}

@Test("a command steered mid-turn shows until the turn ends")
func aCommandSteeredMidTurnShowsUntilTheTurnEnds() {
    var o = Outgoing()
    _ = o.steerText("/compact", now: 1_000)
    o.settle(open(0), working: true, now: 5_000)
    #expect(texts(o) == ["/compact"])
    o.settle(tx(turn("u1", 0, "long job", true)), working: false, now: 6_000)
    #expect(o.list.isEmpty, "it runs now, echoing nothing")
}

@Test("a send queued while stopping goes once the stop lands")
func aSendQueuedWhileStoppingGoesOnceTheStopLands() {
    var o = Outgoing()
    o.stop(now: 1_000)
    o.push("next", working: true, now: 1_500)
    o.settle(open(0), working: true, now: 2_000)
    #expect(released(&o).isEmpty)
    o.settle(tx(stopped("u1", 0, "long job")), working: false, now: 3_000)
    #expect(released(&o) == ["next"], "no Resume to press")
}

@Test("a sub-agent in the background leaves the chat free and holds no queue")
func aSubAgentInTheBackgroundLeavesTheChatFreeAndHoldsNoQueue() {
    var audit = turn("u1", 0, "audit", true)
    audit.blocks = [.tool(Call(id: "a", name: "", kind: .agent(SubAgent(description: "")), status: .running))]
    let t = tx(audit)
    var o = Outgoing()
    #expect(!busy(t, o, working: false), "a send starts a turn rather than steering into one")
    o.push("next", working: true, now: 0)
    o.settle(t, working: false, now: 1_000)
    #expect(released(&o) == ["next"])
}

@Test("working with an empty draft offers stop")
func workingWithAnEmptyDraftOffersStop() { #expect(action(working: true, stopping: false, empty: true) == .stop) }

@Test("working with a draft offers steer")
func workingWithADraftOffersSteer() { #expect(action(working: true, stopping: false, empty: false) == .steer) }

@Test("idle offers send")
func idleOffersSend() {
    #expect(action(working: false, stopping: false, empty: true) == .send)
    #expect(action(working: false, stopping: false, empty: false) == .send)
}

@Test("a turn being stopped offers no second stop but still queues")
func aTurnBeingStoppedOffersNoSecondStopButStillQueues() {
    #expect(action(working: true, stopping: true, empty: true) == .stopping)
    #expect(action(working: true, stopping: true, empty: false) == .queue)
    #expect(action(working: false, stopping: true, empty: true) == .send, "the turn ended before the stop cleared")
}
