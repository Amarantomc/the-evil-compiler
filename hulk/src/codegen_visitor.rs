
use crate::codegen::CodeGenerator;
use crate::expr_visitor::ExprVisitor;
use crate::nodes::binaryop_node::BinaryOp;
use crate::nodes::block_node::BlockNode;
use crate::nodes::destassing_node::DestAssignNode;
use crate::nodes::for_node::ForNode;
use crate::nodes::funcall_node::FunCallNode;
use crate::nodes::if_node::IfNode;
use crate::nodes::let_node::LetNode;
use crate::nodes::typedexpr_node::TypedExpr;
use crate::nodes::unaryop_node::UnaryOp;
use crate::nodes::while_node::WhileNode;
use inkwell::values::{BasicValue, BasicValueEnum, IntValue};
use inkwell::IntPredicate;

impl<'ctx> ExprVisitor<BasicValueEnum<'ctx>> for CodeGenerator<'ctx> {
    fn visit_number(&mut self, n: f32) -> BasicValueEnum<'ctx> {
        self.context.i32_type().const_int(n as u64, false).into()
    }

    fn visit_binary_op(&mut self, left: &TypedExpr, op: &BinaryOp, right: &TypedExpr) -> BasicValueEnum<'ctx> {
        let left_val = left.accept(self).into_int_value();
        let right_val = right.accept(self).into_int_value();

        let result: IntValue = match op {
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
        };

        result.into()
    }

    fn visit_bool(&mut self, b: bool) -> BasicValueEnum<'ctx> {
        let val = if b { 1 } else { 0 };
        self.context.i32_type().const_int(val, false).into()
    }

    fn visit_string(&mut self, s: &str) -> BasicValueEnum<'ctx> {
        self.builder
            .build_global_string_ptr(s, "str")
            .unwrap()
            .as_basic_value_enum()
    }

    fn visit_unary_op(&mut self, op: &UnaryOp, expr: &TypedExpr) -> BasicValueEnum<'ctx> {
        let val = expr.accept(self).into_int_value();

        let result: IntValue = match op {
            UnaryOp::Plus => val,
            UnaryOp::Neg => self.builder.build_int_neg(val, "neg").unwrap(),
            UnaryOp::Not => {
                // Not lógico: si es 0 pasas a 1, si es != 0 pasas a 0.
                let is_zero = self
                    .builder
                    .build_int_compare(
                        IntPredicate::EQ,
                        val,
                        self.context.i32_type().const_int(0, false),
                        "is_zero",
                    )
                    .unwrap();
                self.builder
                    .build_int_z_extend(is_zero, self.context.i32_type(), "not_ext")
                    .unwrap()
            }
        };

        result.into()
    }

    fn visit_let(&mut self, _node: &LetNode) -> BasicValueEnum<'ctx> {
        todo!()
    }

    fn visit_if(&mut self, _node: &IfNode) -> BasicValueEnum<'ctx> {
        todo!()
    }

    fn visit_while(&mut self, node: &WhileNode) -> BasicValueEnum<'ctx> {
        
        //FALTA TESTEO POR FALTA DE LET
        let parent_func = self.builder.get_insert_block().unwrap().get_parent().unwrap();

        // 1. Crear bloques para la condición, el cuerpo y la salida
        let cond_bb = self.context.append_basic_block(parent_func, "while_cond");
        let body_bb = self.context.append_basic_block(parent_func, "while_body");
        let after_bb = self.context.append_basic_block(parent_func, "while_after");

        // 2. Variable para almacenar el resultado (valor de la última iteración)
        // Usamos un 'alloca' al inicio de la función (o en el bloque actual)
        let result_ptr = self.builder.build_alloca(self.context.i32_type(), "while_result").unwrap();
        // Inicializamos con 0 (o false) por si el bucle nunca se ejecuta
        self.builder.build_store(result_ptr, self.context.i32_type().const_int(0, false)).unwrap();

        // Salto inicial a la condición
        self.builder.build_unconditional_branch(cond_bb).unwrap();

        // 3. Bloque de la Condición
        self.builder.position_at_end(cond_bb);
        let cond_val = node.condition.accept(self).into_int_value();
        
        // El tipo Bool en HULK es i1 o i32(0/1). Si es i32, comparamos != 0
        let is_true = self.builder.build_int_compare(
            IntPredicate::NE, 
            cond_val, 
            self.context.i32_type().const_int(0, false), 
            "is_true"
        ).unwrap();

        self.builder.build_conditional_branch(is_true, body_bb, after_bb).unwrap();

        // 4. Bloque del Cuerpo
        self.builder.position_at_end(body_bb);
        let body_val = node.body.accept(self);
        
        // Guardamos el valor de esta iteración
        if body_val.is_int_value() {
            self.builder.build_store(result_ptr, body_val.into_int_value()).unwrap();
        }

        self.builder.build_unconditional_branch(cond_bb).unwrap();

        // 5. Bloque de Salida
        self.builder.position_at_end(after_bb);
        
        // Cargamos y retornamos el último valor guardado
        self.builder.build_load(self.context.i32_type(), result_ptr, "final_result").unwrap().as_basic_value_enum()
    }

    fn visit_for(&mut self, _node: &ForNode) -> BasicValueEnum<'ctx> {
        todo!()
    }

    fn visit_fun_call(&mut self, _node: &FunCallNode) -> BasicValueEnum<'ctx> {
        todo!()
    }

    fn visit_dest_assign(&mut self, _node: &DestAssignNode) -> BasicValueEnum<'ctx> {
        todo!()
    }

    fn visit_block(&mut self, node: &BlockNode) -> BasicValueEnum<'ctx> {
        let mut last_value = self.context.i32_type().const_int(0, false).as_basic_value_enum();

        for expr in &node.expressions {
            last_value = expr.accept(self);
        }

        last_value
    }
    
    fn visit_id(&mut self, id: &str) -> BasicValueEnum<'ctx> {
        todo!()
    }
}
