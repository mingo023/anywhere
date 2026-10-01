export type ComposerAction = "send" | "queue" | "stop";

/** Whether text sent mid-turn waits for the next turn. */
export const QUEUES: Readonly<Record<string, boolean>> = { claude: false, codex: false };

export function composerAction(working: boolean, text: string): ComposerAction {
  if (!working) return "send";
  return text.trim() ? "queue" : "stop";
}

export function composerLabel(action: ComposerAction, provider: string): "Send" | "Queue" | "Stop" {
  if (action === "stop") return "Stop";
  return action === "queue" && QUEUES[provider] ? "Queue" : "Send";
}

export function sendDisabled(working: boolean, text: string): boolean {
  return !working && !text.trim();
}
