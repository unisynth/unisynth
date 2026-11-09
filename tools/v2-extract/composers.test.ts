import { expect, test } from "bun:test";
import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

// composers.json: composer credits found in public demoscene databases for V2 songs whose source names nobody (or only
// the group). make-demos.py applies them; these tests keep the table and the committed songs in step.
const DIR = fileURLToPath(new URL("../../project/music/released/v2/", import.meta.url));
const table: { key: string; slug: string; author: string; credit: string }[] = JSON.parse(readFileSync(new URL("./composers.json", import.meta.url), "utf8"));
const meta = (slug: string) => JSON.parse(readFileSync(DIR + slug + ".song.json", "utf8")).meta;

test("every credited song carries its composer, the credit sentence and the copyright", () => {
  expect(table.length).toBeGreaterThan(0);
  for (const c of table) {
    const m = meta(c.slug);
    expect(m.author, c.slug).toBe(c.author);
    expect(m.description, c.slug).toContain(c.credit);
    expect(m.description, c.slug).not.toContain("The source does not name the author.");
    expect(m.copyright, c.slug).toBe("© " + c.author);
  }
});

test("a credit always cites where it was found", () => {
  for (const c of table) expect(c.credit, c.slug).toMatch(/^Music credited to .+ (on|in|by) .+\.$/);
});

test("songs without a credit say so", () => {
  const credited = new Set(table.map((c) => c.slug));
  for (const f of readdirSync(DIR)) {
    const m = JSON.parse(readFileSync(DIR + f, "utf8")).meta;
    if (m.author === "not named") {
      expect(credited.has(f.replace(".song.json", "")), f).toBe(false);
      expect(m.description, f).toContain("The source does not name the author.");
    }
  }
});
