use crate::nodes::{typedexpr_node::TypedExpr, literal_node::LiteralNode, funcall_node::FunCallNode};

#[derive(Debug)]
pub struct MemberAccessNode {
    pub instance: Box<TypedExpr>,
    pub member: LiteralNode,
}

impl MemberAccessNode {
    pub fn new(instance: TypedExpr, member: LiteralNode) -> Self {
        Self {
            instance: Box::new(instance),
            member,
        }
    }
}

#[derive(Debug)]
pub struct MethodCallNode {
    pub instance: Box<TypedExpr>,
    pub call: FunCallNode,
}

impl MethodCallNode {
    pub fn new(instance: TypedExpr, call: FunCallNode) -> Self {
        Self {
            instance: Box::new(instance),
            call,
        }
    }
}
