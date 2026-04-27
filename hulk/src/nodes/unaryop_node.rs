use crate::nodes::typedexpr_node::TypedExpr;


#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
    Plus,
}

#[derive(Debug)]
pub struct UnaryOpNode {
    pub op: UnaryOp,
    pub expr: Box<TypedExpr>,
}

impl UnaryOpNode {
    pub fn new(op: UnaryOp, expr: TypedExpr) -> Self {
        UnaryOpNode {
            op,
            expr: Box::new(expr),
        }
    }
}