// Vendors Material Icon Theme's icons, with its light-theme overrides applied, into the theme crate.
import { execSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { basename, dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const VERSION = "5.38.1";
const out = join(dirname(fileURLToPath(import.meta.url)), "../packages/desktop/crates/theme/assets/material");
const tmp = mkdtempSync(join(tmpdir(), "material-icons-"));
execSync(`npm pack material-icon-theme@${VERSION} --silent && tar xzf material-icon-theme-${VERSION}.tgz`, { cwd: tmp });
const pkg = join(tmp, "package");
const theme = JSON.parse(readFileSync(join(pkg, "dist/material-icons.json"), "utf8"));
const file = (name) => basename(theme.iconDefinitions[name].iconPath, ".svg");
const section = (key) => Object.fromEntries(Object.entries({ ...theme[key], ...theme.light[key] }).map(([k, v]) => [k, file(v)]));
const manifest = {
  names: section("fileNames"),
  extensions: section("fileExtensions"),
  folders: section("folderNames"),
  foldersOpen: section("folderNamesExpanded"),
};
const used = new Set(["file", "folder", "folder-open", ...Object.values(manifest).flatMap(Object.values)]);
rmSync(out, { recursive: true, force: true });
mkdirSync(out, { recursive: true });
for (const name of used) copyFileSync(join(pkg, "icons", `${name}.svg`), join(out, `${name}.svg`));
copyFileSync(join(pkg, "LICENSE"), join(out, "LICENSE"));
writeFileSync(join(out, "icons.json"), JSON.stringify(manifest));
rmSync(tmp, { recursive: true });
console.log(`${used.size} icons from material-icon-theme ${VERSION}`);
