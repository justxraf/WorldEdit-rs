#[cfg(test)]
use std::rc::Rc;

use pumpkin_plugin_api::common::BlockPos;

use crate::pattern::PatternEvalContext;

#[derive(Clone, Debug)]
pub struct CompiledExpression {
    root: Expr,
    uses_world_queries: bool,
}

impl CompiledExpression {
    pub fn compile(input: &str) -> Result<Self, String> {
        let mut parser = Parser::new(input)?;
        let root = parser.parse_expression()?;
        parser.expect_end()?;
        Ok(Self {
            uses_world_queries: root.uses_world_queries(),
            root,
        })
    }

    pub fn uses_world_queries(&self) -> bool {
        self.uses_world_queries
    }

    pub fn evaluate(
        &self,
        pos: BlockPos,
        before: u16,
        ctx: &PatternEvalContext,
    ) -> Result<f64, String> {
        let mut eval = EvalContext {
            pos,
            before,
            ctx,
            random_counter: 0,
        };
        self.root.eval(&mut eval)
    }
}

#[derive(Clone, Debug)]
enum Expr {
    Number(f64),
    Variable(Variable),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Ternary {
        condition: Box<Expr>,
        on_true: Box<Expr>,
        on_false: Box<Expr>,
    },
    Call {
        name: String,
        args: Vec<Expr>,
    },
}

impl Expr {
    fn uses_world_queries(&self) -> bool {
        match self {
            Self::Number(_) | Self::Variable(_) => false,
            Self::Unary { expr, .. } => expr.uses_world_queries(),
            Self::Binary { left, right, .. } => {
                left.uses_world_queries() || right.uses_world_queries()
            }
            Self::Ternary {
                condition,
                on_true,
                on_false,
            } => {
                condition.uses_world_queries()
                    || on_true.uses_world_queries()
                    || on_false.uses_world_queries()
            }
            Self::Call { name, args } => {
                matches!(name.as_str(), "query" | "queryabs" | "queryrel")
                    || args.iter().any(Expr::uses_world_queries)
            }
        }
    }

