import React, { useMemo } from "react";
import { Linking, ScrollView, StyleSheet, Text, View } from "react-native";
import { d, font } from "../design";
import { highlight } from "../syntax";

type Block =
  | { kind: "code"; lang: string; text: string }
  | { kind: "heading"; level: number; text: string }
  | { kind: "quote"; text: string }
  | { kind: "list"; ordered: boolean; items: string[] }
  | { kind: "table"; header: string[]; rows: string[][] }
  | { kind: "rule" }
  | { kind: "paragraph"; text: string };

const FENCE = /^ {0,3}(?:```|~~~)\s*(\S*)/;
const HEADING = /^ {0,3}(#{1,6})\s+(.*)$/;
const RULE = /^ {0,3}(?:[-*_]\s*){3,}$/;
const BULLET = /^ {0,3}[-*+]\s+(.*)$/;
const NUMBER = /^ {0,3}\d+[.)]\s+(.*)$/;
const QUOTE = /^ {0,3}>\s?(.*)$/;
const DIVIDER = /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)+\|?\s*$/;

function cells(line: string): string[] {
  return line.replace(/^\s*\|/, "").replace(/\|\s*$/, "").split("|").map((cell) => cell.trim());
}

function toBlocks(src: string): Block[] {
  const lines = src.replace(/\r\n/g, "\n").split("\n");
  const blocks: Block[] = [];
  let paragraph: string[] = [];

  const flush = () => {
    if (paragraph.length) blocks.push({ kind: "paragraph", text: paragraph.join("\n") });
    paragraph = [];
  };

  for (let i = 0; i < lines.length; i += 1) {
    const line = lines[i]!;
    const fence = FENCE.exec(line);

    if (fence) {
      flush();
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !FENCE.test(lines[i]!)) body.push(lines[i++]!);
      blocks.push({ kind: "code", lang: fence[1] ?? "", text: body.join("\n") });
      continue;
    }

    if (!line.trim()) {
      flush();
      continue;
    }

    if (RULE.test(line)) {
      flush();
      blocks.push({ kind: "rule" });
      continue;
    }

    const heading = HEADING.exec(line);
    if (heading) {
      flush();
      blocks.push({ kind: "heading", level: heading[1]!.length, text: heading[2]! });
      continue;
    }

    if (line.includes("|") && DIVIDER.test(lines[i + 1] ?? "")) {
      flush();
      const header = cells(line);
      const rows: string[][] = [];
      i += 2;
      while (i < lines.length && lines[i]!.includes("|") && lines[i]!.trim()) rows.push(cells(lines[i++]!));
      i -= 1;
      blocks.push({ kind: "table", header, rows });
      continue;
    }

    const quote = QUOTE.exec(line);
    if (quote) {
      flush();
      const body = [quote[1]!];
      while (i + 1 < lines.length && QUOTE.test(lines[i + 1]!)) body.push(QUOTE.exec(lines[++i]!)![1]!);
      blocks.push({ kind: "quote", text: body.join("\n") });
      continue;
    }

    const bullet = BULLET.exec(line);
    const numbered = NUMBER.exec(line);
    if (bullet || numbered) {
      flush();
      const ordered = !bullet;
      const pattern = ordered ? NUMBER : BULLET;
      const items = [(bullet ?? numbered)![1]!];
      while (i + 1 < lines.length && pattern.test(lines[i + 1]!)) items.push(pattern.exec(lines[++i]!)![1]!);
      blocks.push({ kind: "list", ordered, items });
      continue;
    }

    paragraph.push(line);
  }

  flush();
  return blocks;
}

const INLINE =
  /`([^`]+)`|\*\*([\s\S]+?)\*\*|__([\s\S]+?)__|~~([\s\S]+?)~~|\*([^*\n]+)\*|_([^_\n]+)_|\[([^\]]*)\]\(([^)\s]+)\)/g;

function inline(text: string, key = ""): React.ReactNode[] {
  /** Bold/italic/strike recurse, and a shared /g regex would have its lastIndex reset by the inner call. */
  const re = new RegExp(INLINE.source, INLINE.flags);
  const out: React.ReactNode[] = [];
  let last = 0;
  let match: RegExpExecArray | null;

  while ((match = re.exec(text))) {
    if (match.index > last) out.push(text.slice(last, match.index));
    const at = `${key}${match.index}`;

    if (match[1] !== undefined) {
      out.push(
        <Text key={at} style={styles.inlineCode}>
          {match[1]}
        </Text>,
      );
    } else if (match[2] !== undefined || match[3] !== undefined) {
      out.push(
        <Text key={at} style={styles.bold}>
          {inline(match[2] ?? match[3]!, `${at}.`)}
        </Text>,
      );
    } else if (match[4] !== undefined) {
      out.push(
        <Text key={at} style={styles.strike}>
          {inline(match[4], `${at}.`)}
        </Text>,
      );
    } else if (match[5] !== undefined || match[6] !== undefined) {
      out.push(
        <Text key={at} style={styles.italic}>
          {inline(match[5] ?? match[6]!, `${at}.`)}
        </Text>,
      );
    } else {
      const href = match[8]!;
      out.push(
        <Text key={at} style={styles.link} onPress={() => Linking.openURL(href)}>
          {match[7] || href}
        </Text>,
      );
    }

    last = match.index + match[0].length;
  }

  if (last < text.length) out.push(text.slice(last));
  return out;
}

function CodeBlock({ lang, text }: { lang: string; text: string }) {
  return (
    <View style={styles.code}>
      {lang ? <Text style={styles.lang}>{lang}</Text> : null}
      <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.codeBody}>
        <View>
          {text.split("\n").map((line, index) => (
            <React.Fragment key={index}>{highlight(line || " ", styles.codeLine)}</React.Fragment>
          ))}
        </View>
      </ScrollView>
    </View>
  );
}

