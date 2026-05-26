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
pub struct InheritanceClause {
    pub parent_name: LiteralNode,
    pub parent_args: Option<Vec<TypedExpr>>, // None = heredar argumentos implícitamente
}

impl InheritanceClause {
    pub fn new(parent_name: LiteralNode, parent_args: Option<Vec<TypedExpr>>) -> Self {
        Self { parent_name, parent_args }
    }
}

#[derive(Debug)]
pub struct TypeDeclNode {
    pub name: LiteralNode,
    pub params: Vec<(LiteralNode, HulkType)>,
    pub attributes: Vec<AttributeNode>,
    pub methods: Vec<FunctionDecl>,
    pub inheritance: Option<InheritanceClause>,
}

pub enum TypeBodyItem {
    Assignment(AttributeNode),
    FunctionDecl(FunctionDecl),
}

impl TypeDeclNode {
    pub fn new(name: LiteralNode, params: Vec<(LiteralNode, HulkType)>, attributes: Vec<AttributeNode>, methods: Vec<FunctionDecl>) -> Self {
        Self { name, params, attributes, methods, inheritance: None }
    }

    pub fn with_inheritance(name: LiteralNode, params: Vec<(LiteralNode, HulkType)>,attributes: Vec<AttributeNode>,methods: Vec<FunctionDecl>,inheritance: Option<InheritanceClause>) -> Self {
        Self { name, params,attributes,methods,inheritance }
    }
}
