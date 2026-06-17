use crate::nodes::{function_decl_node::FunctionDecl, expr_node::Expr, type_decl_node::TypeDeclNode};


#[derive(Debug, Clone)]
pub struct Program {
    pub statements: Vec<Statement>,
}

impl Program {
    pub fn new(statements: Vec<Statement>) -> Self {
        Program { statements }
    }
}

#[derive(Debug, Clone)]
pub enum Statement {
    FunctionDecl(FunctionDecl),
    TypeDecl(TypeDeclNode),
    Expression(Expr),
}