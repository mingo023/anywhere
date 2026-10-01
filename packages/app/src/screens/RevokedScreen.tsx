import React from "react";
import { Pressable, Text, View } from "react-native";
import { pairStyles as s } from "./pairStyles";

export function RevokedScreen({ onPairAgain }: { onPairAgain: () => void }) {
  return (
    <View style={s.root}>
      <Text style={s.heading}>This phone was removed.</Text>
      <Pressable style={s.button} onPress={onPairAgain}>
        <Text style={s.buttonText}>Pair again</Text>
      </Pressable>
    </View>
  );
}
