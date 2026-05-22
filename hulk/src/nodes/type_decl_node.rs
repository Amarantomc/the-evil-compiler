use crate::nodes::{literal_node::LiteralNode, typedexpr_node::{TypedExpr, HulkType}, function_decl_node::FunctionDecl};

#[derive(Debug)]
pub struct AttributeNode {
    pub name: LiteralNode,
    pub type_annotation: HulkType,
    pub initializer: TypedExpr,
}

impl AttributeNode {
    pub fn new(name: LiteralNode, type_annotation: HulkType, initializer: TypedExpr) -> Self {
        Self { name, type_annotation, initializer }
    }
}

#[derive(Debug)]
pub struct TypeDeclNode {
    pub name: LiteralNode,
    pub params: Vec<(LiteralNode, HulkType)>,
    pub attributes: Vec<AttributeNode>,
    pub methods: Vec<FunctionDecl>,
}

pub enum TypeBodyItem {
    Assignment(AttributeNode),
    FunctionDecl(FunctionDecl),
}

impl TypeDeclNode {
    pub fn new(name: LiteralNode, params: Vec<(LiteralNode, HulkType)>, attributes: Vec<AttributeNode>, methods: Vec<FunctionDecl>) -> Self {
        Self { name, params, attributes, methods }
    }
}
