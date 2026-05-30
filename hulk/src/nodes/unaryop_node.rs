use crate::nodes::expr_node::{Expr, HulkType};


#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Neg,
    Not,
    Plus,
}

#[derive(Debug)]
pub struct UnaryOpNode {
    pub op: UnaryOp,
    pub expr: Box<Expr>,
    pub return_type: HulkType,
}

impl UnaryOpNode {
    pub fn new(op: UnaryOp, expr: Expr) -> Self {
        UnaryOpNode {
            op,
            expr: Box::new(expr),
            return_type: HulkType::Unknown
        }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}