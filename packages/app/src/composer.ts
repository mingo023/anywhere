import { PROVIDERS } from "./providers.ts";

export type ComposerAction = "send" | "queue" | "stop";

export function composerAction(working: boolean, text: string): ComposerAction {
  if (!working) return "send";
  return text.trim() ? "queue" : "stop";
}

export function composerLabel(action: ComposerAction, provider: string): "Send" | "Queue" | "Stop" {
  if (action === "stop") return "Stop";
  return action === "queue" && PROVIDERS[provider]?.queues ? "Queue" : "Send";
}

export function sendDisabled(working: boolean, text: string): boolean {
  return !working && !text.trim();
}
