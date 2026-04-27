use crate::nodes::{literal_node::LiteralNode, typedexpr_node::TypedExpr};


#[derive(Debug)]
pub struct DestAssignNode {
    pub identifier: LiteralNode,
    pub expr: Box<TypedExpr>,
}

impl DestAssignNode {
    pub fn new(identifier: LiteralNode, expr: TypedExpr) -> Self {
        DestAssignNode {
            identifier,
            expr: Box::new(expr),
        }
    }
}