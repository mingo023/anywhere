import React, { useState } from "react";
import { Pressable, StyleSheet, TextInput, View } from "react-native";
import { d, font } from "../design";
import { ArrowUp, Mic, Plus, Stop } from "../icons";
import { Glass } from "./Glass";
import { composerAction, composerLabel, sendDisabled } from "../composer";

type Props = {
  placeholder: string;
  working: boolean;
  /** Provider id, which picks the Queue label. */
  provider: string;
  providerName: string;
  paddingBottom: number;
  onSend: (text: string) => void;
  onInterrupt: () => void;
};

export function Composer({ placeholder, working, provider, providerName, paddingBottom, onSend, onInterrupt }: Props) {
  const [text, setText] = useState("");
  const action = composerAction(working, text);
  const label = composerLabel(action, provider);
  const disabled = sendDisabled(working, text);

  const submit = () => {
    const trimmed = text.trim();
    if (!trimmed) return;
    onSend(trimmed);
    setText("");
  };

  return (
    <View style={[styles.footer, { paddingBottom }]} pointerEvents="box-none">
      <View style={styles.bar}>
        <Pressable accessibilityLabel="Attach">
          <Glass style={styles.circle} interactive>
            <Plus size={20} color={d.muted} />
          </Glass>
        </Pressable>

        <Glass style={styles.inputPill}>
          <TextInput
            style={styles.input}
            value={text}
            onChangeText={setText}
            onSubmitEditing={submit}
            placeholder={placeholder}
            placeholderTextColor={d.muted}
            returnKeyType="send"
          />
          <Pressable style={styles.iconButton} accessibilityLabel="Dictate">
            <Mic size={20} color={d.muted} />
          </Pressable>
        </Glass>

        <Pressable
          style={[styles.send, disabled && styles.disabled]}
          disabled={disabled}
          accessibilityLabel={label}
          accessibilityHint={label === "Queue" ? `Sends while ${providerName} works` : undefined}
          accessibilityState={{ disabled }}
          onPress={action === "stop" ? onInterrupt : submit}
        >
          {action === "stop" ? <Stop size={20} color={d.bg} /> : <ArrowUp size={20} color={d.bg} />}
        </Pressable>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  footer: { paddingTop: 10, paddingHorizontal: 12 },
  bar: { flexDirection: "row", alignItems: "flex-end", gap: 8 },
  circle: {
    width: 44,
    height: 44,
    borderRadius: 22,
    overflow: "hidden",
    alignItems: "center",
    justifyContent: "center",
  },
  inputPill: {
    flex: 1,
    minHeight: 44,
    borderRadius: 22,
    overflow: "hidden",
    flexDirection: "row",
    alignItems: "center",
    paddingLeft: 16,
    paddingRight: 4,
  },
  input: { flex: 1, height: 44, color: d.text, fontSize: 15, fontFamily: font.regular },
  iconButton: { width: 36, height: 36, alignItems: "center", justifyContent: "center" },
  send: {
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: d.green,
    alignItems: "center",
    justifyContent: "center",
  },
  disabled: { opacity: 0.4 },
});
