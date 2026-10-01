import React from "react";
import { Pressable, Text, View } from "react-native";
import type { Creds } from "../credentials";
import { useSession } from "../session";
import { pairStyles as s } from "./pairStyles";

export function ReconnectScreen({ creds }: { creds: Creds }) {
  const { retry } = useSession();
  return (
    <View style={s.root}>
      <Text style={s.heading}>{creds.macName ?? creds.host}</Text>
      <Text style={s.host}>{creds.host}</Text>
      <Pressable style={s.button} onPress={retry}>
        <Text style={s.buttonText}>Connect</Text>
      </Pressable>
    </View>
  );
}
