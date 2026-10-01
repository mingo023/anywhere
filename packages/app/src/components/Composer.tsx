import React, { useState } from "react";
import { Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import type { ContextChip, ContextTone } from "../context";
import { d, font } from "../design";
import { ArrowUp, Stop } from "../icons";
import { Glass } from "./Glass";
import { composerAction, composerLabel, sendDisabled } from "../composer";

type Props = {
  placeholder: string;
  working: boolean;
  offline: boolean;
  initialText: string;
  /** Provider id, which picks the Queue label. */
  provider: string;
  providerName: string;
  paddingBottom: number;
  onChangeText: (text: string) => void;
  onSend: (text: string) => void;
  onInterrupt: () => void;
  context?: ContextChip;
};

const toneColor: Record<ContextTone, string> = { muted: d.muted, warning: d.warning, danger: d.red };

export function Composer({
  placeholder,
  working,
  offline,
  initialText,
  provider,
  providerName,
  paddingBottom,
  onChangeText,
  onSend,
  onInterrupt,
  context,
}: Props) {
  const [text, setText] = useState(initialText);
  const action = composerAction(working, text);
  const label = composerLabel(action, provider);
  const disabled = sendDisabled(working, text) || offline;

  const change = (next: string) => {
    setText(next);
    onChangeText(next);
  };

  const submit = () => {
    const trimmed = text.trim();
    if (!trimmed || offline) return;
    onSend(trimmed);
    setText("");
  };

  return (
    <View style={[styles.footer, { paddingBottom }]} pointerEvents="box-none">
      {context ? (
        <Text
          style={[styles.context, { color: toneColor[context.tone] }]}
          accessibilityLabel={`${context.percent} percent of context used`}
        >
          {context.percent}% context
        </Text>
      ) : null}
      <View style={styles.bar}>
        <Glass style={styles.inputPill}>
          <TextInput
            style={styles.input}
            value={text}
            onChangeText={change}
            onSubmitEditing={submit}
            placeholder={placeholder}
            placeholderTextColor={d.muted}
            returnKeyType="send"
          />
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
  context: { alignSelf: "flex-end", marginBottom: 6, marginRight: 4, fontSize: 12, fontFamily: font.medium },
  inputPill: {
    flex: 1,
    minHeight: 44,
    borderRadius: 22,
    overflow: "hidden",
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 16,
  },
  input: { flex: 1, height: 44, color: d.text, fontSize: 15, fontFamily: font.regular },
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