    fn eval(&self, ctx: &mut EvalContext<'_>) -> Result<f64, String> {
        match self {
            Self::Number(value) => Ok(*value),
            Self::Variable(variable) => Ok(variable.read(ctx)),
            Self::Unary { op, expr } => {
                let value = expr.eval(ctx)?;
                Ok(match op {
                    UnaryOp::Plus => value,
                    UnaryOp::Minus => -value,
                    UnaryOp::Not => truthy(value).not_as_f64(),
                })
            }
            Self::Binary { op, left, right } => match op {
                BinaryOp::LogicalOr => {
                    let left = left.eval(ctx)?;
                    if truthy(left).0 {
                        Ok(1.0)
                    } else {
                        Ok(truthy(right.eval(ctx)?).as_f64())
                    }
                }
                BinaryOp::LogicalAnd => {
                    let left = left.eval(ctx)?;
                    if !truthy(left).0 {
                        Ok(0.0)
                    } else {
                        Ok(truthy(right.eval(ctx)?).as_f64())
                    }
                }
                _ => {
                    let left = left.eval(ctx)?;
                    let right = right.eval(ctx)?;
                    Ok(match op {
                        BinaryOp::Add => left + right,
                        BinaryOp::Subtract => left - right,
                        BinaryOp::Multiply => left * right,
                        BinaryOp::Divide => left / right,
                        BinaryOp::Remainder => left % right,
                        BinaryOp::Power => left.powf(right),
                        BinaryOp::Equal => bool_to_f64(left == right),
                        BinaryOp::NotEqual => bool_to_f64(left != right),
                        BinaryOp::AlmostEqual => bool_to_f64(almost_equal(left, right)),
                        BinaryOp::Less => bool_to_f64(left < right),
                        BinaryOp::LessOrEqual => bool_to_f64(left <= right),
                        BinaryOp::Greater => bool_to_f64(left > right),
                        BinaryOp::GreaterOrEqual => bool_to_f64(left >= right),
                        BinaryOp::LogicalOr | BinaryOp::LogicalAnd => unreachable!(),
                    })
                }
            },
            Self::Ternary {
                condition,
                on_true,
                on_false,
            } => {
                if truthy(condition.eval(ctx)?).0 {
                    on_true.eval(ctx)
                } else {
                    on_false.eval(ctx)
                }
            }
            Self::Call { name, args } => eval_call(name, args, ctx),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum Variable {
    X,
    Y,
    Z,
}

impl Variable {
    fn parse(input: &str) -> Option<Self> {
        match input {
            "x" => Some(Self::X),
            "y" => Some(Self::Y),
            "z" => Some(Self::Z),
            _ => None,
        }
    }

    fn read(self, ctx: &EvalContext<'_>) -> f64 {
        match self {
            Self::X => ctx.pos.x as f64,
            Self::Y => ctx.pos.y as f64,
            Self::Z => ctx.pos.z as f64,
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum UnaryOp {
    Plus,
    Minus,
    Not,
}

#[derive(Clone, Copy, Debug)]
enum BinaryOp {
    LogicalOr,
    LogicalAnd,
    Equal,
    NotEqual,
    AlmostEqual,
    Less,
    LessOrEqual,
    Greater,
    GreaterOrEqual,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Power,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Number,
    Ident,
    LParen,
    RParen,
    Comma,
    Question,
    Colon,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Bang,
    TildeEqual,
    EqualEqual,
    BangEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    AndAnd,
    OrOr,
    End,
}

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    text: String,
    offset: usize,
}

struct Lexer<'a> {
    input: &'a str,
    index: usize,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, index: 0 }
    }

    fn next_token(&mut self) -> Result<Token, String> {
        self.skip_whitespace();
        let offset = self.index;
        let Some(ch) = self.peek_char() else {
            return Ok(Token {
                kind: TokenKind::End,
                text: String::new(),
                offset,
            });
        };

        let token = match ch {
            '(' => self.single(TokenKind::LParen),
            ')' => self.single(TokenKind::RParen),
            ',' => self.single(TokenKind::Comma),
            '?' => self.single(TokenKind::Question),
            ':' => self.single(TokenKind::Colon),
            '+' => self.single(TokenKind::Plus),
            '-' => self.single(TokenKind::Minus),
            '*' => self.single(TokenKind::Star),
            '/' => self.single(TokenKind::Slash),
            '%' => self.single(TokenKind::Percent),
            '^' => self.single(TokenKind::Caret),
            '!' => {
                if self.consume_if('=') {
                    Token {
                        kind: TokenKind::BangEqual,
                        text: "!=".to_string(),
                        offset,
                    }
                } else {
                    self.single(TokenKind::Bang)
                }
            }
            '~' => {
                if self.consume_if('=') {
                    Token {
                        kind: TokenKind::TildeEqual,
                        text: "~=".to_string(),
                        offset,
                    }
                } else {
                    return Err(format!("Unexpected '~' at column {}.", offset + 1));
                }
            }
            '=' => {
                if self.consume_if('=') {
                    Token {
                        kind: TokenKind::EqualEqual,
                        text: "==".to_string(),
                        offset,
                    }
                } else {
                    return Err(format!("Unexpected '=' at column {}.", offset + 1));
                }
            }
            '<' => {
                if self.consume_if('=') {
                    Token {
                        kind: TokenKind::LessEqual,
                        text: "<=".to_string(),
                        offset,
                    }
                } else {
                    self.single(TokenKind::Less)
                }
            }
            '>' => {
                if self.consume_if('=') {
                    Token {
                        kind: TokenKind::GreaterEqual,
                        text: ">=".to_string(),
                        offset,
                    }
                } else {
                    self.single(TokenKind::Greater)
                }
            }
            '&' => {
                if self.consume_if('&') {
                    Token {
                        kind: TokenKind::AndAnd,
                        text: "&&".to_string(),
                        offset,
                    }
                } else {
                    return Err(format!("Unexpected '&' at column {}.", offset + 1));
                }
            }
            '|' => {
                if self.consume_if('|') {
                    Token {
                        kind: TokenKind::OrOr,
                        text: "||".to_string(),
                        offset,
                    }
                } else {
                    return Err(format!("Unexpected '|' at column {}.", offset + 1));
                }
            }
            '0'..='9' | '.' => self.number()?,
            _ if is_ident_start(ch) => self.ident(),
            _ => return Err(format!("Unexpected '{}' at column {}.", ch, offset + 1)),
        };
        Ok(token)
    }

    fn number(&mut self) -> Result<Token, String> {
        let offset = self.index;
        let mut seen_digit = false;
        let mut seen_dot = false;
        while let Some(ch) = self.peek_char() {
            match ch {
                '0'..='9' => {
                    seen_digit = true;
                    self.bump();
                }
                '.' if !seen_dot => {
                    seen_dot = true;
                    self.bump();
                }
                _ => break,
            }
        }
        if !seen_digit {
            return Err(format!("Invalid number at column {}.", offset + 1));
        }
        if matches!(self.peek_char(), Some('e' | 'E')) {
            self.bump();
            if matches!(self.peek_char(), Some('+' | '-')) {
                self.bump();
            }
            let exp_start = self.index;
            while matches!(self.peek_char(), Some('0'..='9')) {
                self.bump();
            }
            if exp_start == self.index {
                return Err(format!("Invalid exponent at column {}.", offset + 1));
            }
        }
        Ok(Token {
            kind: TokenKind::Number,
            text: self.input[offset..self.index].to_string(),
            offset,
        })
    }

    fn ident(&mut self) -> Token {
        let offset = self.index;
        self.bump();
        while matches!(self.peek_char(), Some(ch) if is_ident_continue(ch)) {
            self.bump();
        }
        Token {
            kind: TokenKind::Ident,
            text: self.input[offset..self.index].to_ascii_lowercase(),
            offset,
        }
    }

    fn single(&mut self, kind: TokenKind) -> Token {
        let offset = self.index;
        let text = self.peek_char().unwrap().to_string();
        self.bump();
        Token { kind, text, offset }
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek_char(), Some(ch) if ch.is_whitespace()) {
            self.bump();
        }
    }

    fn consume_if(&mut self, expected: char) -> bool {
        let Some(current) = self.peek_char() else {
            return false;
        };
        if current != expected {
            return false;
        }
        self.bump();
        true
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.index..].chars().next()
    }

