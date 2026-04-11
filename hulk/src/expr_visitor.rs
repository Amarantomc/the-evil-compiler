use crate::ast::{Expr, BinaryOp};
pub trait ExprVisitor<T> {
    fn visit_number(&mut self, n: f32) -> T;
    fn visit_binary_op(&mut self, left: &Expr, op: &BinaryOp, right: &Expr) -> T;
}
