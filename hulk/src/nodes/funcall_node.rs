use crate::nodes::{literal_node::LiteralNode, typedexpr_node::TypedExpr};


#[derive(Debug)]
pub struct FunCallNode {
    pub name: LiteralNode,
    pub args: Vec<TypedExpr>,
}

impl FunCallNode {
    pub fn new(name: LiteralNode, args: Vec<TypedExpr>) -> Self {
        FunCallNode { name, args }
    }
}