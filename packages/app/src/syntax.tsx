import React from "react";
import { Text, type TextStyle } from "react-native";
import { d } from "./design";

const KEYWORDS = new Set([
  "import", "export", "from", "type", "interface", "const", "let", "var", "function", "return",
  "if", "else", "for", "while", "switch", "case", "class", "struct", "enum", "extends",
  "implements", "new", "async", "await", "try", "catch", "throw", "true", "false", "null",
  "undefined", "this", "in", "of", "as", "is", "void", "public", "private", "protected",
  "static", "default", "package", "def", "func",
]);

const WORD = /\b([A-Za-z_][A-Za-z0-9_]*)\b/g;

/** Enough colour to read a hunk at a glance, without shipping a grammar. */
export function highlight(text: string, style: TextStyle): React.ReactNode {
  const lead = text.trimStart();
  if (lead.startsWith("//") || lead.startsWith("#")) return <Text style={[style, { color: d.faint }]}>{text}</Text>;

  const parts: React.ReactNode[] = [];
  let last = 0;
  let match: RegExpExecArray | null;
  WORD.lastIndex = 0;

  while ((match = WORD.exec(text))) {
    if (match.index > last) parts.push(text.slice(last, match.index));
    const word = match[1]!;
    const color = KEYWORDS.has(word) ? d.keyword : /^[A-Z]/.test(word) ? d.typeName : undefined;
    parts.push(
      color ? (
        <Text key={match.index} style={{ color }}>
          {word}
        </Text>
      ) : (
        word
      ),
    );
    last = match.index + word.length;
  }
  if (last < text.length) parts.push(text.slice(last));

  return <Text style={style}>{parts}</Text>;
}
