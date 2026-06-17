#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // ---- literales con payload ----
    Int(usize),
    Float(f32),
    Str(String),
    Ident(String),

    // ---- palabras clave ----
    If, Elif, Else, While, For, In, Range, Let, Type, Function,
    Inherits, New, Base, SelfKw, True, False, Is, As,
    KwNumber, KwString, KwBoolean,

    // ---- operadores y puntuación ----
    AtAt, At,            // @@  @
    ColonColon,          // ::
    Assign,              // :=
    Colon,               // :
    FatArrow,            // =>
    EqEq, Eq,            // ==  =
    Ne, Bang,            // !=  !
    Le, Lt,              // <=  <
    Ge, Gt,              // >=  >
    AndAnd, Amp,         // &&  &
    OrOr, Pipe,          // ||  |
    Plus, Minus, Star, Slash, Percent, Caret,
    LParen, RParen, LBrace, RBrace,
    Comma, Semi, Dot,
}