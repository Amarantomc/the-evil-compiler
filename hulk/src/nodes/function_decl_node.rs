use crate::nodes::{literal_node::LiteralNode,expr_node::Expr, expr_node::HulkType};



#[derive(Debug)]
pub struct FunctionDecl {
    pub name: LiteralNode,
    pub params: Vec<(LiteralNode, HulkType)>,
    pub body: Expr,
    pub return_type: HulkType,
}

impl FunctionDecl { //Revisar Grammar para return unkwon
    pub fn new(name: LiteralNode, params: Vec<(LiteralNode,HulkType)>, body: Expr,return_type: HulkType ) -> Self {
        FunctionDecl { name, params, body, return_type }
    }
}