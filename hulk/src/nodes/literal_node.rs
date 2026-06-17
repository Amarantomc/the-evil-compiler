
#[derive(Debug, Clone)]
pub struct LiteralNode {
    pub value: Literal,
}

impl LiteralNode {
    pub fn new(value: Literal) -> Self {
        LiteralNode { value }
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