use super::CalcError;

#[derive(Clone, Debug)]
pub(super) struct CellRef {
    pub table_id: Option<u64>,
    pub row: usize,
    pub column: usize,
    pub abs_row: bool,
    pub abs_column: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum UnaryOp {
    Plus,
    Minus,
    Percent,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
    Concat,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Clone, Debug)]
pub(super) enum Expr {
    Number(f64),
    Text(String),
    Bool(bool),
    Ref(CellRef),
    Range(CellRef, CellRef),
    Unary(UnaryOp, Box<Expr>),
    Binary(BinaryOp, Box<Expr>, Box<Expr>),
    Call(String, Vec<Expr>),
}

#[derive(Clone, Debug)]
enum Token {
    Number(f64),
    Text(String),
    Ident(String),
    Ref(CellRef),
    TableRef(u64, CellRef),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    Amp,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    LParen,
    RParen,
    Comma,
    Colon,
    End,
}

pub(super) fn parse_formula(source: &str) -> Result<Expr, CalcError> {
    let source = source.strip_prefix('=').ok_or(CalcError::Parse)?;
    let mut parser = Parser {
        tokens: tokenize(source)?,
        index: 0,
    };
    let expr = parser.parse_expr(0)?;
    if !matches!(parser.peek(), Token::End) {
        return Err(CalcError::Parse);
    }
    Ok(expr)
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn peek(&self) -> &Token {
        self.tokens.get(self.index).unwrap_or(&Token::End)
    }
    fn next(&mut self) -> Token {
        let t = self.peek().clone();
        self.index += 1;
        t
    }
    fn eat(&mut self, wanted: fn(&Token) -> bool) -> bool {
        if wanted(self.peek()) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn parse_expr(&mut self, min_prec: u8) -> Result<Expr, CalcError> {
        let mut left = self.parse_prefix()?;
        loop {
            if matches!(self.peek(), Token::Percent) && min_prec <= 7 {
                self.next();
                left = Expr::Unary(UnaryOp::Percent, Box::new(left));
                continue;
            }
            let Some((prec, op, right_assoc)) = binary_info(self.peek()) else {
                break;
            };
            if prec < min_prec {
                break;
            }
            self.next();
            let right = self.parse_expr(if right_assoc { prec } else { prec + 1 })?;
            left = Expr::Binary(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_prefix(&mut self) -> Result<Expr, CalcError> {
        match self.next() {
            Token::Plus => Ok(Expr::Unary(UnaryOp::Plus, Box::new(self.parse_expr(5)?))),
            Token::Minus => Ok(Expr::Unary(UnaryOp::Minus, Box::new(self.parse_expr(5)?))),
            Token::Number(n) => Ok(Expr::Number(n)),
            Token::Text(s) => Ok(Expr::Text(s)),
            Token::Ref(a) => {
                if self.eat(|t| matches!(t, Token::Colon)) {
                    match self.next() {
                        Token::Ref(b) => Ok(Expr::Range(a, b)),
                        _ => Err(CalcError::Parse),
                    }
                } else {
                    Ok(Expr::Ref(a))
                }
            }
            Token::TableRef(id, mut a) => {
                a.table_id = Some(id);
                if self.eat(|t| matches!(t, Token::Colon)) {
                    match self.next() {
                        Token::Ref(b) => Ok(Expr::Range(a, b)),
                        _ => Err(CalcError::Parse),
                    }
                } else {
                    Ok(Expr::Ref(a))
                }
            }
            Token::Ident(name) => {
                if self.eat(|t| matches!(t, Token::LParen)) {
                    let mut args = Vec::new();
                    if !self.eat(|t| matches!(t, Token::RParen)) {
                        loop {
                            args.push(self.parse_expr(0)?);
                            if self.eat(|t| matches!(t, Token::RParen)) {
                                break;
                            }
                            if !self.eat(|t| matches!(t, Token::Comma)) {
                                return Err(CalcError::Parse);
                            }
                        }
                    }
                    Ok(Expr::Call(name, args))
                } else if name.eq_ignore_ascii_case("TRUE") {
                    Ok(Expr::Bool(true))
                } else if name.eq_ignore_ascii_case("FALSE") {
                    Ok(Expr::Bool(false))
                } else {
                    Err(CalcError::Name)
                }
            }
            Token::LParen => {
                let e = self.parse_expr(0)?;
                if !self.eat(|t| matches!(t, Token::RParen)) {
                    return Err(CalcError::Parse);
                }
                Ok(e)
            }
            _ => Err(CalcError::Parse),
        }
    }
}

fn binary_info(token: &Token) -> Option<(u8, BinaryOp, bool)> {
    Some(match token {
        Token::Eq => (1, BinaryOp::Eq, false),
        Token::Ne => (1, BinaryOp::Ne, false),
        Token::Lt => (1, BinaryOp::Lt, false),
        Token::Le => (1, BinaryOp::Le, false),
        Token::Gt => (1, BinaryOp::Gt, false),
        Token::Ge => (1, BinaryOp::Ge, false),
        Token::Amp => (2, BinaryOp::Concat, false),
        Token::Plus => (3, BinaryOp::Add, false),
        Token::Minus => (3, BinaryOp::Sub, false),
        Token::Star => (4, BinaryOp::Mul, false),
        Token::Slash => (4, BinaryOp::Div, false),
        Token::Caret => (6, BinaryOp::Pow, true),
        _ => return None,
    })
}

fn tokenize(s: &str) -> Result<Vec<Token>, CalcError> {
    let mut out = Vec::new();
    let mut i = 0;
    let b = s.as_bytes();
    while i < b.len() {
        if b[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let ch = b[i] as char;
        match ch {
            '"' => {
                i += 1;
                let mut text = String::new();
                let mut closed = false;
                while i < b.len() {
                    if b[i] == b'"' {
                        if i + 1 < b.len() && b[i + 1] == b'"' {
                            text.push('"');
                            i += 2;
                        } else {
                            i += 1;
                            closed = true;
                            break;
                        }
                    } else {
                        let rest = &s[i..];
                        let c = rest.chars().next().ok_or(CalcError::Parse)?;
                        text.push(c);
                        i += c.len_utf8();
                    }
                }
                if !closed {
                    return Err(CalcError::Parse);
                }
                out.push(Token::Text(text));
            }
            '(' => {
                out.push(Token::LParen);
                i += 1;
            }
            ')' => {
                out.push(Token::RParen);
                i += 1;
            }
            ',' => {
                out.push(Token::Comma);
                i += 1;
            }
            ':' => {
                out.push(Token::Colon);
                i += 1;
            }
            '+' => {
                out.push(Token::Plus);
                i += 1;
            }
            '-' => {
                out.push(Token::Minus);
                i += 1;
            }
            '*' => {
                out.push(Token::Star);
                i += 1;
            }
            '/' => {
                out.push(Token::Slash);
                i += 1;
            }
            '^' => {
                out.push(Token::Caret);
                i += 1;
            }
            '%' => {
                out.push(Token::Percent);
                i += 1;
            }
            '&' => {
                out.push(Token::Amp);
                i += 1;
            }
            '=' => {
                out.push(Token::Eq);
                i += 1;
            }
            '<' => {
                if i + 1 < b.len() && b[i + 1] == b'=' {
                    out.push(Token::Le);
                    i += 2;
                } else if i + 1 < b.len() && b[i + 1] == b'>' {
                    out.push(Token::Ne);
                    i += 2;
                } else {
                    out.push(Token::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < b.len() && b[i + 1] == b'=' {
                    out.push(Token::Ge);
                    i += 2;
                } else {
                    out.push(Token::Gt);
                    i += 1;
                }
            }
            '$' if i + 1 < b.len() && b[i + 1].is_ascii_alphabetic() => {
                let (cell, end) = parse_cell_ref(s, i, None)?;
                out.push(Token::Ref(cell));
                i = end;
            }
            c if c.is_ascii_alphabetic() => {
                if let Ok((cell, end)) = parse_cell_ref(s, i, None) {
                    out.push(Token::Ref(cell));
                    i = end;
                } else {
                    let start = i;
                    while i < b.len()
                        && (b[i].is_ascii_alphanumeric() || b[i] == b'.' || b[i] == b'_')
                    {
                        i += 1;
                    }
                    out.push(Token::Ident(s[start..i].to_owned()));
                }
            }
            c if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i < b.len() && b[i] == b':' && start < i {
                    if let Ok((cell, end)) = parse_cell_ref(s, i + 1, None) {
                        if let Ok(id) = s[start..i].parse::<u64>() {
                            out.push(Token::TableRef(id, cell));
                            i = end;
                            continue;
                        }
                    }
                }
                if i < b.len() && b[i] == b'.' {
                    i += 1;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                    let exponent = i;
                    i += 1;
                    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
                        i += 1;
                    }
                    let digits = i;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                    if digits == i {
                        i = exponent;
                    }
                }
                let n = s[start..i].parse::<f64>().map_err(|_| CalcError::Parse)?;
                out.push(Token::Number(n));
            }
            _ => return Err(CalcError::Parse),
        }
    }
    out.push(Token::End);
    Ok(out)
}

fn parse_cell_ref(
    s: &str,
    start: usize,
    table_id: Option<u64>,
) -> Result<(CellRef, usize), CalcError> {
    let b = s.as_bytes();
    let mut i = start;
    let abs_column = i < b.len() && b[i] == b'$';
    if abs_column {
        i += 1;
    }
    let col_start = i;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    if col_start == i {
        return Err(CalcError::Parse);
    }
    let column = column_number(&s[col_start..i]).ok_or(CalcError::Ref)?;
    let abs_row = i < b.len() && b[i] == b'$';
    if abs_row {
        i += 1;
    }
    let row_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if row_start == i {
        return Err(CalcError::Parse);
    }
    let row = s[row_start..i]
        .parse::<usize>()
        .map_err(|_| CalcError::Ref)?;
    if row == 0 {
        return Err(CalcError::Ref);
    }
    Ok((
        CellRef {
            table_id,
            row: row - 1,
            column: column - 1,
            abs_row,
            abs_column,
        },
        i,
    ))
}

fn column_number(s: &str) -> Option<usize> {
    let mut n = 0usize;
    for b in s.bytes() {
        n = n
            .checked_mul(26)?
            .checked_add((b.to_ascii_uppercase() - b'A' + 1) as usize)?;
    }
    Some(n)
}
