import { expect, it } from "vite-plus/test";
import { isExplicitRelativePath, normalizeProjectPathForDispatch } from "../packages/shared/src/path.ts";

it("recognizes relative parent and Windows current-directory prefixes", () => {
  expect(isExplicitRelativePath("../repo")).toBe(true);
  expect(isExplicitRelativePath(".\\repo")).toBe(true);
  expect(isExplicitRelativePath("./repo")).toBe(true);
  expect(isExplicitRelativePath("..\\repo")).toBe(true);
  expect(isExplicitRelativePath("~/repo")).toBe(false);
});

it("trims a trailing separator after a single-character project name", () => {
  expect(normalizeProjectPathForDispatch("x/")).toBe("x");
  expect(normalizeProjectPathForDispatch("x")).toBe("x");
  expect(normalizeProjectPathForDispatch("/repo/")).toBe("/repo");
  expect(normalizeProjectPathForDispatch("/")).toBe("/");
});
