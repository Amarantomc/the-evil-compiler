use crate::ast::{Expr,Opcode};
pub trait ExprVisitor<T>{
    
    fn visit_number(&mut self, n: i32) -> T;
    fn visit_binary_op(&mut self, left: &Expr, op: &Opcode, right: &Expr) -> T;
}
