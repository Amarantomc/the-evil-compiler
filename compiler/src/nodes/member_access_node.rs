use crate::nodes::{expr_node::{Expr, HulkType}, funcall_node::FunCallNode, literal_node::LiteralNode};

#[derive(Debug, Clone)]
pub struct MemberAccessNode {
    pub instance: Box<Expr>,
    pub member: LiteralNode,
    pub return_type: HulkType,
}

impl MemberAccessNode {
    pub fn new(instance: Expr, member: LiteralNode) -> Self {
        Self {
            instance: Box::new(instance),
            member,
            return_type:HulkType::Unknown,
        }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}

#[derive(Debug, Clone)]
pub struct MethodCallNode {
    pub instance: Box<Expr>,
    pub call: FunCallNode,
    pub return_type: HulkType,
}

impl MethodCallNode {
    pub fn new(instance: Expr, call: FunCallNode) -> Self {
        Self {
            instance: Box::new(instance),
            call,
            return_type:HulkType::Unknown,
        }
    }
    pub fn set_type(&mut self, node_type: HulkType) {
        self.return_type = node_type;
    }
}
