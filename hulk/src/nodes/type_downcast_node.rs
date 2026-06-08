use crate::nodes::{expr_node::{Expr, HulkType}, literal_node::LiteralNode};


#[derive(Debug)]
pub struct TypeDowncastNode {
      pub expr: Box<Expr>,
      pub target_type: LiteralNode,
      pub return_type: HulkType,
}

impl TypeDowncastNode {
    pub fn new(expr: Expr, target_type: LiteralNode) -> Self {
        TypeDowncastNode {
            expr:Box::new(expr),
            target_type,
            return_type: HulkType::Unknown,
        }
    }
     
}