import * as SecureStore from "expo-secure-store";
import type { KV } from "./credentials";

const options = { keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY };

export const keychain: KV = {
  getItemAsync: (key) => SecureStore.getItemAsync(key, options),
  setItemAsync: (key, value) => SecureStore.setItemAsync(key, value, options),
  deleteItemAsync: (key) => SecureStore.deleteItemAsync(key, options),
};
