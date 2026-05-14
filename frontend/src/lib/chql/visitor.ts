import type { CstNode, IToken } from 'chevrotain';
import { ChqlCstParser } from './parser';
import type { Ast, CmpOp, Value } from './ast';
import { VALID_FIELDS, isDateString } from './ast';

// ── Visitor base ─────────────────────────────────────────────────────────────

const BaseVisitor = new ChqlCstParser().getBaseCstVisitorConstructor();

export interface VisitorError {
  message: string;
  position?: { start: number; end: number };
}

class ChqlAstVisitor extends BaseVisitor {
  errors: VisitorError[] = [];

  constructor() {
    super();
    this.validateVisitor();
  }

  query(ctx: Record<string, CstNode[]>): Ast | null {
    if (!ctx.or_expr?.length) return null;
    return this.visit(ctx.or_expr[0]) as Ast | null;
  }

  or_expr(ctx: Record<string, CstNode[] | IToken[]>): Ast | null {
    const heads = ctx.head as CstNode[];
    const rests = (ctx.rest as CstNode[]) ?? [];

    let left = this.visit(heads[0]) as Ast | null;
    for (const rest of rests) {
      const right = this.visit(rest) as Ast | null;
      if (left && right) {
        left = { type: 'or', left, right };
      } else if (right) {
        left = right;
      }
    }
    return left;
  }

  and_expr(ctx: Record<string, CstNode[] | IToken[]>): Ast | null {
    const heads = ctx.head as CstNode[];
    const rests = (ctx.rest as CstNode[]) ?? [];

    let left = this.visit(heads[0]) as Ast | null;
    for (const rest of rests) {
      const right = this.visit(rest) as Ast | null;
      if (left && right) {
        left = { type: 'and', left, right };
      } else if (right) {
        left = right;
      }
    }
    return left;
  }

  not_expr(ctx: Record<string, CstNode[] | IToken[]>): Ast | null {
    const isNot = !!(ctx.notKw as IToken[] | undefined)?.length;
    const exprs = ctx.expr as CstNode[];
    const inner = this.visit(exprs[0]) as Ast | null;
    if (isNot && inner) return { type: 'not', expr: inner };
    return inner;
  }

  atom(ctx: Record<string, CstNode[]>): Ast | null {
    if (ctx.inner?.length) return this.visit(ctx.inner[0]) as Ast | null;
    if (ctx.field_term?.length) return this.visit(ctx.field_term[0]) as Ast | null;
    return null;
  }

  field_term(ctx: Record<string, CstNode[] | IToken[]>): Ast | null {
    const fieldToks = ctx.field as IToken[];
    if (!fieldToks?.length) return null;

    const fieldName = fieldToks[0].image;
    const tok = fieldToks[0];

    if (!(VALID_FIELDS as readonly string[]).includes(fieldName)) {
      this.errors.push({
        message: `Unknown field '${fieldName}'. Valid fields: ${VALID_FIELDS.join(', ')}`,
        position: { start: tok.startOffset, end: tok.endOffset ?? tok.startOffset + fieldName.length },
      });
      return null;
    }

    const valueNodes = ctx.value_expr as CstNode[];
    if (!valueNodes?.length) return null;

    const valueResult = this.visit(valueNodes[0]) as ValueVisitResult | null;
    if (!valueResult) return null;

    if (valueResult.kind === 'range') {
      return { type: 'in_range', field: fieldName, lo: valueResult.lo, hi: valueResult.hi };
    }

    return { type: 'cmp', field: fieldName, op: valueResult.op, value: valueResult.value };
  }

  value_expr(ctx: Record<string, CstNode[]>): ValueVisitResult | null {
    for (const key of ['range_expr', 'numcmp_expr', 'not_eq_expr', 'call_expr', 'bare_or_quoted_expr']) {
      if (ctx[key]?.length) return this.visit(ctx[key][0]) as ValueVisitResult | null;
    }
    return null;
  }

  range_expr(ctx: Record<string, CstNode[]>): RangeResult | null {
    const los = ctx.lo as CstNode[];
    const his = ctx.hi as CstNode[];
    if (!los?.length || !his?.length) return null;
    const lo = this.visit(los[0]) as Value | null;
    const hi = this.visit(his[0]) as Value | null;
    if (!lo || !hi) return null;
    return { kind: 'range', lo, hi };
  }

