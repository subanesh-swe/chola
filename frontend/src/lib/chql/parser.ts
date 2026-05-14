import { CstParser } from 'chevrotain';
import {
  ALL_TOKENS,
  And,
  Bang,
  BangEq,
  Colon,
  Comma,
  Gt,
  Gte,
  Identifier,
  LBracket,
  LParen,
  Lt,
  Lte,
  Not,
  NumberLiteral,
  Or,
  RBracket,
  RParen,
  StringLiteral,
} from './lexer';

class ChqlCstParser extends CstParser {
  constructor() {
    super(ALL_TOKENS, { recoveryEnabled: true });
    this.performSelfAnalysis();
  }

  // query := or_expr
  query = this.RULE('query', () => {
    this.SUBRULE(this.or_expr);
  });

  // or_expr := and_expr ( OR and_expr )*
  or_expr = this.RULE('or_expr', () => {
    this.SUBRULE(this.and_expr, { LABEL: 'head' });
    this.MANY(() => {
      this.CONSUME(Or, { LABEL: 'op' });
      this.SUBRULE2(this.and_expr, { LABEL: 'rest' });
    });
  });

  // and_expr := not_expr ( ( AND | implicit_ws ) not_expr )*
  and_expr = this.RULE('and_expr', () => {
    this.SUBRULE(this.not_expr, { LABEL: 'head' });
    this.MANY(() => {
      this.OPTION(() => {
        this.CONSUME(And, { LABEL: 'op' });
      });
      this.SUBRULE2(this.not_expr, { LABEL: 'rest' });
    });
  });

  // not_expr := NOT atom | atom
  not_expr = this.RULE('not_expr', () => {
    this.OR([
      {
        ALT: () => {
          this.CONSUME(Not, { LABEL: 'notKw' });
          this.SUBRULE(this.atom, { LABEL: 'expr' });
        },
      },
      {
        ALT: () => {
          this.SUBRULE2(this.atom, { LABEL: 'expr' });
        },
      },
    ]);
  });

  // atom := '(' or_expr ')' | field_term
  atom = this.RULE('atom', () => {
    this.OR([
      {
        ALT: () => {
          this.CONSUME(LParen);
          this.SUBRULE(this.or_expr, { LABEL: 'inner' });
          this.CONSUME(RParen);
        },
      },
      {
        ALT: () => {
          this.SUBRULE(this.field_term);
        },
      },
    ]);
  });

  // field_term := Identifier ':' value_expr
  field_term = this.RULE('field_term', () => {
    this.CONSUME(Identifier, { LABEL: 'field' });
    this.CONSUME(Colon);
    this.SUBRULE(this.value_expr);
  });

  // value_expr — unified, ordered alternatives.
  // Key insight: call_expr starts with Identifier LParen.
  // bare_or_quoted starts with Identifier NOT followed by LParen.
  // We use GATE predicates to disambiguate.
  value_expr = this.RULE('value_expr', () => {
    this.OR([
      // Range: '[' ...
      { ALT: () => this.SUBRULE(this.range_expr) },
      // Numcmp: '>=' | '<=' | '>' | '<'
      { ALT: () => this.SUBRULE(this.numcmp_expr) },
      // Not-equal: '!=' | '!'
      { ALT: () => this.SUBRULE(this.not_eq_expr) },
      // Function call: Identifier '(' StringLiteral ')' — uses GATE to require LParen as 2nd token
      {
        GATE: () => this.LA(2).tokenType === LParen,
        ALT: () => this.SUBRULE(this.call_expr),
      },
      // Bare or quoted: StringLiteral | Identifier (not followed by '(') | NumberLiteral
      { ALT: () => this.SUBRULE(this.bare_or_quoted_expr) },
    ]);
  });

  // range_expr := '[' range_endpoint ',' range_endpoint ']'
  range_expr = this.RULE('range_expr', () => {
    this.CONSUME(LBracket);
    this.SUBRULE(this.range_endpoint, { LABEL: 'lo' });
    this.CONSUME(Comma);
    this.SUBRULE2(this.range_endpoint, { LABEL: 'hi' });
    this.CONSUME(RBracket);
  });

  // range_endpoint := NumberLiteral | StringLiteral
  range_endpoint = this.RULE('range_endpoint', () => {
    this.OR([
      { ALT: () => this.CONSUME(NumberLiteral) },
      { ALT: () => this.CONSUME(StringLiteral) },
    ]);
  });

  // numcmp_expr := ( '>=' | '<=' | '>' | '<' ) ( NumberLiteral | StringLiteral )
  numcmp_expr = this.RULE('numcmp_expr', () => {
    this.OR([
      { ALT: () => this.CONSUME(Gte, { LABEL: 'op' }) },
      { ALT: () => this.CONSUME(Lte, { LABEL: 'op' }) },
      { ALT: () => this.CONSUME(Gt, { LABEL: 'op' }) },
      { ALT: () => this.CONSUME(Lt, { LABEL: 'op' }) },
    ]);
    this.OR2([
      { ALT: () => this.CONSUME(NumberLiteral, { LABEL: 'val' }) },
      { ALT: () => this.CONSUME(StringLiteral, { LABEL: 'val' }) },
    ]);
  });

  // not_eq_expr := ( '!=' | '!' ) ( StringLiteral | NumberLiteral | Identifier )
  not_eq_expr = this.RULE('not_eq_expr', () => {
    this.OR([
      { ALT: () => this.CONSUME(BangEq, { LABEL: 'op' }) },
      { ALT: () => this.CONSUME(Bang, { LABEL: 'op' }) },
    ]);
    this.OR2([
      { ALT: () => this.CONSUME(StringLiteral, { LABEL: 'val' }) },
      { ALT: () => this.CONSUME(NumberLiteral, { LABEL: 'val' }) },
      { ALT: () => this.CONSUME(Identifier, { LABEL: 'val' }) },
    ]);
  });

  // call_expr := Identifier '(' StringLiteral ')' — covers both i() and re()
  call_expr = this.RULE('call_expr', () => {
    this.CONSUME(Identifier, { LABEL: 'fn' });
    this.CONSUME(LParen);
    this.CONSUME(StringLiteral, { LABEL: 'arg' });
    this.CONSUME(RParen);
  });

  // bare_or_quoted_expr := StringLiteral | NumberLiteral | Identifier
  bare_or_quoted_expr = this.RULE('bare_or_quoted_expr', () => {
    this.OR([
      { ALT: () => this.CONSUME(StringLiteral) },
      { ALT: () => this.CONSUME(NumberLiteral) },
      { ALT: () => this.CONSUME(Identifier) },
    ]);
  });
}

// Singleton — Chevrotain parsers are expensive to construct.
let _parser: ChqlCstParser | null = null;
export function getParser(): ChqlCstParser {
  if (!_parser) _parser = new ChqlCstParser();
  return _parser;
}

export { ChqlCstParser };
