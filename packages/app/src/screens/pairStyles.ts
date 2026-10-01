import { StyleSheet } from "react-native";
import { font } from "../design";
import { theme } from "../theme";

export const pairStyles = StyleSheet.create({
  root: { flex: 1, justifyContent: "center", padding: 24, gap: 8 },
  heading: { color: theme.text, fontSize: 28, fontWeight: "700", marginBottom: 16 },
  body: { color: theme.muted, fontSize: 15, lineHeight: 21 },
  host: { color: theme.muted, fontFamily: font.mono, fontSize: 13 },
  label: { color: theme.muted, fontSize: 12, marginTop: 8 },
  input: {
    color: theme.text,
    backgroundColor: theme.surfaceAlt,
    borderWidth: 1,
    borderColor: theme.border,
    borderRadius: theme.radius,
    padding: 12,
    fontSize: 15,
  },
  button: {
    backgroundColor: theme.accent,
    borderRadius: theme.radius,
    paddingVertical: 14,
    alignItems: "center",
    marginTop: 24,
  },
  disabled: { opacity: 0.4 },
  buttonText: { color: theme.bg, fontWeight: "700", fontSize: 15 },
  link: { color: theme.accent, fontSize: 15, textAlign: "center", marginTop: 16 },
  error: { color: theme.error, fontSize: 13, textAlign: "center", marginTop: 12 },
});
