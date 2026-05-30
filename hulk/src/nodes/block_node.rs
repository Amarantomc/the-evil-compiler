use crate::nodes::expr_node::{Expr, HulkType};


#[derive(Debug)]
pub struct BlockNode {
    pub expressions: Vec<Expr>,
    pub return_type: HulkType,
}

impl BlockNode {
    pub fn new(expressions: Vec<Expr>) -> Self {
        BlockNode { expressions, return_type: HulkType::Unknown }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}