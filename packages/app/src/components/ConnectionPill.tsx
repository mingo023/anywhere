import React from "react";
import { Pressable, StyleSheet, Text } from "react-native";
import { d, font } from "../design";
import type { Banner } from "../connectivity";
import { Glass } from "./Glass";

export function ConnectionPill({ banner, onPress }: { banner: Banner | null; onPress: () => void }) {
  if (!banner || banner.kind === "connecting") return null;

  return (
    <Pressable style={styles.wrap} accessibilityLabel="Reconnect now" onPress={onPress}>
      <Glass style={styles.pill} interactive>
        <Text style={styles.text}>{banner.text}</Text>
      </Glass>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  wrap: { alignSelf: "center", maxWidth: "90%", marginBottom: 10 },
  pill: {
    minHeight: 30,
    borderRadius: 15,
    overflow: "hidden",
    paddingHorizontal: 14,
    paddingVertical: 6,
    justifyContent: "center",
  },
  text: { color: d.text, fontSize: 12.5, fontFamily: font.medium, textAlign: "center" },
});
