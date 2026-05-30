use crate::nodes::expr_node::{Expr, HulkType};



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
    SingleConc,
    SpacedConc,
}

#[derive(Debug)]
pub struct BinaryOpNode {
    pub left: Box<Expr>,
    pub op: BinaryOp,
    pub right: Box<Expr>,
    pub return_type: HulkType,
}

impl BinaryOpNode {
    pub fn new(left: Expr, op: BinaryOp, right: Expr) -> Self {
        BinaryOpNode {
            left: Box::new(left),
            op,
            right: Box::new(right),
            return_type: HulkType::Unknown,
        }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}