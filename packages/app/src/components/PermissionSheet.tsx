import React, { useEffect, useState } from "react";
import { KeyboardAvoidingView, Modal, Pressable, StyleSheet, Text, TextInput, View } from "react-native";
import type { PermissionRequest } from "@pocket/protocol";
import type { PermissionAnswer } from "../session";
import { theme } from "../theme";

type Props = {
  request?: PermissionRequest;
  /** "Bash · 1 of 2"; the tool name when unset. */
  header?: string;
  onResolve: (requestId: string, decision: "allow" | "deny", answer?: PermissionAnswer) => void;
};

function describe(request: PermissionRequest): string {
  const d = request.detail;
  switch (d.kind) {
    case "shell":
      return d.command;
    case "read":
    case "edit":
    case "write":
      return d.path;
    case "search":
      return d.query;
    case "task":
      return d.description;
    case "other":
      return JSON.stringify(d.input).slice(0, 300);
  }
}

type Choice = { label: string; decision: "allow" | "deny"; option?: string };

function choices(request: PermissionRequest): Choice[] {
  return [
    { label: "Yes", decision: "allow" },
    ...(request.options ?? []).map((o) => ({ label: o.label, decision: "allow" as const, option: o.id })),
    { label: "No", decision: "deny" },
  ];
}

export function PermissionSheet({ request, header, onResolve }: Props) {
  const [feedback, setFeedback] = useState("");
  useEffect(() => setFeedback(""), [request?.requestId]);

  const sendFeedback = () => {
    const message = feedback.trim();
    if (request && message) onResolve(request.requestId, "deny", { message });
  };

  return (
    <Modal visible={!!request} transparent animationType="slide">
      <KeyboardAvoidingView behavior="padding" style={styles.backdrop}>
        {request && (
          <View style={styles.sheet}>
            <Text style={styles.title}>{header ?? request.toolName}</Text>
            <View style={styles.detailBox}>
              <Text style={styles.detail}>{describe(request)}</Text>
              {request.detail.kind === "shell" && !!request.detail.description && (
                <Text style={styles.description}>{request.detail.description}</Text>
              )}
            </View>
            <Text style={styles.question}>Do you want to proceed?</Text>
            {choices(request).map((c, i) => (
              <Pressable
                key={c.option ?? c.decision}
                style={[styles.choice, i === 0 && styles.primary]}
                onPress={() => onResolve(request.requestId, c.decision, c.option ? { option: c.option } : undefined)}
              >
                <Text style={[styles.choiceText, i === 0 && styles.primaryText]}>
                  {i + 1}. {c.label}
                </Text>
              </Pressable>
            ))}
            {request.feedback && (
              <TextInput
                style={styles.feedback}
                value={feedback}
                onChangeText={setFeedback}
                onSubmitEditing={sendFeedback}
                placeholder="No, and tell Claude what to do differently"
                placeholderTextColor={theme.muted}
                returnKeyType="send"
              />
            )}
          </View>
        )}
      </KeyboardAvoidingView>
    </Modal>
  );
}

const styles = StyleSheet.create({
  backdrop: { flex: 1, justifyContent: "flex-end", backgroundColor: "#0008" },
  sheet: {
    backgroundColor: theme.surface,
    borderTopLeftRadius: 20,
    borderTopRightRadius: 20,
    padding: 20,
    paddingBottom: 40,
    gap: 10,
  },
  title: { color: theme.text, fontSize: 16, fontWeight: "600" },
  detailBox: { backgroundColor: theme.surfaceAlt, borderRadius: theme.radius, padding: 12, gap: 6 },
  detail: { color: theme.text, fontSize: 13, fontFamily: "Menlo" },
  description: { color: theme.muted, fontSize: 13 },
  question: { color: theme.text, fontSize: 14, marginTop: 4 },
  choice: {
    borderRadius: theme.radius,
    borderWidth: 1,
    borderColor: theme.border,
    backgroundColor: theme.surfaceAlt,
    paddingVertical: 12,
    paddingHorizontal: 14,
  },
  primary: { backgroundColor: theme.accent, borderColor: theme.accent },
  choiceText: { color: theme.text, fontWeight: "500" },
  primaryText: { color: theme.bg, fontWeight: "600" },
  feedback: {
    color: theme.text,
    fontSize: 14,
    borderRadius: theme.radius,
    borderWidth: 1,
    borderColor: theme.border,
    paddingVertical: 12,
    paddingHorizontal: 14,
  },
});
