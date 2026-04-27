
#[derive(Debug)]
pub struct LiteralNode {
    pub value: Literal,
}

impl LiteralNode {
    pub fn new(value: Literal) -> Self {
        LiteralNode { value }
    }
}

#[derive(Debug)]
pub enum Literal {
    Number(f32),
    Bool(bool),
    Str(String),
    Id(String)
}