function Table({ header, rows }: { header: string[]; rows: string[][] }) {
  return (
    <ScrollView horizontal showsHorizontalScrollIndicator={false}>
      <View style={styles.table}>
        <View style={styles.tableHead}>
          {header.map((cell, index) => (
            <Text key={index} style={[styles.cell, styles.headCell]}>
              {inline(cell)}
            </Text>
          ))}
        </View>
        {rows.map((row, index) => (
          <View key={index} style={styles.tableRow}>
            {header.map((_, column) => (
              <Text key={column} style={styles.cell}>
                {inline(row[column] ?? "")}
              </Text>
            ))}
          </View>
        ))}
      </View>
    </ScrollView>
  );
}

const HEADING_SIZE = [20, 18, 16, 15, 15, 15];

function Piece({ block }: { block: Block }) {
  switch (block.kind) {
    case "code":
      return <CodeBlock lang={block.lang} text={block.text} />;
    case "heading":
      return <Text style={[styles.heading, { fontSize: HEADING_SIZE[block.level - 1] }]}>{inline(block.text)}</Text>;
    case "rule":
      return <View style={styles.rule} />;
    case "quote":
      return (
        <View style={styles.quote}>
          <Text style={[styles.paragraph, { color: d.muted }]}>{inline(block.text)}</Text>
        </View>
      );
    case "list":
      return (
        <View style={styles.list}>
          {block.items.map((item, index) => (
            <View key={index} style={styles.listItem}>
              <Text style={styles.marker}>{block.ordered ? `${index + 1}.` : "•"}</Text>
              <Text style={[styles.paragraph, styles.listText]}>{inline(item)}</Text>
            </View>
          ))}
        </View>
      );
    case "table":
      return <Table header={block.header} rows={block.rows} />;
    case "paragraph":
      return <Text style={styles.paragraph}>{inline(block.text)}</Text>;
  }
}

export function Markdown({ text, dim = false }: { text: string; dim?: boolean }) {
  const blocks = useMemo(() => toBlocks(text), [text]);
  return (
    <View style={[styles.root, dim && styles.dim]}>
      {blocks.map((block, index) => (
        <Piece key={index} block={block} />
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  root: { gap: 10 },
  dim: { opacity: 0.72 },
  paragraph: { color: d.body, fontSize: 15, lineHeight: 23, fontFamily: font.regular },
  bold: { fontFamily: font.semibold, color: d.text },
  italic: { fontStyle: "italic" },
  strike: { textDecorationLine: "line-through", color: d.faint },
  link: { color: d.teal, textDecorationLine: "underline" },
  inlineCode: { fontFamily: font.mono, fontSize: 13.5, color: d.addText, backgroundColor: d.code },
  heading: { color: d.text, fontFamily: font.semibold, lineHeight: 26 },
  rule: { height: 1, backgroundColor: d.rule },
  quote: { borderLeftWidth: 2, borderLeftColor: d.stroke, paddingLeft: 10 },
  list: { gap: 4 },
  listItem: { flexDirection: "row", gap: 8 },
  marker: { color: d.faint, fontSize: 15, lineHeight: 23, fontFamily: font.mono, minWidth: 16 },
  listText: { flex: 1 },
  code: {
    backgroundColor: d.code,
    borderWidth: 1,
    borderColor: d.cardRule,
    borderRadius: 10,
    overflow: "hidden",
  },
  lang: {
    color: d.faint,
    fontSize: 11,
    fontFamily: font.mono,
    paddingHorizontal: 12,
    paddingTop: 8,
  },
  codeBody: { padding: 12 },
  codeLine: { color: d.body, fontSize: 12.5, lineHeight: 19, fontFamily: font.mono },
  table: { borderWidth: 1, borderColor: d.cardRule, borderRadius: 10, overflow: "hidden" },
  tableHead: { flexDirection: "row", backgroundColor: d.card },
  tableRow: { flexDirection: "row", borderTopWidth: 1, borderTopColor: d.cardRule },
  cell: {
    minWidth: 96,
    paddingHorizontal: 10,
    paddingVertical: 7,
    color: d.body,
    fontSize: 13,
    lineHeight: 19,
    fontFamily: font.regular,
  },
  headCell: { color: d.text, fontFamily: font.semibold },
});
