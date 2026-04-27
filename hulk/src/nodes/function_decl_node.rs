use crate::nodes::{literal_node::LiteralNode,typedexpr_node::TypedExpr, typedexpr_node::HulkType};



#[derive(Debug)]
pub struct FunctionDecl {
    pub name: LiteralNode,
    pub params: Vec<(LiteralNode, HulkType)>,
    pub body: TypedExpr,
}

impl FunctionDecl {
    pub fn new(name: LiteralNode, params: Vec<(LiteralNode,HulkType)>, body: TypedExpr) -> Self {
        FunctionDecl { name, params, body }
    }
}