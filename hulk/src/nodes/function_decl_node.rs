use crate::nodes::{literal_node::LiteralNode,expr_node::Expr, expr_node::HulkType};



#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: LiteralNode,
    pub params: Vec<(LiteralNode, HulkType)>,
    pub body: Expr,
    pub generics: Vec<String>, 
    pub return_type: HulkType,
}

impl FunctionDecl { //Revisar Grammar para return unkwon
        pub fn new(
        name: LiteralNode,
        generics: Vec<String>,
        params: Vec<(LiteralNode, HulkType)>,
        body: Expr,
        return_type: HulkType,
    ) -> Self {
        FunctionDecl { name, generics, params, body, return_type }
    }

    pub fn is_generic(&self) -> bool { !self.generics.is_empty() }
}