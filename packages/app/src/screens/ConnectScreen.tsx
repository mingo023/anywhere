import React, { useEffect, useState } from "react";
import { ActivityIndicator, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import * as SecureStore from "expo-secure-store";
import { theme } from "../theme";
import { useSession } from "../session";

const HOST_KEY = "pocket.host";
const TOKEN_KEY = "pocket.token";

export function ConnectScreen() {
  const { state, connect } = useSession();
  const [host, setHost] = useState("");
  const [token, setToken] = useState("");

  useEffect(() => {
    void (async () => {
      setHost((await SecureStore.getItemAsync(HOST_KEY)) ?? "");
      setToken((await SecureStore.getItemAsync(TOKEN_KEY)) ?? "");
    })();
  }, []);

  const submit = async () => {
    await SecureStore.setItemAsync(HOST_KEY, host);
    await SecureStore.setItemAsync(TOKEN_KEY, token);
    connect(host, token);
  };

  return (
    <View style={styles.root}>
      <Text style={styles.heading}>Coding Pocket</Text>
      <Text style={styles.label}>Host</Text>
      <TextInput
        style={styles.input}
        value={host}
        onChangeText={setHost}
        placeholder="100.77.122.82:4517"
        placeholderTextColor={theme.muted}
        autoCapitalize="none"
        autoCorrect={false}
      />
      <Text style={styles.label}>Token</Text>
      <TextInput
        style={styles.input}
        value={token}
        onChangeText={setToken}
        placeholder="daemon token"
        placeholderTextColor={theme.muted}
        autoCapitalize="none"
        autoCorrect={false}
        secureTextEntry
      />
      <Pressable style={styles.button} onPress={submit} disabled={state === "connecting"}>
        {state === "connecting" ? (
          <ActivityIndicator color={theme.bg} />
        ) : (
          <Text style={styles.buttonText}>Connect</Text>
        )}
      </Pressable>
      {state === "offline" ? <Text style={styles.error}>Disconnected — retrying</Text> : null}
    </View>
  );
}

const styles = StyleSheet.create({
  root: { flex: 1, justifyContent: "center", padding: 24, gap: 8 },
  heading: { color: theme.text, fontSize: 28, fontWeight: "700", marginBottom: 24 },
  label: { color: theme.muted, fontSize: 12, marginTop: 8 },
  input: {
    color: theme.text,
    backgroundColor: theme.surfaceAlt,
    borderWidth: 1,
    borderColor: theme.border,
    borderRadius: theme.radius,
    padding: 12,
    fontSize: 15,
  },
  button: {
    backgroundColor: theme.accent,
    borderRadius: theme.radius,
    paddingVertical: 14,
    alignItems: "center",
    marginTop: 24,
  },
  buttonText: { color: theme.bg, fontWeight: "700", fontSize: 15 },
  error: { color: theme.error, fontSize: 12, textAlign: "center", marginTop: 12 },
});
