use crate::nodes::{binaryop_node::BinaryOp, block_node::BlockNode, destassing_node::DestAssignNode, expr_node::Expr, for_node::ForNode, funcall_node::FunCallNode, if_node::IfNode, instantiation_node::InstantiationNode, let_node::LetNode, member_access_node::{MemberAccessNode, MethodCallNode}, tuple_node::{TupleAccessNode, TupleNode}, type_downcast_node::TypeDowncastNode, type_test_node::TypeTestNode, unaryop_node::UnaryOp, while_node::WhileNode};

 

pub trait ExprVisitor<T> {
    fn visit_number(&mut self, n: f32) -> T;
    fn visit_bool(&mut self, b: bool) -> T;
    fn visit_string(&mut self, s: &str) -> T;
    fn visit_id(&mut self, id: &str) -> T;
    fn visit_binary_op(&mut self, left: &mut Expr, op: &BinaryOp, right: &mut Expr) -> T;
    fn visit_unary_op(&mut self, op: &UnaryOp, expr: &mut Expr) -> T;
    fn visit_let(&mut self, node: &mut LetNode) -> T;
    fn visit_if(&mut self, node: &mut IfNode) -> T;
    fn visit_while(&mut self, node: &mut WhileNode) -> T;
    fn visit_for(&mut self, node: &mut ForNode) -> T;
    fn visit_fun_call(&mut self, node: &mut FunCallNode) -> T;
    fn visit_dest_assign(&mut self, node: &mut DestAssignNode) -> T;
    fn visit_block(&mut self, node: &mut BlockNode) -> T;
    fn visit_instantiation(&mut self, node: &mut InstantiationNode) -> T;
    fn visit_member_access(&mut self, node: &mut MemberAccessNode) -> T;
    fn visit_method_call(&mut self, node: &mut MethodCallNode) -> T;
    fn visit_self(&mut self) -> T;
    fn visit_base_call(&mut self, args: &mut [Expr]) -> T;
    fn visit_type_downcast(&mut self, node: &mut TypeDowncastNode) -> T;
    fn visit_type_test(&mut self, node: &mut TypeTestNode) -> T;
    fn visit_tuple(&mut self, node: &mut TupleNode) -> T;
    fn visit_tuple_access(&mut self, node: &mut TupleAccessNode) -> T;
}
