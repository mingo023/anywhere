import React from "react";
import { Pressable, Text, View } from "react-native";
import { pairStyles as s } from "./pairStyles";

export function KeychainScreen({ onRetry }: { onRetry: () => void }) {
  return (
    <View style={s.root}>
      <Text style={s.heading}>Can't open this phone's saved pairing.</Text>
      <Text style={s.body}>Unlock the phone, then try again.</Text>
      <Pressable style={s.button} onPress={onRetry}>
        <Text style={s.buttonText}>Try again</Text>
      </Pressable>
    </View>
  );
}
