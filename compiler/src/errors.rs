

use lalrpop_util::ParseError;
use crate::lexer::token::Token;
use crate::lexer::lexer::LexicalError;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase { Lexical, Syntactic, Semantic }

impl Phase {
    pub fn tag(self) -> &'static str {
        match self {
            Phase::Lexical => "LEXICAL",
            Phase::Syntactic => "SYNTACTIC",
            Phase::Semantic => "SEMANTIC",
        }
    }
    /// Código de salida del contrato.
    pub fn exit_code(self) -> i32 {
        match self {
            Phase::Lexical => 1,
            Phase::Syntactic => 2,
            Phase::Semantic => 3,
        }
    }
}

pub struct Diagnostic {
    pub phase: Phase,
    pub line: usize,
    pub col: usize,
    pub message: String,
}

impl Diagnostic {
    pub fn new(phase: Phase, line: usize, col: usize, message: impl Into<String>) -> Self {
        Diagnostic { phase, line, col, message: message.into() }
    }
    /// `(line,col) TYPE: message`
    pub fn format(&self) -> String {
        format!("({},{}) {}: {}", self.line, self.col, self.phase.tag(), self.message)
    }
}

/// Offset de byte -> (línea, columna) 1-based.
pub fn line_col(src: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in src.char_indices() {
        if i >= offset { break; }
        if ch == '\n' { line += 1; col = 1; } else { col += 1; }
    }
    (line, col)
}

/// Traduce un error de LALRPOP (que incluye los léxicos vía `User`) al
/// diagnóstico del contrato, distinguiendo LEXICAL de SYNTACTIC.
pub fn from_parse_error(src: &str, e: ParseError<usize, Token, LexicalError>) -> Diagnostic {
    match e {
        // Error del lexer externo -> LÉXICO (exit 1)
        ParseError::User { error } => {
            let (line, col) = line_col(src, error.pos);
            Diagnostic::new(Phase::Lexical, line, col, error.message)
        }
        // El resto -> SINTÁCTICO (exit 2)
        ParseError::InvalidToken { location } => {
            let (line, col) = line_col(src, location);
            Diagnostic::new(Phase::Syntactic, line, col, "token inválido")
        }
        ParseError::UnrecognizedEof { location, expected } => {
            let (line, col) = line_col(src, location);
            Diagnostic::new(Phase::Syntactic, line, col,
                format!("fin de entrada inesperado{}", fmt_expected(&expected)))
        }
        ParseError::UnrecognizedToken { token: (start, tok, _), expected } => {
            let (line, col) = line_col(src, start);
            Diagnostic::new(Phase::Syntactic, line, col,
                format!("token inesperado {}{}", describe(&tok), fmt_expected(&expected)))
        }
        ParseError::ExtraToken { token: (start, tok, _) } => {
            let (line, col) = line_col(src, start);
            Diagnostic::new(Phase::Syntactic, line, col,
                format!("token sobrante {}", describe(&tok)))
        }
    }
}

fn fmt_expected(expected: &[String]) -> String {
    if expected.is_empty() { String::new() }
    else { format!("; se esperaba {}", expected.join(", ")) }
}

/// Representación legible de un token para los mensajes de error.
fn describe(tok: &Token) -> String {
    match tok {
        Token::Ident(s) => format!("identificador '{}'", s),
        Token::Int(n) => format!("número '{}'", n),
        Token::Float(f) => format!("número '{}'", f),
        Token::Str(s) => format!("cadena \"{}\"", s),
        other => format!("'{:?}'", other),
    }
}