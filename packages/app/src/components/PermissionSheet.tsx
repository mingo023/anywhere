import React from "react";
import { Modal, Pressable, StyleSheet, Text, View } from "react-native";
import type { PermissionRequest } from "@pocket/protocol";
import { theme } from "../theme";

type Props = {
  request?: PermissionRequest;
  onResolve: (requestId: string, decision: "allow" | "deny") => void;
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

export function PermissionSheet({ request, onResolve }: Props) {
  return (
    <Modal visible={!!request} transparent animationType="slide">
      <View style={styles.backdrop}>
        <View style={styles.sheet}>
          <Text style={styles.title}>{request?.toolName}</Text>
          <Text style={styles.detail}>{request ? describe(request) : ""}</Text>
          <View style={styles.actions}>
            <Pressable
              style={[styles.button, styles.deny]}
              onPress={() => request && onResolve(request.requestId, "deny")}
            >
              <Text style={styles.denyText}>Deny</Text>
            </Pressable>
            <Pressable
              style={[styles.button, styles.allow]}
              onPress={() => request && onResolve(request.requestId, "allow")}
            >
              <Text style={styles.allowText}>Allow</Text>
            </Pressable>
          </View>
        </View>
      </View>
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
    gap: 12,
  },
  title: { color: theme.text, fontSize: 16, fontWeight: "600" },
  detail: {
    color: theme.muted,
    fontSize: 13,
    fontFamily: "Menlo",
    backgroundColor: theme.surfaceAlt,
    borderRadius: theme.radius,
    padding: 12,
  },
  actions: { flexDirection: "row", gap: 12 },
  button: { flex: 1, borderRadius: theme.radius, paddingVertical: 14, alignItems: "center" },
  deny: { backgroundColor: theme.surfaceAlt, borderWidth: 1, borderColor: theme.border },
  allow: { backgroundColor: theme.accent },
  denyText: { color: theme.text, fontWeight: "600" },
  allowText: { color: theme.bg, fontWeight: "600" },
});
