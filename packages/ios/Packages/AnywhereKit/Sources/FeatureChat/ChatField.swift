/// The chat's text fields, one focus between them, so a tap or swipe down the transcript lets go of whichever holds the
/// keyboard.
enum ChatField: Hashable {
    case composer, reply, answer
}
