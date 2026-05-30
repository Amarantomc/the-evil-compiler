use crate::nodes::expr_node::{Expr, HulkType};


#[derive(Debug)]
pub struct DestAssignNode {
    pub target: Box<Expr>,
    pub expr: Box<Expr>,
    pub return_type: HulkType,
}

impl DestAssignNode {
    pub fn new(target: Expr, expr: Expr) -> Self {
        DestAssignNode {
            target: Box::new(target),
            expr: Box::new(expr),
            return_type: HulkType::Unknown
        }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}