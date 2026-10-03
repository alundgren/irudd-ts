import { expect, it } from "vite-plus/test";
import { parseDelimitedPreview } from "../packages/shared/src/delimitedPreview.ts";

it("reports column truncation independently of row and cell limits", () => {
  const complete = parseDelimitedPreview(Array(30).fill("x").join(","), ",");
  expect(complete.rows).toEqual([Array(30).fill("x")]);
  expect(complete.truncated).toBe(false);
  const partial = parseDelimitedPreview(Array(31).fill("x").join(","), ",");
  expect(partial.rows).toEqual([Array(30).fill("x")]);
  expect(partial.truncated).toBe(true);
});

it("reports cell truncation for ordinary text and escaped quotes at the boundary", () => {
  expect(parseDelimitedPreview("x".repeat(2000), ",")).toEqual({ rows: [["x".repeat(2000)]], truncated: false });
  expect(parseDelimitedPreview("x".repeat(2001), ",")).toEqual({ rows: [["x".repeat(2000)]], truncated: true });
  expect(parseDelimitedPreview('"' + "x".repeat(1999) + '"""', ",")).toEqual({ rows: [["x".repeat(1999) + '"']], truncated: false });
  expect(parseDelimitedPreview('"' + "x".repeat(2000) + '"""', ",")).toEqual({ rows: [["x".repeat(2000)]], truncated: true });
});

it("preserves bare carriage returns and empty records between line endings", () => {
  for (const delimiter of [",", "\t"] as const) {
    expect(parseDelimitedPreview("first\rsecond\r", delimiter)).toEqual({ rows: [["first"], ["second"]], truncated: false });
    expect(parseDelimitedPreview("first\n\nsecond", delimiter)).toEqual({ rows: [["first"], [""], ["second"]], truncated: false });
    expect(parseDelimitedPreview("first\r\nsecond\r\n", delimiter)).toEqual({ rows: [["first"], ["second"]], truncated: false });
  }
});

it("keeps an exact row limit complete with and without a final line ending", () => {
  const rows = Array.from({ length: 100 }, (_, index) => [String(index)]);
  for (const ending of ["", "\n", "\r", "\r\n"]) {
    const complete = parseDelimitedPreview(rows.map(row => row[0]).join("\n") + ending, ",");
    expect(complete).toEqual({ rows, truncated: false });
  }
  const partial = parseDelimitedPreview(rows.map(row => row[0]).join("\n") + "\nextra", ",");
  expect(partial).toEqual({ rows, truncated: true });
  expect(parseDelimitedPreview("ordinary\n", ",")).toEqual({ rows: [["ordinary"]], truncated: false });
});
