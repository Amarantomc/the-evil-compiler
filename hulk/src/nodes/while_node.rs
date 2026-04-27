use crate::nodes::typedexpr_node::TypedExpr;


#[derive(Debug)]
pub struct WhileNode {
    pub condition: Box<TypedExpr>,
    pub body: Box<TypedExpr>,
}

impl WhileNode {
    pub fn new(condition: TypedExpr, body: TypedExpr) -> Self {
        WhileNode {
            condition: Box::new(condition),
            body: Box::new(body),
        }
    }
}