import React, { useRef, useState } from "react";
import { ActivityIndicator, KeyboardAvoidingView, Platform, Pressable, Text, TextInput } from "react-native";
import { PairError, pairErrorText, type PairLink } from "../pairing";
import { useSession } from "../session";
import { theme } from "../theme";
import { pairStyles as s } from "./pairStyles";

export function ConfirmPairScreen({ link, onDone }: { link: PairLink; onDone: () => void }) {
  const { pairWith } = useSession();
  const [name, setName] = useState(Platform.OS === "ios" ? "iPhone" : "Android phone");
  const [busy, setBusy] = useState(false);
  const pairing = useRef(false);
  const [error, setError] = useState<string>();

  const submit = async () => {
    if (pairing.current) return;
    pairing.current = true;
    setBusy(true);
    setError(undefined);
    try {
      await pairWith(link, name);
      onDone();
    } catch (e) {
      setError(pairErrorText(e instanceof PairError ? e.code : "pair_failed", link.host));
      pairing.current = false;
      setBusy(false);
    }
  };

  return (
    <KeyboardAvoidingView style={s.root} behavior={Platform.OS === "ios" ? "padding" : "height"}>
      <Text style={s.heading}>Pair with {link.name}?</Text>
      <Text style={s.host}>{link.host}</Text>
      <Text style={s.label}>Name on your Mac</Text>
      <TextInput style={s.input} value={name} onChangeText={setName} autoCorrect={false} />
      <Text style={s.body}>A paired phone can run commands on this Mac as you.</Text>
      <Pressable style={s.button} onPress={submit} disabled={busy}>
        {busy ? <ActivityIndicator color={theme.bg} /> : <Text style={s.buttonText}>Pair</Text>}
      </Pressable>
      <Pressable onPress={onDone} disabled={busy}>
        <Text style={s.link}>Cancel</Text>
      </Pressable>
      {error ? <Text style={s.error}>{error}</Text> : null}
    </KeyboardAvoidingView>
  );
}
