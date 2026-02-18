#[derive(Debug)]
pub enum Expr {
    Number(i32),
    Op(Box<Expr>, Opcode, Box<Expr>),
}

#[derive(Debug)]
pub enum Opcode{
    Mul,
    Div,
    Add,
    Sub,
    Pow,
    Sqrt,
    Equal,
    Dist,
    Gequa,
    Lequa,
    Great,
    Less,
    And,
    Or,
}
