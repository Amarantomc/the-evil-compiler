use regex::Regex;

use crate::lexer::token::Token;



#[derive(Debug, Clone, PartialEq)]
pub struct LexicalError {
    pub message: String,
    pub pos: usize,
}

 
enum Kind {
    Ident,              
    Int,
    Float,
    StrLit,
    Fixed(Token),       
}

pub struct Lexer<'input> {
    input: &'input str,
    pos: usize,
    ws: Regex,
    rules: Vec<(Regex, Kind)>,
}

impl<'input> Lexer<'input> {
    pub fn new(input: &'input str) -> Self {
        // Orden = prioridad. Patrones multi-carácter ANTES que los de un
        // carácter ⇒ "maximal munch" determinista (@@ antes que @, := antes
        // que :, float antes que int, etc.). Todos anclados con ^.
        let r = |p: &str| Regex::new(p).unwrap();
        let rules = vec![
            // identificadores (las keywords se promueven después)
            (r(r"^[A-Za-z_][A-Za-z0-9_]*"), Kind::Ident),
            // números: float antes que int
            (r(r"^[0-9]+\.[0-9]+"), Kind::Float),
            (r(r"^[0-9]+"), Kind::Int),
            // cadena
            (r(r#"^"(?:\\[\s\S]|[^"\\])*""#), Kind::StrLit),
            // operadores multi-carácter
            (r(r"^@@"), Kind::Fixed(Token::AtAt)),
            (r(r"^::"), Kind::Fixed(Token::ColonColon)),
            (r(r"^:="), Kind::Fixed(Token::Assign)),
            (r(r"^=>"), Kind::Fixed(Token::FatArrow)),
            (r(r"^=="), Kind::Fixed(Token::EqEq)),
            (r(r"^!="), Kind::Fixed(Token::Ne)),
            (r(r"^<="), Kind::Fixed(Token::Le)),
            (r(r"^>="), Kind::Fixed(Token::Ge)),
            (r(r"^&&"), Kind::Fixed(Token::AndAnd)),
            (r(r"^\|\|"), Kind::Fixed(Token::OrOr)),
            // operadores de un carácter
            (r(r"^@"), Kind::Fixed(Token::At)),
            (r(r"^:"), Kind::Fixed(Token::Colon)),
            (r(r"^="), Kind::Fixed(Token::Eq)),
            (r(r"^!"), Kind::Fixed(Token::Bang)),
            (r(r"^<"), Kind::Fixed(Token::Lt)),
            (r(r"^>"), Kind::Fixed(Token::Gt)),
            (r(r"^&"), Kind::Fixed(Token::Amp)),
            (r(r"^\|"), Kind::Fixed(Token::Pipe)),
            (r(r"^\+"), Kind::Fixed(Token::Plus)),
            (r(r"^-"), Kind::Fixed(Token::Minus)),
            (r(r"^\*"), Kind::Fixed(Token::Star)),
            (r(r"^/"), Kind::Fixed(Token::Slash)),
            (r(r"^%"), Kind::Fixed(Token::Percent)),
            (r(r"^\^"), Kind::Fixed(Token::Caret)),
            (r(r"^\("), Kind::Fixed(Token::LParen)),
            (r(r"^\)"), Kind::Fixed(Token::RParen)),
            (r(r"^\{"), Kind::Fixed(Token::LBrace)),
            (r(r"^\}"), Kind::Fixed(Token::RBrace)),
            (r(r"^,"), Kind::Fixed(Token::Comma)),
            (r(r"^;"), Kind::Fixed(Token::Semi)),
            (r(r"^\."), Kind::Fixed(Token::Dot)),
        ];
        Lexer { input, pos: 0, ws: r(r"^\s+"), rules }
    }

    fn keyword(text: &str) -> Option<Token> {
        Some(match text {
            "if" => Token::If, "elif" => Token::Elif, "else" => Token::Else,
            "while" => Token::While, "for" => Token::For, "in" => Token::In,
            "let" => Token::Let, "type" => Token::Type,
            "function" => Token::Function, "inherits" => Token::Inherits,
            "new" => Token::New, "base" => Token::Base, "self" => Token::SelfKw,
            "true" => Token::True, "false" => Token::False,
            "is" => Token::Is, "as" => Token::As,
            "Number" => Token::KwNumber, "String" => Token::KwString,
            "Boolean" => Token::KwBoolean,
            _ => return None,
        })
    }
}

fn unescape_string(content: &str) -> Result<String, usize> {
    let mut out = String::new();
    let mut i = 0usize;
    while i < content.len() {
        let c = content[i..].chars().next().unwrap();
        if c == '\\' {
            match content[i + 1..].chars().next() {
                Some(e) => {
                    let decoded = match e {
                        'n'  => '\n',
                        't'  => '\t',
                        'r'  => '\r',
                        '\\' => '\\',
                        '"'  => '"',
                        '\'' => '\'',
                        '0'  => '\0',
                        _    => return Err(i), // escape no reconocido
                    };
                    out.push(decoded);
                    i += 1 + e.len_utf8();
                }
                None => return Err(i), // barra invertida al final (no debería ocurrir)
            }
        } else {
            out.push(c);
            i += c.len_utf8();
        }
    }
    Ok(out)
}

impl<'input> Iterator for Lexer<'input> {
    type Item = Result<(usize, Token, usize), LexicalError>;

    fn next(&mut self) -> Option<Self::Item> {
        // Bucle para saltar espacios en blanco y comentarios consecutivamente
        loop {
            if self.pos >= self.input.len() {
                return None;
            }

            let rest = &self.input[self.pos..];

            // 1. Saltar espacios en blanco
            if let Some(m) = self.ws.find(rest) {
                self.pos += m.end();
                continue; // Vuelve a chequear el bucle
            }

            // 2. Saltar comentarios de una sola línea (// ...)
            if rest.starts_with("//") {
                if let Some(newline_pos) = rest.find('\n') {
                    self.pos += newline_pos + 1; // Avanza pasando el salto de línea
                } else {
                    self.pos = self.input.len(); // El comentario termina el archivo
                }
                continue;
            }

             
            if rest.starts_with("/*") {
                if let Some(end_comment_pos) = rest.find("*/") {
                    self.pos += end_comment_pos + 2; // Avanza pasando el "*/"
                } else {
                    // Error léxico: Comentario de bloque abierto pero nunca cerrado
                    let pos = self.pos;
                    self.pos = self.input.len();
                    return Some(Err(LexicalError {
                        message: "Comentario de bloque sin cerrar (Falta '*/')".to_string(),
                        pos,
                    }));
                }
                continue;
            }

            // Si no hay espacios ni comentarios, salimos del bucle para procesar tokens reales
            break;
        }

         
        if self.pos >= self.input.len() {
            return None;
        }

        let rest = &self.input[self.pos..];
        for (re, kind) in &self.rules {
            if let Some(m) = re.find(rest) {
                let text = &rest[..m.end()];
                let start = self.pos;
                let end = self.pos + m.end();
                self.pos = end;

                let tok = match kind {
                    Kind::Ident => Self::keyword(text)
                        .unwrap_or_else(|| Token::Ident(text.to_string())),
                    Kind::Int => Token::Int(text.parse().unwrap()),
                    Kind::Float => Token::Float(text.parse().unwrap()),
                    Kind::StrLit => {
                        // Quitar las comillas y decodificar las secuencias de escape.
                        let content = &text[1..text.len() - 1];
                        match unescape_string(content) {
                            Ok(decoded) => Token::Str(decoded),
                            Err(rel) => {
                                // `rel` = offset del `\\` ofensor dentro del contenido.
                                let bad_pos = start + 1 + rel;
                                return Some(Err(LexicalError {
                                    message: "secuencia de escape inválida en cadena".to_string(),
                                    pos: bad_pos,
                                }));
                            }
                        }
                    }
                    Kind::Fixed(t) => t.clone(),
                };
                return Some(Ok((start, tok, end)));
            }
        }

        let pos = self.pos;
        self.pos = self.input.len();
        Some(Err(LexicalError {
            message: format!("carácter inesperado: {:?}", rest.chars().next().unwrap()),
            pos,
        }))
    }
}