use crate::nodes::expr_node::{Expr, HulkType};


#[derive(Debug, Clone)]
pub struct IfNode {
    pub condition: Box<Expr>,
    pub if_branch: Box<Expr>,
    pub elif_branches: Vec<(Expr, Expr)>,
    pub else_branch: Box<Expr>,
    pub return_type: HulkType,
}

impl IfNode {
    pub fn new(condition: Expr, if_branch:Expr, elif_branches: Vec<(Expr, Expr)>, else_branch: Expr) -> Self {
        IfNode {
            condition: Box::new(condition),
            if_branch: Box::new(if_branch),
            elif_branches,
            else_branch: Box::new(else_branch),
            return_type: HulkType::Unknown
        }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}