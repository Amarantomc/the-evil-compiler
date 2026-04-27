use crate::nodes::typedexpr_node::TypedExpr;


#[derive(Debug)]
pub struct BlockNode {
    pub expressions: Vec<TypedExpr>,
}

impl BlockNode {
    pub fn new(expressions: Vec<TypedExpr>) -> Self {
        BlockNode { expressions }
    }
}