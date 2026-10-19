/** What the phone knows of each agent: its name, and whether text sent mid-turn waits for the next turn. */
export const PROVIDERS: Readonly<Record<string, { label: string; queues: boolean }>> = {
  claude: { label: "Claude", queues: false },
  codex: { label: "Codex", queues: false },
};
