import React from "react";
import { Pressable, Text, View } from "react-native";
import { pairStyles as s } from "./pairStyles";

export function UpdateScreen({ side, onRetry }: { side: "updateMac" | "updatePhone"; onRetry: () => void }) {
  return (
    <View style={s.root}>
      <Text style={s.heading}>{side === "updateMac" ? "Update Pocket on your Mac." : "Update Pocket on this phone."}</Text>
      <Pressable style={s.button} onPress={onRetry}>
        <Text style={s.buttonText}>Try again</Text>
      </Pressable>
    </View>
  );
}
