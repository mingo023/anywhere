import Testing

@testable import ChatModel

private func share(_ status: CallStatus, _ caption: String = "", _ output: String? = nil) -> Call {
    Call(id: "t", name: SharedFile.tool, kind: .shared(path: "out/chart.png", caption: caption), status: status, output: output)
}

@Test("only a share that went through has a kept copy")
func onlyAShareThatWentThroughHasAKeptCopy() {
    #expect(SharedFile.kept(share(.ok, "", "/h/attachments/0a1b-chart.png\n")) == "/h/attachments/0a1b-chart.png")
    #expect(SharedFile.kept(share(.running)) == nil)
    #expect(SharedFile.kept(share(.failed, "", "There's no file at out/chart.png")) == nil)
    #expect(SharedFile.kept(share(.ok, "", " ")) == nil)
}

@Test("a card reads the caption, that it is on its way, or why it failed")
func aCardReadsTheCaptionThatItIsOnItsWayOrWhyItFailed() {
    #expect(SharedFile.detail(share(.ok, "Q3  revenue\nby month", "/k")) == "Q3 revenue by month")
    #expect(SharedFile.detail(share(.running, "Q3")) == "Sharing…")
    #expect(SharedFile.detail(share(.failed, "Q3", "Files over 20 MB can't be shared\n")) == "Files over 20 MB can't be shared")
    #expect(SharedFile.detail(share(.failed, "Q3")) == "Couldn't share it")
}

@Test("the phone previews images and text but not files only an app opens")
func thePhonePreviewsImagesAndTextButNotFilesOnlyAnAppOpens() {
    #expect(SharedFile.previews("/r/Chart.PNG"))
    #expect(SharedFile.previews("/r/notes.md"))
    #expect(SharedFile.previews("/r.d/Makefile"))
    #expect(!SharedFile.previews("/r/report.PDF"))
    #expect(!SharedFile.previews("/r/clip.mov"))
}

@Test("a card names the file, and its copy is read from its folder by its name")
func aCardNamesTheFileAndItsCopyIsReadFromItsFolderByItsName() {
    #expect(SharedFile.name("/r/out/chart.png") == "chart.png")
    #expect(SharedFile.name(#"C:\r\chart.png"#) == "chart.png")
    let place = SharedFile.place("/Users/me/.coding-pocket/attachments/0a1b-chart.png")
    #expect(place.tree == "/Users/me/.coding-pocket/attachments" && place.path == "0a1b-chart.png")
}
