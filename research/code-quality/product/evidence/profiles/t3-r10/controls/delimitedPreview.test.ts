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
