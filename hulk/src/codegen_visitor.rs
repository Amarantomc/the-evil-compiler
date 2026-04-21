use crate::ast::{Expr, BinaryOp};
use crate::codegen::CodeGenerator;
use crate::expr_visitor::ExprVisitor;
use inkwell::IntPredicate;
use inkwell::values::IntValue;

impl<'ctx> ExprVisitor<IntValue<'ctx>> for CodeGenerator<'ctx> {
    fn visit_number(&mut self, n: f32) -> IntValue<'ctx> {
        self.context.i32_type().const_int(n as u64, false)
    }

    fn visit_binary_op(&mut self, left: &Expr, op: &BinaryOp, right: &Expr) -> IntValue<'ctx> {
        let left_val = left.accept(self);
        let right_val = right.accept(self);

        match op {
            BinaryOp::Add => self
                .builder
                .build_int_add(left_val, right_val, "add")
                .unwrap(),
            BinaryOp::Sub => self
                .builder
                .build_int_sub(left_val, right_val, "sub")
                .unwrap(),
            BinaryOp::Mul => self
                .builder
                .build_int_mul(left_val, right_val, "mul")
                .unwrap(),
            BinaryOp::Div => self
                .builder
                .build_int_signed_div(left_val, right_val, "div")
                .unwrap(),

            // Comparaciones
            BinaryOp::Equal => {
                let cmp = self
                    .builder
                    .build_int_compare(IntPredicate::EQ, left_val, right_val, "eq")
                    .unwrap();
                self.builder
                    .build_int_z_extend(cmp, self.context.i32_type(), "eq_ext")
                    .unwrap()
            }
            BinaryOp::Great => {
                let cmp = self
                    .builder
                    .build_int_compare(IntPredicate::SGT, left_val, right_val, "gt")
                    .unwrap();
                self.builder
                    .build_int_z_extend(cmp, self.context.i32_type(), "gt_ext")
                    .unwrap()
            }
            BinaryOp::Less => {
                let cmp = self
                    .builder
                    .build_int_compare(IntPredicate::SLT, left_val, right_val, "lt")
                    .unwrap();
                self.builder
                    .build_int_z_extend(cmp, self.context.i32_type(), "lt_ext")
                    .unwrap()
            }
            BinaryOp::Gequa => {
                let cmp = self
                    .builder
                    .build_int_compare(IntPredicate::SGE, left_val, right_val, "ge")
                    .unwrap();
                self.builder
                    .build_int_z_extend(cmp, self.context.i32_type(), "ge_ext")
                    .unwrap()
            }
            BinaryOp::Lequa => {
                let cmp = self
                    .builder
                    .build_int_compare(IntPredicate::SLE, left_val, right_val, "le")
                    .unwrap();
                self.builder
                    .build_int_z_extend(cmp, self.context.i32_type(), "le_ext")
                    .unwrap()
            }

            // Distancia: valor absoluto
            BinaryOp::Dist => {
                let sub = self
                    .builder
                    .build_int_sub(left_val, right_val, "dist_sub")
                    .unwrap();
                let zero = self.context.i32_type().const_int(0, false);

                let is_negative = self
                    .builder
                    .build_int_compare(IntPredicate::SLT, sub, zero, "is_neg")
                    .unwrap();

                let neg_sub = self.builder.build_int_sub(zero, sub, "neg_sub").unwrap();

                self.builder
                    .build_select(is_negative, neg_sub, sub, "dist")
                    .unwrap()
                    .into_int_value()
            }

            // No implementados o nuevos en BinaryOp
            BinaryOp::Pow => panic!("Pow no implementado"),
            BinaryOp::And => panic!("And no implementado"),
            BinaryOp::Or => panic!("Or no implementado"),
            BinaryOp::Mod => panic!("Mod no implementado"),
        }
    }
    
    fn visit_bool(&mut self, b: bool) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_string(&mut self, s: &str) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_unary_op(&mut self, op: &crate::ast::UnaryOp, expr: &Expr) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_let(&mut self, node: &crate::ast::LetNode) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_if(&mut self, node: &crate::ast::IfNode) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_while(&mut self, node: &crate::ast::WhileNode) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_for(&mut self, node: &crate::ast::ForNode) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_fun_call(&mut self, node: &crate::ast::FunCallNode) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_dest_assign(&mut self, node: &crate::ast::DestAssignNode) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_identifier(&mut self, node: &crate::ast::IdentifierNode) -> IntValue<'ctx> {
        todo!()
    }
    
    fn visit_block(&mut self, node: &crate::ast::BlockNode) -> IntValue<'ctx> {
        todo!()
    }
}
