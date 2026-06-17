
#[derive(Debug, Clone)]
pub struct LiteralNode {
    pub value: Literal,
    pub span: (usize, usize),
}

impl LiteralNode {
     pub fn new(value: Literal) -> Self {
        LiteralNode { value, span: (0, 0) }
    }
    pub fn new_spanned(value: Literal, span: (usize, usize)) -> Self {
        LiteralNode { value, span }
    }
}

#[derive(Debug, Clone)]
pub enum Literal {
    Number(f32),
    Bool(bool),
    Str(String),
    Id(String)
}

impl Literal {
    pub fn as_id(&self) -> String {
        match self {
            Literal::Id(s) => s.clone(),
            _ => panic!("Expected Id"),
        }
    }
}