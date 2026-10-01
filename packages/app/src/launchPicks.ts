import { keychain } from "./keychain";
import type { Picks } from "./launch";

const KEY = "pocket.launch";

export async function loadPicks(): Promise<Picks> {
  try {
    const saved = JSON.parse((await keychain.getItemAsync(KEY)) ?? "{}") as Partial<Picks>;
    return { ...saved, projects: saved.projects ?? {} };
  } catch {
    return { projects: {} };
  }
}

export function savePicks(picks: Picks): Promise<void> {
  return keychain.setItemAsync(KEY, JSON.stringify(picks));
}
