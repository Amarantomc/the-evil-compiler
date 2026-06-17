use crate::nodes::{expr_node::{Expr, HulkType}, literal_node::{LiteralNode}};


#[derive(Debug, Clone)]
pub struct TypeTestNode {
     pub expr: Box<Expr>,
     pub  target_type: LiteralNode,
     pub return_type: HulkType,
}

impl TypeTestNode {
    pub fn new(expr:Expr, target_type: LiteralNode) -> Self {
        TypeTestNode {
            expr:Box::new(expr),
            target_type,
            return_type: HulkType::Unknown,
        }
    }
     
}