    fn bump(&mut self) {
        if let Some(ch) = self.peek_char() {
            self.index += ch.len_utf8();
        }
    }
}

struct Parser<'a> {
    lexer: Lexer<'a>,
    current: Token,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Result<Self, String> {
        let mut lexer = Lexer::new(input);
        let current = lexer.next_token()?;
        Ok(Self { lexer, current })
    }

    fn parse_expression(&mut self) -> Result<Expr, String> {
        self.parse_ternary()
    }

    fn expect_end(&self) -> Result<(), String> {
        if self.current.kind == TokenKind::End {
            Ok(())
        } else {
            Err(format!(
                "Unexpected token '{}' at column {}.",
                self.current.text,
                self.current.offset + 1
            ))
        }
    }

    fn parse_ternary(&mut self) -> Result<Expr, String> {
        let condition = self.parse_logical_or()?;
        if self.current.kind != TokenKind::Question {
            return Ok(condition);
        }
        self.bump()?;
        let on_true = self.parse_expression()?;
        self.expect(TokenKind::Colon, "Expected ':' in ternary expression.")?;
        let on_false = self.parse_ternary()?;
        Ok(Expr::Ternary {
            condition: Box::new(condition),
            on_true: Box::new(on_true),
            on_false: Box::new(on_false),
        })
    }

    fn parse_logical_or(&mut self) -> Result<Expr, String> {
        self.parse_left_associative(
            Self::parse_logical_and,
            &[TokenKind::OrOr],
            |kind| match kind {
                TokenKind::OrOr => BinaryOp::LogicalOr,
                _ => unreachable!(),
            },
        )
    }

    fn parse_logical_and(&mut self) -> Result<Expr, String> {
        self.parse_left_associative(
            Self::parse_equality,
            &[TokenKind::AndAnd],
            |kind| match kind {
                TokenKind::AndAnd => BinaryOp::LogicalAnd,
                _ => unreachable!(),
            },
        )
    }

    fn parse_equality(&mut self) -> Result<Expr, String> {
        self.parse_left_associative(
            Self::parse_comparison,
            &[
                TokenKind::EqualEqual,
                TokenKind::BangEqual,
                TokenKind::TildeEqual,
            ],
            |kind| match kind {
                TokenKind::EqualEqual => BinaryOp::Equal,
                TokenKind::BangEqual => BinaryOp::NotEqual,
                TokenKind::TildeEqual => BinaryOp::AlmostEqual,
                _ => unreachable!(),
            },
        )
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        self.parse_left_associative(
            Self::parse_additive,
            &[
                TokenKind::Less,
                TokenKind::LessEqual,
                TokenKind::Greater,
                TokenKind::GreaterEqual,
            ],
            |kind| match kind {
                TokenKind::Less => BinaryOp::Less,
                TokenKind::LessEqual => BinaryOp::LessOrEqual,
                TokenKind::Greater => BinaryOp::Greater,
                TokenKind::GreaterEqual => BinaryOp::GreaterOrEqual,
                _ => unreachable!(),
            },
        )
    }

    fn parse_additive(&mut self) -> Result<Expr, String> {
        self.parse_left_associative(
            Self::parse_multiplicative,
            &[TokenKind::Plus, TokenKind::Minus],
            |kind| match kind {
                TokenKind::Plus => BinaryOp::Add,
                TokenKind::Minus => BinaryOp::Subtract,
                _ => unreachable!(),
            },
        )
    }

    fn parse_multiplicative(&mut self) -> Result<Expr, String> {
        self.parse_left_associative(
            Self::parse_power,
            &[TokenKind::Star, TokenKind::Slash, TokenKind::Percent],
            |kind| match kind {
                TokenKind::Star => BinaryOp::Multiply,
                TokenKind::Slash => BinaryOp::Divide,
                TokenKind::Percent => BinaryOp::Remainder,
                _ => unreachable!(),
            },
        )
    }

    fn parse_power(&mut self) -> Result<Expr, String> {
        let left = self.parse_unary()?;
        if self.current.kind != TokenKind::Caret {
            return Ok(left);
        }
        self.bump()?;
        let right = self.parse_power()?;
        Ok(Expr::Binary {
            op: BinaryOp::Power,
            left: Box::new(left),
            right: Box::new(right),
        })
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        match self.current.kind {
            TokenKind::Plus => {
                self.bump()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Plus,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            TokenKind::Minus => {
                self.bump()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Minus,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            TokenKind::Bang => {
                self.bump()?;
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(self.parse_unary()?),
                })
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        match self.current.kind {
            TokenKind::Number => {
                let text = self.current.text.clone();
                self.bump()?;
                let value = text
                    .parse::<f64>()
                    .map_err(|_| format!("Invalid number '{text}'."))?;
                Ok(Expr::Number(value))
            }
            TokenKind::Ident => {
                let name = self.current.text.clone();
                self.bump()?;
                if self.current.kind == TokenKind::LParen {
                    self.bump()?;
                    let mut args = Vec::new();
                    if self.current.kind != TokenKind::RParen {
                        loop {
                            args.push(self.parse_expression()?);
                            if self.current.kind == TokenKind::Comma {
                                self.bump()?;
                                continue;
                            }
                            break;
                        }
                    }
                    self.expect(TokenKind::RParen, "Expected ')' after function arguments.")?;
                    Ok(Expr::Call { name, args })
                } else if let Some(variable) = Variable::parse(&name) {
                    Ok(Expr::Variable(variable))
                } else if let Some(value) = named_constant(&name) {
                    Ok(Expr::Number(value))
                } else {
                    Err(format!(
                        "Unknown identifier '{}' at column {}.",
                        name,
                        self.current.offset + 1
                    ))
                }
            }
            TokenKind::LParen => {
                self.bump()?;
                let expr = self.parse_expression()?;
                self.expect(TokenKind::RParen, "Expected ')' to close expression.")?;
                Ok(expr)
            }
            _ => Err(format!(
                "Expected an expression at column {}, got '{}'.",
                self.current.offset + 1,
                self.current.text
            )),
        }
    }

    fn parse_left_associative(
        &mut self,
        mut parse_operand: impl FnMut(&mut Self) -> Result<Expr, String>,
        operators: &[TokenKind],
        map: impl Fn(TokenKind) -> BinaryOp,
    ) -> Result<Expr, String> {
        let mut expr = parse_operand(self)?;
        while operators.contains(&self.current.kind) {
            let op = map(self.current.kind);
            self.bump()?;
            let right = parse_operand(self)?;
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        Ok(expr)
    }

    fn expect(&mut self, kind: TokenKind, message: &str) -> Result<(), String> {
        if self.current.kind != kind {
            return Err(format!("{message} Column {}.", self.current.offset + 1));
        }
        self.bump()
    }

    fn bump(&mut self) -> Result<(), String> {
        self.current = self.lexer.next_token()?;
        Ok(())
    }
}

struct EvalContext<'a> {
    pos: BlockPos,
    before: u16,
    ctx: &'a PatternEvalContext,
    random_counter: u32,
}

fn eval_call(name: &str, args: &[Expr], ctx: &mut EvalContext<'_>) -> Result<f64, String> {
    match name {
        "abs" => unary(args, ctx, f64::abs),
        "acos" => unary(args, ctx, f64::acos),
        "asin" => unary(args, ctx, f64::asin),
        "atan" => unary(args, ctx, f64::atan),
        "atan2" => binary(args, ctx, f64::atan2),
        "cbrt" => unary(args, ctx, f64::cbrt),
        "ceil" => unary(args, ctx, f64::ceil),
        "cos" => unary(args, ctx, f64::cos),
        "cosh" => unary(args, ctx, f64::cosh),
        "exp" => unary(args, ctx, f64::exp),
        "floor" => unary(args, ctx, f64::floor),
        "ln" | "log" => unary(args, ctx, f64::ln),
        "log10" => unary(args, ctx, f64::log10),
        "max" => varargs(args, ctx, |values| {
            values.into_iter().fold(f64::NEG_INFINITY, f64::max)
        }),
        "min" => varargs(args, ctx, |values| {
            values.into_iter().fold(f64::INFINITY, f64::min)
        }),
        "rint" => unary(args, ctx, f64::round),
        "round" => unary(args, ctx, f64::round),
        "sin" => unary(args, ctx, f64::sin),
        "sinh" => unary(args, ctx, f64::sinh),
        "sqrt" => unary(args, ctx, f64::sqrt),
        "tan" => unary(args, ctx, f64::tan),
        "tanh" => unary(args, ctx, f64::tanh),
        "random" => {
            expect_arity(name, args, 0)?;
            Ok(position_random(ctx, 0))
        }
        "randint" => {
            let max = unary_value(name, args, ctx)?;
            if !max.is_finite() || max <= 0.0 {
                return Ok(0.0);
            }
            Ok((position_random(ctx, 1) * max.floor()).floor())
        }
        "query" => query(args, ctx, QueryMode::AtArgs),
        "queryabs" => query(args, ctx, QueryMode::Absolute),
        "queryrel" => query(args, ctx, QueryMode::Relative),
        _ => Err(format!("Unknown function '{name}'.")),
    }
}

fn unary(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    function: impl Fn(f64) -> f64,
) -> Result<f64, String> {
    Ok(function(unary_value("function", args, ctx)?))
}

fn binary(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    function: impl Fn(f64, f64) -> f64,
) -> Result<f64, String> {
    expect_arity("function", args, 2)?;
    Ok(function(args[0].eval(ctx)?, args[1].eval(ctx)?))
}

fn unary_value(name: &str, args: &[Expr], ctx: &mut EvalContext<'_>) -> Result<f64, String> {
    expect_arity(name, args, 1)?;
    args[0].eval(ctx)
}

fn varargs(
    args: &[Expr],
    ctx: &mut EvalContext<'_>,
    function: impl Fn(Vec<f64>) -> f64,
) -> Result<f64, String> {
    if args.is_empty() {
        return Err("Expected at least one function argument.".to_string());
    }
    let mut values = Vec::with_capacity(args.len());
    for arg in args {
        values.push(arg.eval(ctx)?);
    }
    Ok(function(values))
}

#[derive(Clone, Copy)]
enum QueryMode {
    AtArgs,
    Absolute,
    Relative,
}

fn query(args: &[Expr], ctx: &mut EvalContext<'_>, mode: QueryMode) -> Result<f64, String> {
    expect_arity("query", args, 5)?;
    let x = args[0].eval(ctx)?;
    let y = args[1].eval(ctx)?;
    let z = args[2].eval(ctx)?;
    let expected_type = args[3].eval(ctx)?;
    let expected_data = args[4].eval(ctx)?;

    let target = match mode {
        QueryMode::AtArgs | QueryMode::Absolute => BlockPos {
            x: x.round() as i32,
            y: y.round() as i32,
            z: z.round() as i32,
        },
        QueryMode::Relative => BlockPos {
            x: ctx.pos.x + x.round() as i32,
            y: ctx.pos.y + y.round() as i32,
            z: ctx.pos.z + z.round() as i32,
        },
    };

    let state_id = if target.x == ctx.pos.x && target.y == ctx.pos.y && target.z == ctx.pos.z {
        Some(ctx.before)
    } else {
        ctx.ctx.sample_block_state(target)
    }
    .ok_or_else(|| "Expression query needs world access in this command context.".to_string())?;

    let actual_type = state_id as f64;
    let actual_data = 0.0;
    let type_matches = expected_type == -1.0 || almost_equal(actual_type, expected_type);
    let data_matches = expected_data == -1.0 || almost_equal(actual_data, expected_data);
    Ok(bool_to_f64(type_matches && data_matches))
}

fn expect_arity(name: &str, args: &[Expr], expected: usize) -> Result<(), String> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(format!(
            "Function '{name}' expects {expected} argument(s), got {}.",
            args.len()
        ))
    }
}

