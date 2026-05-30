use crate::nodes::{expr_node::{Expr, HulkType}, literal_node::LiteralNode};


#[derive(Debug)]
pub struct ForNode {
    pub variable: LiteralNode,
    pub iterator: Box<Expr>,
    pub body: Box<Expr>,
    pub return_type: HulkType,
}

impl ForNode {
    pub fn new(variable: LiteralNode, iterator: Expr, body: Expr) -> Self {
        ForNode {
            variable,
            iterator: Box::new(iterator),
            body: Box::new(body),
            return_type: HulkType::Unknown,
        }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}