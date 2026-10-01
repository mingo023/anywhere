import React, { useEffect, useState } from "react";
import { KeyboardAvoidingView, Platform, Pressable, Text, TextInput } from "react-native";
import { savedHost } from "../credentials";
import { keychain } from "../keychain";
import { isCode, isHost, type PairLink } from "../pairing";
import { theme } from "../theme";
import { pairStyles as s } from "./pairStyles";

export function EnterCodeScreen({ onContinue, onBack }: { onContinue: (link: PairLink) => void; onBack: () => void }) {
  const [host, setHost] = useState("");
  const [code, setCode] = useState("");
  const ready = isHost(host.trim()) && isCode(code.trim());

  useEffect(() => {
    savedHost(keychain).then((saved) => setHost((typed) => typed || saved || ""), () => {});
  }, []);

  return (
    <KeyboardAvoidingView style={s.root} behavior={Platform.OS === "ios" ? "padding" : "height"}>
      <Text style={s.label}>Mac address</Text>
      <TextInput
        style={s.input}
        value={host}
        onChangeText={setHost}
        placeholder="mac-mini.tail1234.ts.net:4517"
        placeholderTextColor={theme.muted}
        autoCapitalize="none"
        autoCorrect={false}
      />
      <Text style={s.label}>Code</Text>
      <TextInput
        style={s.input}
        value={code}
        onChangeText={setCode}
        placeholder="22 characters from your Mac"
        placeholderTextColor={theme.muted}
        autoCapitalize="none"
        autoCorrect={false}
      />
      <Pressable
        style={[s.button, !ready && s.disabled]}
        disabled={!ready}
        onPress={() => onContinue({ host: host.trim(), code: code.trim(), name: host.trim() })}
      >
        <Text style={s.buttonText}>Continue</Text>
      </Pressable>
      <Pressable onPress={onBack}>
        <Text style={s.link}>Back</Text>
      </Pressable>
    </KeyboardAvoidingView>
  );
}
