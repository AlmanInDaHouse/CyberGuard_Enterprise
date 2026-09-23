import { UnsupportedRuleError } from "./errors.js";

// SPEC-015 condition parser — a PURE module (no dependency on engine.ts) that
// tokenizes and parses a Sigma `condition` string over named detection blocks.
//
// Grammar (precedence not > and > or):
//
//   expr    := or
//   or      := and ("or" and)*
//   and     := not ("and" not)*
//   not     := "not" not | primary
//   primary := IDENT | "(" expr ")"
//
// IDENT is `^[A-Za-z_][A-Za-z0-9_]*$`, excluding the lowercase keywords `and` /
// `or` / `not` (operators) and the reserved words `of` / `all` / `them`.
// Anything outside this subset — a reserved word, a digit (`1 of`), an
// aggregation pipe `|`, unbalanced parentheses, trailing tokens, an empty
// condition, or any other character — is rejected with UnsupportedRuleError
// naming the construct (fail-closed; SPEC-015 §Scope).

/** Identifier shape, shared with the block-name validator in engine.ts. */
export const IDENTIFIER_RE = /^[A-Za-z_][A-Za-z0-9_]*$/;

/** Words that may never name a block: the operators plus the unsupported set-ops. */
export const RESERVED_WORDS: ReadonlySet<string> = new Set([
  "and",
  "or",
  "not",
  "of",
  "all",
  "them",
]);

/** Parsed condition AST over block-name identifiers. */
export type ConditionAst =
  | { type: "ident"; name: string }
  | { type: "not"; operand: ConditionAst }
  | { type: "and"; left: ConditionAst; right: ConditionAst }
  | { type: "or"; left: ConditionAst; right: ConditionAst };

type Token =
  | { kind: "ident"; value: string }
  | { kind: "op"; op: "and" | "or" | "not" }
  | { kind: "lparen" }
  | { kind: "rparen" };

function tokenize(src: string): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  while (i < src.length) {
    const c = src.charAt(i);
    if (c === " " || c === "\t" || c === "\n" || c === "\r") {
      i += 1;
      continue;
    }
    if (c === "(") {
      tokens.push({ kind: "lparen" });
      i += 1;
      continue;
    }
    if (c === ")") {
      tokens.push({ kind: "rparen" });
      i += 1;
      continue;
    }
    if (c === "|") {
      throw new UnsupportedRuleError('condition contains an aggregation pipe "|"');
    }
    if (/[A-Za-z_]/.test(c)) {
      let j = i + 1;
      while (j < src.length && /[A-Za-z0-9_]/.test(src.charAt(j))) {
        j += 1;
      }
      const word = src.slice(i, j);
      i = j;
      if (word === "and" || word === "or" || word === "not") {
        tokens.push({ kind: "op", op: word });
        continue;
      }
      if (RESERVED_WORDS.has(word)) {
        throw new UnsupportedRuleError(`condition uses reserved word "${word}"`);
      }
      tokens.push({ kind: "ident", value: word });
      continue;
    }
    throw new UnsupportedRuleError(`condition has an unexpected character "${c}"`);
  }
  return tokens;
}

function parseTokens(tokens: Token[]): ConditionAst {
  let pos = 0;
  const peek = (): Token | undefined => tokens[pos];
  const isOp = (t: Token | undefined, op: "and" | "or" | "not"): boolean =>
    t !== undefined && t.kind === "op" && t.op === op;

  function parseOr(): ConditionAst {
    let left = parseAnd();
    while (isOp(peek(), "or")) {
      pos += 1;
      const right = parseAnd();
      left = { type: "or", left, right };
    }
    return left;
  }
  function parseAnd(): ConditionAst {
    let left = parseNot();
    while (isOp(peek(), "and")) {
      pos += 1;
      const right = parseNot();
      left = { type: "and", left, right };
    }
    return left;
  }
  function parseNot(): ConditionAst {
    if (isOp(peek(), "not")) {
      pos += 1;
      return { type: "not", operand: parseNot() };
    }
    return parsePrimary();
  }
  function parsePrimary(): ConditionAst {
    const t = peek();
    if (t === undefined) {
      throw new UnsupportedRuleError('condition ended unexpectedly; expected a block name or "("');
    }
    if (t.kind === "ident") {
      pos += 1;
      return { type: "ident", name: t.value };
    }
    if (t.kind === "lparen") {
      pos += 1;
      const inner = parseOr();
      const close = peek();
      if (close === undefined || close.kind !== "rparen") {
        throw new UnsupportedRuleError("condition has unbalanced parentheses");
      }
      pos += 1;
      return inner;
    }
    throw new UnsupportedRuleError(
      'condition has an unexpected token; expected a block name or "("',
    );
  }

  const ast = parseOr();
  if (pos < tokens.length) {
    throw new UnsupportedRuleError("condition has unexpected trailing tokens");
  }
  return ast;
}

function collectIdentifiers(ast: ConditionAst, out: Set<string>): void {
  if (ast.type === "ident") {
    out.add(ast.name);
    return;
  }
  if (ast.type === "not") {
    collectIdentifiers(ast.operand, out);
    return;
  }
  collectIdentifiers(ast.left, out);
  collectIdentifiers(ast.right, out);
}

/** Parse a condition string into an AST plus the set of block names it references. */
export function parseCondition(src: string): { ast: ConditionAst; identifiers: Set<string> } {
  const tokens = tokenize(src);
  if (tokens.length === 0) {
    throw new UnsupportedRuleError("condition is empty");
  }
  const ast = parseTokens(tokens);
  const identifiers = new Set<string>();
  collectIdentifiers(ast, identifiers);
  return { ast, identifiers };
}

/** Evaluate a parsed condition against per-block boolean results. */
export function evaluateCondition(ast: ConditionAst, results: Map<string, boolean>): boolean {
  if (ast.type === "ident") {
    return results.get(ast.name) ?? false;
  }
  if (ast.type === "not") {
    return !evaluateCondition(ast.operand, results);
  }
  if (ast.type === "and") {
    return evaluateCondition(ast.left, results) && evaluateCondition(ast.right, results);
  }
  return evaluateCondition(ast.left, results) || evaluateCondition(ast.right, results);
}
