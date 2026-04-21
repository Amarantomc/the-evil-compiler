use crate::ast::{
    BinaryOp, BlockNode, DestAssignNode, Expr, ForNode, FunCallNode, IdentifierNode, IfNode,
    LetNode, Literal, UnaryOp, WhileNode,
};

pub trait ExprVisitor<T> {
    fn visit_number(&mut self, n: f32) -> T;
    fn visit_bool(&mut self, b: bool) -> T;
    fn visit_string(&mut self, s: &str) -> T;
    fn visit_binary_op(&mut self, left: &Expr, op: &BinaryOp, right: &Expr) -> T;
    fn visit_unary_op(&mut self, op: &UnaryOp, expr: &Expr) -> T;
    fn visit_let(&mut self, node: &LetNode) -> T;
    fn visit_if(&mut self, node: &IfNode) -> T;
    fn visit_while(&mut self, node: &WhileNode) -> T;
    fn visit_for(&mut self, node: &ForNode) -> T;
    fn visit_fun_call(&mut self, node: &FunCallNode) -> T;
    fn visit_dest_assign(&mut self, node: &DestAssignNode) -> T;
    fn visit_identifier(&mut self, node: &IdentifierNode) -> T;
    fn visit_block(&mut self, node: &BlockNode) -> T;
}
