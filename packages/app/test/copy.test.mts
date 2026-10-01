import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import ts from "typescript";

const src = new URL("../src/", import.meta.url).pathname;
const context = new URL("../../../CONTEXT.md", import.meta.url).pathname;

/** Words CONTEXT.md lists as "_Avoid_" that are right in a given file. */
const allowed = [
  { file: "components/ToolGroup.tsx", term: "read", reason: "Claude's Read tool, not Seen" },
  { file: "tools.ts", term: "read", reason: "Claude's Read tool, not Seen" },
  { file: "components/Composer.tsx", term: "queue", reason: "the D37 composer label, not Up next" },
  { file: "composer.ts", term: "queue", reason: "the D37 composer label, not Up next" },
  { file: "launch.ts", term: "branch", reason: "a git branch, not a Worktree" },
  { file: "launch.ts", term: "folder", reason: "Claude's folder trust, not a Project" },
  { file: "screens/NewSessionScreen.tsx", term: "branch", reason: "the new worktree's git branch" },
];

function files(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return files(path);
    return /\.tsx?$/.test(entry.name) ? [path] : [];
  });
}

/** Text a user can read: JSX text and attributes, and literals that look like prose. */
function strings(path: string): string[] {
  const source = ts.createSourceFile(path, readFileSync(path, "utf8"), ts.ScriptTarget.Latest, true);
  const found: string[] = [];
  const prose = (text: string) => /\s/.test(text.trim()) || /^[A-Z]/.test(text);
  const visit = (node: ts.Node): void => {
    if (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) return;
    if (ts.isJsxText(node)) {
      if (node.text.trim()) found.push(node.text.trim());
    } else if (ts.isJsxAttribute(node) && node.initializer && ts.isStringLiteral(node.initializer)) {
      found.push(node.initializer.text);
      return;
    } else if (ts.isStringLiteral(node) || ts.isNoSubstitutionTemplateLiteral(node)) {
      if (prose(node.text)) found.push(node.text);
    } else if (ts.isTemplateExpression(node)) {
      const text = [node.head.text, ...node.templateSpans.map((span) => span.literal.text)].join("…");
      if (prose(text)) found.push(text);
    }
    ts.forEachChild(node, visit);
  };
  visit(source);
  return found;
}

function avoided(): string[] {
  return readFileSync(context, "utf8")
    .split("\n")
    .filter((line) => line.startsWith("_Avoid_:"))
    .flatMap((line) => line.slice("_Avoid_:".length).split(","))
    .map((term) => term.trim().toLowerCase())
    .filter((term, at, all) => term && all.indexOf(term) === at);
}

function uses(text: string, term: string): boolean {
  const escaped = term.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(?<![\\w$])${escaped}(?!\\w)`, "i").test(text);
}

const copy = files(src).map((path) => ({ file: relative(src, path), texts: strings(path) }));

test("phone copy uses no CONTEXT.md avoided word", () => {
  const terms = avoided();
  assert.ok(terms.includes("waiting") && terms.includes("running"), "CONTEXT.md avoid lines were not found");
  const hits = copy.flatMap(({ file, texts }) =>
    texts.flatMap((text) =>
      terms
        .filter((term) => uses(text, term))
        .filter((term) => !allowed.some((a) => a.file === file && a.term === term))
        .map((term) => `${file}: "${text}" uses "${term}"`),
    ),
  );
  assert.deepEqual(hits, []);
});

const all = copy.flatMap(({ texts }) => texts);

test("the app offers no raw terminal, attach or dictate control", () => {
  for (const label of ["Open raw terminal", "Attach", "Dictate"]) {
    assert.ok(!all.includes(label), `"${label}" is still in the app`);
  }
});

test("changes can be reviewed", () => {
  assert.ok(all.includes("Review changes"));
});

test("the phone labels agents as sessions", () => {
  for (const label of ["Sessions", "Back to sessions", "No sessions yet", "Start one on your Mac."]) {
    assert.ok(all.includes(label), `"${label}" is missing`);
  }
  for (const label of ["Agents", "Back to agents"]) {
    assert.ok(!all.includes(label), `"${label}" is still in the app`);
  }
});
