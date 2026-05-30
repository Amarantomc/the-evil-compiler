use crate::nodes::expr_node::{Expr, HulkType};


#[derive(Debug)]
pub struct WhileNode {
    pub condition: Box<Expr>,
    pub body: Box<Expr>,
    pub return_type: HulkType,
}

impl WhileNode {
    pub fn new(condition: Expr, body: Expr) -> Self {
        WhileNode {
            condition: Box::new(condition),
            body: Box::new(body),
            return_type: HulkType::Unknown
        }
    }
}