use crate::nodes::{function_decl_node::FunctionDecl, typedexpr_node::TypedExpr};


#[derive(Debug)]
pub struct Program {
    pub statements: Vec<Statement>,
}

impl Program {
    pub fn new(statements: Vec<Statement>) -> Self {
        Program { statements }
    }
}

#[derive(Debug)]
pub enum Statement {
    FunctionDecl(FunctionDecl),
    Expression(TypedExpr),
}