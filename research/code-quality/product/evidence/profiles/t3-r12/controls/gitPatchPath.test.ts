import { expect, it } from "vite-plus/test";
import { quoteGitPatchPath, unquoteGitPatchPath } from "../packages/shared/src/gitPatchPath.ts";

it("preserves single-character literal segments around path escapes", () => {
  expect(unquoteGitPatchPath("a\\tb")).toBe("a\tb");
  expect(unquoteGitPatchPath('"a\\tb"')).toBe("a\tb");
  expect(unquoteGitPatchPath("a\\n")).toBe("a\n");
  expect(unquoteGitPatchPath(quoteGitPatchPath("a\tb"))).toBe("a\tb");
  expect(unquoteGitPatchPath("ab\\tcd")).toBe("ab\tcd");
  expect(unquoteGitPatchPath("a")).toBe("a");
  expect(unquoteGitPatchPath("plain.ts")).toBe("plain.ts");
});
