import React from "react";
import { Pressable, Text, View } from "react-native";
import { pairStyles as s } from "./pairStyles";

export function PairScreen({ onEnterCode }: { onEnterCode: () => void }) {
  return (
    <View style={s.root}>
      <Text style={s.heading}>Pair with your Mac</Text>
      <Text style={s.body}>On your Mac, press ⌘K → Pair phone, then scan the code with the Camera app.</Text>
      <Pressable onPress={onEnterCode}>
        <Text style={s.link}>Enter code instead</Text>
      </Pressable>
    </View>
  );
}
