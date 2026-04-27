use crate::nodes::typedexpr_node::TypedExpr;


#[derive(Debug)]
pub struct IfNode {
    pub condition: Box<TypedExpr>,
    pub if_branch: Box<TypedExpr>,
    pub elif_branches: Vec<(TypedExpr, TypedExpr)>,
    pub else_branch: Box<TypedExpr>,
}

impl IfNode {
    pub fn new(condition: TypedExpr, if_branch:TypedExpr, elif_branches: Vec<(TypedExpr, TypedExpr)>, else_branch: TypedExpr) -> Self {
        IfNode {
            condition: Box::new(condition),
            if_branch: Box::new(if_branch),
            elif_branches,
            else_branch: Box::new(else_branch),
        }
    }
}