  range_endpoint(ctx: Record<string, IToken[]>): Value | null {
    if (ctx.NumberLiteral?.length) {
      return { kind: 'num', v: parseFloat(ctx.NumberLiteral[0].image) };
    }
    if (ctx.StringLiteral?.length) {
      const raw = unquote(ctx.StringLiteral[0].image);
      return isDateString(raw) ? { kind: 'date', v: raw } : { kind: 'str', v: raw };
    }
    return null;
  }

  numcmp_expr(ctx: Record<string, IToken[]>): CmpResult | null {
    const ops = ctx.op as IToken[];
    const vals = ctx.val as IToken[];
    if (!ops?.length || !vals?.length) return null;

    const opMap: Record<string, CmpOp> = { '>=': 'gte', '<=': 'lte', '>': 'gt', '<': 'lt' };
    const op = opMap[ops[0].image];
    if (!op) return null;

    return { kind: 'cmp', op, value: tokenToValue(vals[0]) ?? { kind: 'str', v: vals[0].image } };
  }

  not_eq_expr(ctx: Record<string, IToken[]>): CmpResult | null {
    const vals = ctx.val as IToken[];
    if (!vals?.length) return null;
    const value = tokenToValue(vals[0]);
    if (!value) return null;
    return { kind: 'cmp', op: 'neq', value };
  }

  // Handles both i(...) and re(...) — inspect fn name to determine op.
  call_expr(ctx: Record<string, IToken[]>): CmpResult | null {
    const fns = ctx.fn as IToken[];
    const args = ctx.arg as IToken[];
    if (!fns?.length || !args?.length) return null;

    const fnName = fns[0].image.toLowerCase();
    const raw = unquote(args[0].image);

    if (fnName === 'i') {
      return { kind: 'cmp', op: 'ilike_contains', value: { kind: 'str', v: raw } };
    }
    if (fnName === 're') {
      return { kind: 'cmp', op: 'regex', value: { kind: 'str', v: raw } };
    }
    // Unknown function — treat as ilike_contains (best guess).
    return { kind: 'cmp', op: 'ilike_contains', value: { kind: 'str', v: raw } };
  }

  bare_or_quoted_expr(ctx: Record<string, IToken[]>): CmpResult | null {
    if (ctx.NumberLiteral?.length) {
      return { kind: 'cmp', op: 'eq', value: { kind: 'num', v: parseFloat(ctx.NumberLiteral[0].image) } };
    }
    if (ctx.StringLiteral?.length) {
      const raw = unquote(ctx.StringLiteral[0].image);
      // Wildcard
      if (raw.includes('*')) return { kind: 'cmp', op: 'wildcard', value: { kind: 'str', v: raw } };
      // Date
      if (isDateString(raw)) return { kind: 'cmp', op: 'eq', value: { kind: 'date', v: raw } };
      return { kind: 'cmp', op: 'eq', value: { kind: 'str', v: raw } };
    }
    if (ctx.Identifier?.length) {
      const raw = ctx.Identifier[0].image;
      // Wildcard in bare identifier
      if (raw.includes('*')) return { kind: 'cmp', op: 'wildcard', value: { kind: 'str', v: raw } };
      return { kind: 'cmp', op: 'eq', value: { kind: 'str', v: raw } };
    }
    return null;
  }
}

// ── Helper types ─────────────────────────────────────────────────────────────

interface CmpResult { kind: 'cmp'; op: CmpOp; value: Value; }
interface RangeResult { kind: 'range'; lo: Value; hi: Value; }
type ValueVisitResult = CmpResult | RangeResult;

// ── Utility ──────────────────────────────────────────────────────────────────

function unquote(s: string): string {
  if (s.startsWith('"') && s.endsWith('"')) {
    return s.slice(1, -1).replace(/\\(.)/g, '$1');
  }
  return s;
}

function tokenToValue(tok: IToken): Value | null {
  if (!tok) return null;
  if (tok.tokenType.name === 'NumberLiteral') {
    return { kind: 'num', v: parseFloat(tok.image) };
  }
  if (tok.tokenType.name === 'StringLiteral') {
    const raw = unquote(tok.image);
    return isDateString(raw) ? { kind: 'date', v: raw } : { kind: 'str', v: raw };
  }
  // Identifier (bare value)
  return { kind: 'str', v: tok.image };
}

// ── Singleton ────────────────────────────────────────────────────────────────

let _visitor: ChqlAstVisitor | null = null;
export function getVisitor(): ChqlAstVisitor {
  if (!_visitor) _visitor = new ChqlAstVisitor();
  return _visitor;
}

export { ChqlAstVisitor };