fn truthy(value: f64) -> Truthy {
    Truthy(value > 0.0)
}

#[derive(Clone, Copy)]
struct Truthy(bool);

impl Truthy {
    fn as_f64(self) -> f64 {
        bool_to_f64(self.0)
    }

    fn not_as_f64(self) -> f64 {
        bool_to_f64(!self.0)
    }
}

fn bool_to_f64(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}

fn almost_equal(left: f64, right: f64) -> bool {
    let scale = left.abs().max(right.abs()).max(1.0);
    (left - right).abs() <= f64::EPSILON * scale * 8.0
}

fn named_constant(name: &str) -> Option<f64> {
    match name {
        "e" => Some(std::f64::consts::E),
        "pi" => Some(std::f64::consts::PI),
        "true" => Some(1.0),
        "false" => Some(0.0),
        _ => None,
    }
}

fn position_random(ctx: &mut EvalContext<'_>, salt: u32) -> f64 {
    let counter = ctx.random_counter;
    ctx.random_counter = ctx.random_counter.wrapping_add(1);
    let mut value = (ctx.pos.x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    value ^= (ctx.pos.y as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= (ctx.pos.z as u64).wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= (counter as u64).wrapping_mul(0xD6E8_FDCD_BB1C_AA95);
    value ^= salt as u64;
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as f64) / (u64::MAX as f64)
}

fn is_ident_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}

