use crate::nodes::{literal_node::LiteralNode, expr_node::{Expr, HulkType}, function_decl_node::FunctionDecl};

#[derive(Debug, Clone)]
pub struct AttributeNode {
    pub name: LiteralNode,
    pub type_annotation: HulkType,
    pub initializer: Expr,
     
}

impl AttributeNode {
    pub fn new(name: LiteralNode, type_annotation: HulkType, initializer: Expr) -> Self {
        Self { name, type_annotation, initializer }
    }
    
    pub fn set_type(&mut self, node_type: HulkType) {
        self.type_annotation = node_type;
    }
     
}

#[derive(Debug, Clone)]
pub struct InheritanceClause {
    pub parent_name: LiteralNode,
    pub parent_args: Option<Vec<Expr>>, // None = heredar argumentos implícitamente
}

impl InheritanceClause {
    pub fn new(parent_name: LiteralNode, parent_args: Option<Vec<Expr>>) -> Self {
        Self { parent_name, parent_args }
    }
}

#[derive(Debug, Clone)]
pub struct TypeDeclNode {
    pub name: LiteralNode,
    pub params: Vec<(LiteralNode, HulkType)>,
    pub attributes: Vec<AttributeNode>,
    pub methods: Vec<FunctionDecl>,
    pub inheritance: Option<InheritanceClause>,
    pub generics: Vec<String>, 
}

pub enum TypeBodyItem {
    Assignment(AttributeNode),
    FunctionDecl(FunctionDecl),
}

impl TypeDeclNode {
    pub fn new(name: LiteralNode, params: Vec<(LiteralNode, HulkType)>, attributes: Vec<AttributeNode>, methods: Vec<FunctionDecl>, generics: Vec<String>) -> Self {
        Self { name, params, attributes, methods, inheritance: None, generics }
    }

    pub fn with_inheritance(
        name: LiteralNode,
        generics: Vec<String>,
        params: Vec<(LiteralNode, HulkType)>,
        attributes: Vec<AttributeNode>,
        methods: Vec<FunctionDecl>,
        inheritance: Option<InheritanceClause>,
    ) -> Self {
        Self { name, generics, params, attributes, methods, inheritance }
    }

    pub fn is_generic(&self) -> bool { !self.generics.is_empty() }
}
