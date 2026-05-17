use crate::codegen::{CodeGenerator, GeneratorResult};
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
use crate::nodes::literal_node::Literal;

impl ExprVisitor<GeneratorResult> for CodeGenerator {
    fn visit_number(&mut self, n: f32) -> GeneratorResult {
        GeneratorResult::new(n.to_string(), "double".to_string())
    }

    fn visit_bool(&mut self, b: bool) -> GeneratorResult {
        let val = if b { "1" } else { "0" };
        GeneratorResult::new(val.to_string(), "i1".to_string())
    }

    fn visit_binary_op(&mut self, left: &TypedExpr, op: &BinaryOp, right: &TypedExpr) -> GeneratorResult {
        let l = left.accept(self);
        let r = right.accept(self);
        let res_reg = self.next_temp();

        let (instr, res_type) = match op {
            BinaryOp::Add => (format!("{} = fadd double {}, {}", res_reg, l.register, r.register), "double"),
            BinaryOp::Sub => (format!("{} = fsub double {}, {}", res_reg, l.register, r.register), "double"),
            BinaryOp::Mul => (format!("{} = fmul double {}, {}", res_reg, l.register, r.register), "double"),
            BinaryOp::Div => (format!("{} = fdiv double {}, {}", res_reg, l.register, r.register), "double"),
            BinaryOp::Mod => {
                // LLVM usa frem para el resto de punto flotante
                (format!("{} = frem double {}, {}", res_reg, l.register, r.register), "double")
            },
            BinaryOp::Equal => (format!("{} = fcmp oeq double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Great => (format!("{} = fcmp ogt double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Less  => (format!("{} = fcmp olt double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Gequa => (format!("{} = fcmp oge double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Lequa => (format!("{} = fcmp ole double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Dist  => (format!("{} = fcmp une double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::And => (format!("{} = and i1 {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Or => (format!("{} = or i1 {}, {}", res_reg, l.register, r.register), "i1"),
            _ => (format!("; TODO: Implement {:?} binop", op), "double"),
        };

        self.emit(instr);
        GeneratorResult::new(res_reg, res_type.to_string())
    }

    fn visit_block(&mut self, node: &BlockNode) -> GeneratorResult {
        let mut last_res = GeneratorResult::new("0.0".to_string(), "double".to_string());
        self.push_scope(); // Un bloque también puede definir su propio scope
        for expr in &node.expressions {
            last_res = expr.accept(self);
        }
        self.pop_scope();
        last_res
    }

    fn visit_while(&mut self, node: &WhileNode) -> GeneratorResult {
        let cond_label = self.next_label("while_cond");
        let body_label = self.next_label("while_body");
        let end_label = self.next_label("while_end");
        
        let res_ptr = self.next_temp();
        self.emit(format!("{} = alloca double", res_ptr));
        self.emit(format!("store double 0.0, ptr {}", res_ptr));

        self.emit(format!("br label %{}", cond_label));
        self.emit_label(cond_label.clone());
        let cond = node.condition.accept(self);
        self.emit(format!("br i1 {}, label %{}", cond.register, body_label));
        self.emit(format!("br label %{}", end_label));

        self.emit_label(body_label);
        let body_res = node.body.accept(self);
        self.emit(format!("store double {}, ptr {}", body_res.register, res_ptr));
        self.emit(format!("br label %{}", cond_label));

        self.emit_label(end_label);
        let final_val = self.next_temp();
        self.emit(format!("{} = load double, ptr {}", final_val, res_ptr));
        
        GeneratorResult::new(final_val, "double".to_string())
    }

    fn visit_if(&mut self, node: &IfNode) -> GeneratorResult {
        let then_label = self.next_label("if_then");
        let else_label = self.next_label("if_else");
        let merge_label = self.next_label("if_merge");
        
        let cond = node.condition.accept(self);
        self.emit(format!("br i1 {}, label %{}, label %{}", cond.register, then_label, else_label));

        self.emit_label(then_label.clone());
        let then_res = node.if_branch.accept(self);
        let actual_then_block = self.last_block_label();
        self.emit(format!("br label %{}", merge_label));

        self.emit_label(else_label.clone());
        let else_res = node.else_branch.accept(self);
        let actual_else_block = self.last_block_label();
        self.emit(format!("br label %{}", merge_label));

        self.emit_label(merge_label);
        let res_reg = self.next_temp();
        self.emit(format!("{} = phi {} [ {}, %{} ], [ {}, %{} ]", 
            res_reg, then_res.llvm_type, 
            then_res.register, actual_then_block,
            else_res.register, actual_else_block
        ));

        GeneratorResult::new(res_reg, then_res.llvm_type)
    }

    fn visit_let(&mut self, node: &LetNode) -> GeneratorResult {
        self.push_scope(); // Nuevo scope para la expresión LET
        
        for ((name_node, _hulk_type), expr) in &node.assignments {
            let val = expr.accept(self);
            if let Literal::Id(name) = &name_node.value {
                // En LLVM manual, guardamos el valor en un puntero para permitir redifinición/shadowing simple
                let ptr = self.next_temp();
                self.emit(format!("{} = alloca {}", ptr, val.llvm_type));
                self.emit(format!("store {} {}, ptr {}", val.llvm_type, val.register, ptr));
                
                // Registramos el puntero en la tabla de símbolos
                self.define_variable(name.clone(), ptr, val.llvm_type);
            }
        }
        
        let res = node.body.accept(self);
        self.pop_scope();
        res
    }

    fn visit_id(&mut self, id: &str) -> GeneratorResult {
        if let Some((ptr, ty)) = self.resolve_variable(id) {
            let res_reg = self.next_temp();
            self.emit(format!("{} = load {}, ptr {}", res_reg, ty, ptr));
            GeneratorResult::new(res_reg, ty)
        } else {
            // Si no se encuentra, retornamos 0.0 (esto debería ser capturado por el semántico)
            GeneratorResult::new("0.0".to_string(), "double".to_string())
        }
    }

    fn visit_dest_assign(&mut self, node: &DestAssignNode) -> GeneratorResult {
        let val = node.expr.accept(self);
        // FIXME: Handle member access assignment like self.x := 5
        /*
        if let Literal::Id(name) = &node.identifier.value {
            if let Some((ptr, ty)) = self.resolve_variable(name) {
                // Destructive assignment: actualizamos el valor en la dirección de memoria existente
                self.emit(format!("store {} {}, ptr {}", ty, val.register, ptr));
                return val;
            }
        }
        */
        val
    }

    fn visit_fun_call(&mut self, node: &FunCallNode) -> GeneratorResult {
        let mut args = Vec::new();
        for arg_expr in &node.args {
            args.push(arg_expr.accept(self));
        }

        if let Literal::Id(name) = &node.name.value {
            // HULK builtin: print
            if name == "print" {
                // Implementación simple de print (requiere declaración externa de printf o similar)
                // Por ahora emitimos un comentario de placeholder
                self.emit(format!("; call to print with {:?}", args));
                return GeneratorResult::new("0.0".to_string(), "double".to_string());
            }

            let res_reg = self.next_temp();
            let arg_strings: Vec<String> = args.iter()
                .map(|a| format!("{} {}", a.llvm_type, a.register))
                .collect();
            
            self.emit(format!("{} = call double @{}({})", res_reg, name, arg_strings.join(", ")));
            return GeneratorResult::new(res_reg, "double".to_string());
        }
        
        GeneratorResult::new("0.0".to_string(), "double".to_string())
    }

    fn visit_for(&mut self, node: &ForNode) -> GeneratorResult {
        // Un bucle FOR en HULK suele ser: for (x in range(a, b)) body
        // Esto es azúcar sintáctico para un mientras o similar.
        // Implementación básica usando la variable de iteración.
        
        let start_res = node.iterator.accept(self); // Suponemos que retorna el inicio del rango o similar
        
        let loop_cond = self.next_label("for_cond");
        let loop_body = self.next_label("for_body");
        let loop_end = self.next_label("for_end");

        self.push_scope();
        if let Literal::Id(name) = &node.variable.value {
            let ptr = self.next_temp();
            self.emit(format!("{} = alloca double", ptr));
            self.emit(format!("store double {}, ptr {}", start_res.register, ptr));
            self.define_variable(name.clone(), ptr, "double".to_string());
        }

        self.emit_label(loop_cond.clone());
        // Aquí faltaría la lógica de comparación con el límite superior del range,
        // por simplicidad saltamos al cuerpo.
        self.emit(format!("br label %{}", loop_body));

        self.emit_label(loop_body);
        let body_res = node.body.accept(self);
        self.emit(format!("br label %{}", loop_cond));

        self.emit_label(loop_end);
        self.pop_scope();
        
        body_res
    }

    fn visit_unary_op(&mut self, op: &UnaryOp, expr: &TypedExpr) -> GeneratorResult {
        let val = expr.accept(self);
        let res_reg = self.next_temp();
        let (instr, res_ty) = match op {
            UnaryOp::Not => (format!("{} = xor i1 {}, 1", res_reg, val.register), "i1"),
            UnaryOp::Neg => (format!("{} = fneg double {}", res_reg, val.register), "double"),
            UnaryOp::Plus => (format!("{} = fadd double 0.0, {}", res_reg, val.register), "double"),
        };
        self.emit(instr);
        GeneratorResult::new(res_reg, res_ty.to_string())
    }

    fn visit_string(&mut self, s: &str) -> GeneratorResult {
        // Placeholder para strings (requiere manejo de constantes globales)
        self.emit(format!("; string literal: {:?}", s));
        GeneratorResult::new("null".to_string(), "ptr".to_string())
    }
    
    fn visit_instantiation(&mut self, node: &crate::nodes::instantiation_node::InstantiationNode) -> GeneratorResult {
        todo!()
    }
    
    fn visit_member_access(&mut self, node: &crate::nodes::member_access_node::MemberAccessNode) -> GeneratorResult {
        todo!()
    }
    
    fn visit_method_call(&mut self, node: &crate::nodes::member_access_node::MethodCallNode) -> GeneratorResult {
        todo!()
    }
}