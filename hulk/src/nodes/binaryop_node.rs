use crate::nodes::typedexpr_node::TypedExpr;



#[derive(Debug, Clone, Copy)]
pub enum BinaryOp {
    Mul,
    Div,
    Add,
    Sub,
    Pow,
    Equal,
    Dist,
    Gequa,
    Lequa,
    Great,
    Less,
    And,
    Or,
    Mod,
}

#[derive(Debug)]
pub struct BinaryOpNode {
    pub left: Box<TypedExpr>,
    pub op: BinaryOp,
    pub right: Box<TypedExpr>,
}

impl BinaryOpNode {
    pub fn new(left: TypedExpr, op: BinaryOp, right: TypedExpr) -> Self {
        BinaryOpNode {
            left: Box::new(left),
            op,
            right: Box::new(right),
        }
    }
}