use crate::nodes::{literal_node::LiteralNode, expr_node::{HulkType, Expr}};


#[derive(Debug)]
pub struct LetNode {
    pub assignments: Vec<((LiteralNode,HulkType), Expr)>,
    pub body: Box<Expr>,
    pub return_type: HulkType,
}

impl LetNode {
    pub fn new(assignments: Vec<((LiteralNode,HulkType), Expr)>, body: Expr) -> Self {
        LetNode {
            assignments,
            body: Box::new(body),
            return_type: HulkType::Unknown,
        }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}