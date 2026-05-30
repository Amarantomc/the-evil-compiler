use crate::nodes::{expr_node::{Expr, HulkType}, literal_node::LiteralNode};


#[derive(Debug)]
pub struct FunCallNode {
    pub name: LiteralNode,
    pub args: Vec<Expr>,
    pub return_type: HulkType
}

impl FunCallNode {
    pub fn new(name: LiteralNode, args: Vec<Expr>) -> Self {
        FunCallNode { name, args, return_type: HulkType::Unknown }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}