use crate::expr_visitor::ExprVisitor;
#[derive(Debug)]
pub enum Expr {
    Number(i32),
    Op(Box<Expr>, Opcode, Box<Expr>),
}

#[derive(Debug)]
pub enum Opcode {
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

impl Expr {
    pub fn accept<T>(&self, v: &mut impl ExprVisitor<T>) -> T {
        match self {
            Expr::Number(n) => v.visit_number(*n),
            Expr::Op(left, op, right) => v.visit_binary_op(left, op, right),
        }
    }
}
