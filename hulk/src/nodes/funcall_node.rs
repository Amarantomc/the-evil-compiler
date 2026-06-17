use crate::nodes::{expr_node::{Expr, HulkType}, literal_node::LiteralNode};


#[derive(Debug, Clone)]
pub struct FunCallNode {
    pub name: LiteralNode,
    pub args: Vec<Expr>,
    pub type_args: Vec<HulkType>, 
    pub return_type: HulkType
}

impl FunCallNode {
    pub fn new(name: LiteralNode, args: Vec<Expr>) -> Self {
        FunCallNode { name, type_args: vec![], args, return_type: HulkType::Unknown }
    }
    pub fn with_type_args(name: LiteralNode, type_args: Vec<HulkType>, args: Vec<Expr>) -> Self {
        FunCallNode { name, type_args, args, return_type: HulkType::Unknown }
    }
    pub fn set_type(&mut self, t: HulkType) { self.return_type = t; }
}