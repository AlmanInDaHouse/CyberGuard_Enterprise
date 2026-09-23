import { describe, expect, test } from "vitest";
import { evaluateCondition, parseCondition } from "../src/detect/condition.js";
import { UnsupportedRuleError } from "../src/detect/errors.js";

// SPEC-015 C1 — the pure condition parser: precedence (not > and > or),
// parentheses, nested not, identifier extraction, evaluation, and the
// fail-closed rejection of every out-of-subset construct.

/** Parse `src`, then evaluate with the identifiers in `truthy` set true, rest false. */
function evalWith(src: string, truthy: string[]): boolean {
  const { ast, identifiers } = parseCondition(src);
  const results = new Map<string, boolean>();
  for (const id of identifiers) {
    results.set(id, truthy.includes(id));
  }
  return evaluateCondition(ast, results);
}

describe("precedence (not > and > or)", () => {
  test("`a or b and c` groups as a OR (b AND c)", () => {
    // b AND c is false unless both true; a alone can carry the OR.
    expect(evalWith("a or b and c", ["b"])).toBe(false);
    expect(evalWith("a or b and c", ["b", "c"])).toBe(true);
    expect(evalWith("a or b and c", ["a"])).toBe(true);
  });

  test("`a and b or c` groups as (a AND b) OR c", () => {
    expect(evalWith("a and b or c", ["a"])).toBe(false);
    expect(evalWith("a and b or c", ["c"])).toBe(true);
    expect(evalWith("a and b or c", ["a", "b"])).toBe(true);
  });

  test("`not` binds tighter than `and`: `not a and b` is (not a) AND b", () => {
    expect(evalWith("not a and b", ["b"])).toBe(true); // (not false) and true
    expect(evalWith("not a and b", ["a", "b"])).toBe(false); // (not true) and true
  });
});

describe("parentheses and nested not", () => {
  test("parentheses override precedence: `(a or b) and c`", () => {
    expect(evalWith("(a or b) and c", ["a"])).toBe(false);
    expect(evalWith("(a or b) and c", ["a", "c"])).toBe(true);
  });

  test("nested not: `not not a` is a", () => {
    expect(evalWith("not not a", ["a"])).toBe(true);
    expect(evalWith("not not a", [])).toBe(false);
  });

  test("`not (a and b)` is !(a AND b)", () => {
    expect(evalWith("not (a and b)", ["a"])).toBe(true);
    expect(evalWith("not (a and b)", ["a", "b"])).toBe(false);
  });

  test("`selection and not filter` fires unless filter is true", () => {
    expect(evalWith("selection and not filter", ["selection"])).toBe(true);
    expect(evalWith("selection and not filter", ["selection", "filter"])).toBe(false);
    expect(evalWith("selection and not filter", ["filter"])).toBe(false);
  });
});

describe("identifier extraction", () => {
  test("collects every referenced block name once", () => {
    const { identifiers } = parseCondition("a and (b or not c)");
    expect([...identifiers].sort()).toEqual(["a", "b", "c"]);
  });

  test("a repeated identifier is collected once", () => {
    const { identifiers } = parseCondition("a or (a and b)");
    expect([...identifiers].sort()).toEqual(["a", "b"]);
  });
});

describe("evaluation against a results map", () => {
  test("a single identifier reads its result", () => {
    const { ast } = parseCondition("selection");
    expect(ast.type).toBe("ident");
    expect(evaluateCondition(ast, new Map([["selection", true]]))).toBe(true);
    expect(evaluateCondition(ast, new Map([["selection", false]]))).toBe(false);
  });
});

describe("fail-closed: rejection of out-of-subset constructs", () => {
  const rejects = (src: string) => expect(() => parseCondition(src)).toThrow(UnsupportedRuleError);

  test("reserved words of / all / them", () => {
    rejects("all of them");
    rejects("selection of x");
    rejects("them");
    expect(() => parseCondition("a or them")).toThrow(/reserved word/);
  });

  test("digits (`1 of`)", () => {
    rejects("1 of them");
    expect(() => parseCondition("1 of them")).toThrow(/unexpected character/);
  });

  test("aggregation pipe `|`", () => {
    rejects("selection | count() > 5");
    expect(() => parseCondition("a | b")).toThrow(/aggregation pipe/);
  });

  test("unbalanced parentheses", () => {
    rejects("(a and b");
    rejects("((a)");
    expect(() => parseCondition("(a and b")).toThrow(/unbalanced/);
  });

  test("trailing tokens", () => {
    rejects("a b");
    rejects("a and b)");
    expect(() => parseCondition("a b")).toThrow(/trailing/);
  });

  test("empty condition", () => {
    rejects("");
    rejects("   ");
    expect(() => parseCondition("")).toThrow(/empty/);
  });

  test("any other character", () => {
    rejects("a & b");
    rejects("a - b");
    expect(() => parseCondition("a & b")).toThrow(/unexpected character/);
  });

  test("a dangling operator with no operand", () => {
    rejects("a and");
    rejects("not");
    rejects("or a");
  });
});