fn is_ident_continue(ch: char) -> bool {
    is_ident_start(ch) || ch.is_ascii_digit()
}

#[cfg(test)]
pub fn lookup_from_fn(lookup: impl Fn(BlockPos) -> u16 + 'static) -> Rc<dyn Fn(BlockPos) -> u16> {
    Rc::new(lookup)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    #[test]
    fn evaluates_math_with_precedence() {
        let expr = CompiledExpression::compile("1 + 2 * 3").unwrap();
        let value = expr
            .evaluate(at(0, 0, 0), 0, &PatternEvalContext::default())
            .unwrap();
        assert_eq!(value, 7.0);
    }

    #[test]
    fn evaluates_coordinate_variables() {
        let expr = CompiledExpression::compile("x + y * z").unwrap();
        let value = expr
            .evaluate(at(2, 3, 4), 0, &PatternEvalContext::default())
            .unwrap();
        assert_eq!(value, 14.0);
    }

    #[test]
    fn evaluates_ternary_conditionals() {
        let expr = CompiledExpression::compile("x > 0 ? 1 : 2").unwrap();
        assert_eq!(
            expr.evaluate(at(5, 0, 0), 0, &PatternEvalContext::default())
                .unwrap(),
            1.0
        );
        assert_eq!(
            expr.evaluate(at(-1, 0, 0), 0, &PatternEvalContext::default())
                .unwrap(),
            2.0
        );
    }

    #[test]
    fn rejects_unknown_identifiers() {
        let err = CompiledExpression::compile("foo + 1").unwrap_err();
        assert!(err.contains("Unknown identifier"));
    }

    #[test]
    fn query_rel_uses_lookup_context() {
        let expr = CompiledExpression::compile("queryRel(1,0,0,10,-1)").unwrap();
        let ctx = PatternEvalContext::with_block_lookup(
            at(0, 0, 0),
            lookup_from_fn(|pos| if pos.x == 6 { 10 } else { 0 }),
        );
        let value = expr.evaluate(at(5, 1, 1), 1, &ctx).unwrap();
        assert_eq!(value, 1.0);
    }
}
