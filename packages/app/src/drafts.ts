export type Drafts = {
  get(agentId: string): string;
  set(agentId: string, text: string): void;
  clear(agentId: string): void;
};

export function createDrafts(): Drafts {
  const texts = new Map<string, string>();
  return {
    get: (agentId) => texts.get(agentId) ?? "",
    set: (agentId, text) => (text ? texts.set(agentId, text) : texts.delete(agentId)),
    clear: (agentId) => texts.delete(agentId),
  };
}
