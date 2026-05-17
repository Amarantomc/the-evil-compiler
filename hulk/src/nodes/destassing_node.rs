use crate::nodes::typedexpr_node::TypedExpr;


#[derive(Debug)]
pub struct DestAssignNode {
    pub target: Box<TypedExpr>,
    pub expr: Box<TypedExpr>,
}

impl DestAssignNode {
    pub fn new(target: TypedExpr, expr: TypedExpr) -> Self {
        DestAssignNode {
            target: Box::new(target),
            expr: Box::new(expr),
        }
    }
}