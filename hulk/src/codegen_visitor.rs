use crate::codegen::{CodeGenerator, GeneratorResult};
use crate::expr_visitor::ExprVisitor;
use crate::nodes::binaryop_node::BinaryOp;
use crate::nodes::block_node::BlockNode;
use crate::nodes::destassing_node::DestAssignNode;
use crate::nodes::for_node::ForNode;
use crate::nodes::funcall_node::FunCallNode;
use crate::nodes::if_node::IfNode;
use crate::nodes::let_node::LetNode;
use crate::nodes::typedexpr_node::{Expr, HulkType, TypedExpr};
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

    fn visit_binary_op(
        &mut self,
        left: &TypedExpr,
        op: &BinaryOp,
        right: &TypedExpr,
    ) -> GeneratorResult {
        let l = left.accept(self);
        let r = right.accept(self);
        let res_reg = self.next_temp();

        let (instr, res_type) = match op {
            BinaryOp::Add  => (format!("{} = fadd double {}, {}", res_reg, l.register, r.register), "double"),
            BinaryOp::Sub  => (format!("{} = fsub double {}, {}", res_reg, l.register, r.register), "double"),
            BinaryOp::Mul  => (format!("{} = fmul double {}, {}", res_reg, l.register, r.register), "double"),
            BinaryOp::Div  => (format!("{} = fdiv double {}, {}", res_reg, l.register, r.register), "double"),
            BinaryOp::Mod  => (format!("{} = frem double {}, {}", res_reg, l.register, r.register), "double"),
            BinaryOp::Pow  => {
                // LLVM no tiene pow directo; usamos llvm.pow.f64 o una llamada a libm.
                (format!("{} = call double @llvm.pow.f64(double {}, double {})", res_reg, l.register, r.register), "double")
            }
            BinaryOp::Equal => (format!("{} = fcmp oeq double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Dist  => (format!("{} = fcmp une double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Great => (format!("{} = fcmp ogt double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Less  => (format!("{} = fcmp olt double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Gequa => (format!("{} = fcmp oge double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Lequa => (format!("{} = fcmp ole double {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::And   => (format!("{} = and i1 {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::Or    => (format!("{} = or i1 {}, {}", res_reg, l.register, r.register), "i1"),
            BinaryOp::SingleConc => return self.emit_single_concat(&l, &r),
            BinaryOp::SpacedConc => return self.emit_spaced_concat(&l, &r),
        };

        self.emit(instr);
        GeneratorResult::new(res_reg, res_type.to_string())
    }

    fn visit_block(&mut self, node: &BlockNode) -> GeneratorResult {
        let mut last_res = GeneratorResult::new("0.0".to_string(), "double".to_string());
        self.push_scope();
        for expr in &node.expressions {
            last_res = expr.accept(self);
        }
        self.pop_scope();
        last_res
    }

    fn visit_while(&mut self, node: &WhileNode) -> GeneratorResult {
        let cond_label  = self.next_label("while_cond");
        let body_label  = self.next_label("while_body");
        let end_label   = self.next_label("while_end");
 
        let res_ptr = self.next_temp();
        self.emit(format!("{} = alloca double", res_ptr));
        self.emit(format!("store double 0.0, ptr {}", res_ptr));
 
        self.emit(format!("br label %{}", cond_label));
        self.emit_label(cond_label.clone());
        let cond = node.condition.accept(self);
        self.emit(format!("br i1 {}, label %{}, label %{}", cond.register, body_label, end_label));
 
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
        let then_label  = self.next_label("if_then");
        let else_label  = self.next_label("if_else");
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
        self.emit(format!(
            "{} = phi {} [ {}, %{} ], [ {}, %{} ]",
            res_reg, then_res.llvm_type,
            then_res.register, actual_then_block,
            else_res.register, actual_else_block
        ));
        GeneratorResult::new(res_reg, then_res.llvm_type)
    }

    fn visit_let(&mut self, node: &LetNode) -> GeneratorResult {
        self.push_scope();
        for ((name_node, hulk_type), expr) in &node.assignments {
            let val = expr.accept(self);
            if let Literal::Id(name) = &name_node.value {
                let ptr = self.next_temp();
                match hulk_type {
                    HulkType::Class(_) => {
                        self.emit(format!("{} = alloca ptr", ptr));
                        self.emit(format!("store ptr {}, ptr {}", val.register, ptr));
                    }
                    _ => {
                        self.emit(format!("{} = alloca {}", ptr, val.llvm_type));
                        self.emit(format!("store {} {}, ptr {}", val.llvm_type, val.register, ptr));
                    }
                }
                self.define_variable(name.clone(), ptr, val.llvm_type.clone());
            }
        }
        let res = node.body.accept(self);
        self.pop_scope();
        res
    }

    fn visit_id(&mut self, id: &str) -> GeneratorResult {
        if let Some((ptr, ty)) = self.resolve_variable(id) {
            let res_reg = self.next_temp();
            match ty.as_str() {
                "double" | "i1" => self.emit(format!("{} = load {}, ptr {}", res_reg, ty, ptr)),
                _               => self.emit(format!("{} = load ptr, ptr {}", res_reg, ptr)),
            }
            GeneratorResult::new(res_reg, ty)
        } else {
            GeneratorResult::new("0.0".to_string(), "double".to_string())
        }
    }

    fn visit_self(&mut self) -> GeneratorResult {
        if let Some((ptr, ty)) = self.resolve_variable("%self") {
            GeneratorResult::new(ptr, ty)
        } else {
            self.emit("; ERROR: 'self' usado fuera de contexto de método".to_string());
            GeneratorResult::new("null".to_string(), "ptr".to_string())
        }
    }

    fn visit_dest_assign(&mut self, node: &DestAssignNode) -> GeneratorResult {
        let val = node.expr.accept(self);

        match &node.target.kind {
            Expr::Literal(lit_node) => {
                if let Literal::Id(name) = &lit_node.value {
                    if let Some((ptr, ty)) = self.resolve_variable(name) {
                        self.emit(format!("store {} {}, ptr {}", ty, val.register, ptr));
                    } else {
                        self.emit(format!("; ERROR: variable '{}' no declarada en dest-assign", name));
                    }
                }
                val
            }
            Expr::MemberAccess(access) => {
                let inst_res = access.instance.accept(self);
                if let Literal::Id(field_name) = &access.member.value {
                    let field_ptr = self.next_temp();
                    let index = self.get_field_index(&inst_res.llvm_type, field_name);
                    self.emit(format!(
                        "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {} ; campo {}",
                        field_ptr, inst_res.llvm_type, inst_res.register, index, field_name
                    ));
                    self.emit(format!("store {} {}, ptr {}", val.llvm_type, val.register, field_ptr));
                }
                val
            }
            Expr::SelfRef => {
                self.emit("; ERROR semántico: 'self' no es target válido de asignación".to_string());
                val
            }
            _ => {
                self.emit("; ERROR: target de asignación no válido".to_string());
                val
            }
        }
    }

    fn visit_fun_call(&mut self, node: &FunCallNode) -> GeneratorResult {
        let mut args = Vec::new();
        for arg_expr in &node.args {
            args.push(arg_expr.accept(self));
        }

        if let Literal::Id(name) = &node.name.value {
            if name == "print" {
                if let Some(arg) = args.first() {
                    let res_reg = self.next_temp();
                    match arg.llvm_type.as_str() {
                        "double" => {
                            self.emit(format!(
                                "{} = call i32 (ptr, ...) @printf(ptr @.fmt_double, double {})",
                                res_reg, arg.register
                            ));
                        }
                        "i1" => {
                            let str_ptr = self.next_temp();
                            self.emit(format!(
                                "{} = select i1 {}, ptr @.str_true, ptr @.str_false",
                                str_ptr, arg.register
                            ));
                            self.emit(format!(
                                "{} = call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})",
                                res_reg, str_ptr
                            ));
                        }
                        _ => {
                            self.emit(format!(
                                "{} = call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})",
                                res_reg, arg.register
                            ));
                        }
                    }

                }
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
        // El parser construye el iterador como FunCall("range", [start, end]).
        // No existe una funcion `range` real; extraemos los dos argumentos
        // directamente del nodo y generamos un loop con contador en IR.
        let (start_res, end_res) = match &node.iterator.kind {
            Expr::FunCall(call) => {
                if call.args.len() == 2 {
                    let s = call.args[0].accept(self);
                    let e = call.args[1].accept(self);
                    (s, e)
                } else {
                    let s = node.iterator.accept(self);
                    (s, GeneratorResult::new("0.0".to_string(), "double".to_string()))
                }
            }
            _ => {
                let s = node.iterator.accept(self);
                (s, GeneratorResult::new("0.0".to_string(), "double".to_string()))
            }
        };
 
        let loop_cond  = self.next_label("for_cond");
        let loop_body  = self.next_label("for_body");
        let loop_end   = self.next_label("for_end");
 
        // Resultado acumulado del loop (ultimo valor del cuerpo).
        let result_ptr = self.next_temp();
        self.emit(format!("{} = alloca double", result_ptr));
        self.emit(format!("store double 0.0, ptr {}", result_ptr));
 
        // Contador: inicializado con start.
        let counter_ptr = self.next_temp();
        self.emit(format!("{} = alloca double", counter_ptr));
        self.emit(format!("store double {}, ptr {}", start_res.register, counter_ptr));
 
        // Saltar al bloque de condicion (cierra el bloque entry / previo).
        self.emit(format!("br label %{}", loop_cond));
 
        // ---- bloque de condicion: counter < end --------------------------
        self.emit_label(loop_cond.clone());
        let cur  = self.next_temp();
        let cond = self.next_temp();
        self.emit(format!("{} = load double, ptr {}", cur, counter_ptr));
        self.emit(format!("{} = fcmp olt double {}, {}", cond, cur, end_res.register));
        self.emit(format!("br i1 {}, label %{}, label %{}", cond, loop_body, loop_end));
 
        // ---- bloque de cuerpo --------------------------------------------
        self.emit_label(loop_body.clone());
        self.push_scope();
 
        // Exponer la variable del for con el valor actual del contador.
        if let Literal::Id(var_name) = &node.variable.value {
            let var_ptr = self.next_temp();
            self.emit(format!("{} = alloca double", var_ptr));
            self.emit(format!("store double {}, ptr {}", cur, var_ptr));
            self.define_variable(var_name.clone(), var_ptr, "double".to_string());
        }
 
        let body_res = node.body.accept(self);
        self.emit(format!("store double {}, ptr {}", body_res.register, result_ptr));
 
        // Incrementar el contador en 1.
        let cur2     = self.next_temp();
        let next_val = self.next_temp();
        self.emit(format!("{} = load double, ptr {}", cur2, counter_ptr));
        self.emit(format!("{} = fadd double {}, 1.0", next_val, cur2));
        self.emit(format!("store double {}, ptr {}", next_val, counter_ptr));
 
        self.emit(format!("br label %{}", loop_cond));
        self.pop_scope();
 
        // ---- bloque de salida --------------------------------------------
        self.emit_label(loop_end.clone());
        let final_val = self.next_temp();
        self.emit(format!("{} = load double, ptr {}", final_val, result_ptr));
 
        GeneratorResult::new(final_val, "double".to_string())
    }

    fn visit_unary_op(&mut self, op: &UnaryOp, expr: &TypedExpr) -> GeneratorResult {
        let val = expr.accept(self);
        let res_reg = self.next_temp();
        let (instr, res_ty) = match op {
            UnaryOp::Not  => (format!("{} = xor i1 {}, 1", res_reg, val.register), "i1"),
            UnaryOp::Neg  => (format!("{} = fneg double {}", res_reg, val.register), "double"),
            UnaryOp::Plus => (format!("{} = fadd double 0.0, {}", res_reg, val.register), "double"),
        };
        self.emit(instr);
        GeneratorResult::new(res_reg, res_ty.to_string())
    }

    fn visit_string(&mut self, s: &str) -> GeneratorResult {
        let global_name = format!("@.str.{}", self.temp_counter);
        self.temp_counter += 1;
        let len = s.len() + 1;
        self.global_decls.push(format!(
            "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"",
            global_name, len, s
        ));
        GeneratorResult::new(global_name, "ptr".to_string())
    }

    fn visit_instantiation(
        &mut self,
        node: &crate::nodes::instantiation_node::InstantiationNode,
    ) -> GeneratorResult {
        let mut args = Vec::new();
        for arg_expr in &node.args {
            args.push(arg_expr.accept(self));
        }

        if let Literal::Id(type_name) = &node.name.value {
            let res_reg = self.next_temp();
            let arg_strings: Vec<String> = args.iter()
                .map(|a| format!("{} {}", a.llvm_type, a.register))
                .collect();
            self.emit(format!(
                "{} = call ptr @{}_new({})",
                res_reg, type_name, arg_strings.join(", ")
            ));
            return GeneratorResult::new(res_reg, type_name.to_string());
        }
        GeneratorResult::new("null".to_string(), "ptr".to_string())
    }

    /// Lee el valor de un atributo de una instancia.
    ///
    /// El índice del campo incluye +1 por el vptr en posición 0.
    fn visit_member_access(
        &mut self,
        node: &crate::nodes::member_access_node::MemberAccessNode,
    ) -> GeneratorResult {
        let inst_res = node.instance.accept(self);

        if let Literal::Id(field_name) = &node.member.value {
            // get_field_index ya suma 1 para saltar el vptr.
            let field_index = self.get_field_index(&inst_res.llvm_type, field_name);
            let field_ptr   = self.next_temp();
            let field_val   = self.next_temp();

            self.emit(format!(
                "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
                field_ptr, inst_res.llvm_type, inst_res.register, field_index
            ));
            self.emit(format!("{} = load double, ptr {}", field_val, field_ptr));
            return GeneratorResult::new(field_val, "double".to_string());
        }
        GeneratorResult::new("0.0".to_string(), "double".to_string())
    }

    /// Emite una llamada a un método de instancia usando despacho indirecto
    /// a través de la VTable.
    ///
    /// # Secuencia de instrucciones emitidas
    ///
    /// ```text
    ///   ; 1. Leer el vptr del objeto (campo 0 del struct)
    ///   %vptr_field = getelementptr inbounds %T, ptr %obj, i32 0, i32 0
    ///   %vptr       = load ptr, ptr %vptr_field
    ///
    ///   ; 2. Leer el function pointer del slot correcto de la VTable
    ///   %fn_slot    = getelementptr inbounds %VTable_T, ptr %vptr, i32 0, i32 <slot>
    ///   %fn_ptr     = load ptr, ptr %fn_slot
    ///
    ///   ; 3. Llamada indirecta (polimórfica)
    ///   %result     = call double %fn_ptr(ptr %obj, <args...>)
    /// ```
    ///
    /// # Por qué despacho indirecto y no una llamada directa
    ///
    /// Con una llamada directa (`call double @Animal_speak(...)`) el compilador
    /// fijaría en tiempo de compilación qué función se ejecuta.  Si el objeto
    /// es en realidad un `Dog` (hijo de `Animal`), se ejecutaría la versión
    /// equivocada.  El despacho a través del vptr lee la función *real* del
    /// objeto en tiempo de ejecución, que en el caso de un `Dog` apuntará a
    /// `@Dog_speak`.  Esto es exactamente lo que permite el polimorfismo.
    ///
    /// # Por qué necesitamos el tipo estático (`inst_res.llvm_type`)
    ///
    /// Solo usamos el tipo estático para:
    ///   a) Calcular el índice del slot en la VTable (invariante entre padre
    ///      e hijo gracias a build_vtable_for_class).
    ///   b) Emitir el GEP del vptr con el tipo LLVM correcto del struct.
    ///
    /// El tipo *dinámico* real se resuelve solo en tiempo de ejecución a
    /// través del contenido del vptr.
    fn visit_method_call(
        &mut self,
        node: &crate::nodes::member_access_node::MethodCallNode,
    ) -> GeneratorResult {
        // ---- 0.  Evaluar receiver y argumentos ----------------------------
        let inst_res = node.instance.accept(self);

        let mut arg_results = Vec::new();
        for arg_expr in &node.call.args {
            arg_results.push(arg_expr.accept(self));
        }

        if let Literal::Id(method_name) = &node.call.name.value {
            // ---- 1.  Resolver el slot de la vtable para este método -------
            //
            // Usamos el tipo *estático* de la instancia (conocido en compile
            // time) para encontrar el índice del slot.  Gracias a que
            // build_vtable_for_class preserva el orden del padre, este índice
            // es válido también cuando el tipo dinámico es un subtipo.
            let type_name_raw = inst_res.llvm_type.trim_start_matches('%').to_string();

            let slot_index = self
                .get_vtable_slot_index(&type_name_raw, method_name)
                .unwrap_or_else(|| {
                    // Fallback: si no encontramos el slot (error semántico ya reportado),
                    // usamos 0 para generar IR sintácticamente válido.
                    0
                });

            // ---- 2.  Leer vptr del objeto (campo 0) -----------------------
            let vptr_field = self.next_temp();
            let vptr       = self.next_temp();

            self.emit(format!(
                "; Despacho virtual: {}.{}()",
                type_name_raw, method_name
            ));
            self.emit(format!(
                "{} = getelementptr inbounds %{}, ptr {}, i32 0, i32 0  ; leer vptr",
                vptr_field, type_name_raw, inst_res.register
            ));
            self.emit(format!(
                "{} = load ptr, ptr {}",
                vptr, vptr_field
            ));

            // ---- 3.  Leer function pointer del slot de la VTable ----------
            let fn_slot = self.next_temp();
            let fn_ptr  = self.next_temp();

            self.emit(format!(
                "{} = getelementptr inbounds %VTable_{}, ptr {}, i32 0, i32 {}  ; slot {}",
                fn_slot, type_name_raw, vptr, slot_index, method_name
            ));
            self.emit(format!(
                "{} = load ptr, ptr {}",
                fn_ptr, fn_slot
            ));

            // ---- 4.  Llamada indirecta ------------------------------------
            //
            // El tipo de retorno se asume `double` (la convención del codegen
            // existente).  Una extensión futura puede leerlo del ClassMeta.
            let res_reg = self.next_temp();
            let mut all_args = vec![format!("ptr {}", inst_res.register)];
            all_args.extend(arg_results.iter().map(|a| format!("{} {}", a.llvm_type, a.register)));

            self.emit(format!(
                "{} = call double {}({})",
                res_reg, fn_ptr, all_args.join(", ")
            ));

            return GeneratorResult::new(res_reg, "double".to_string());
        }

        GeneratorResult::new("0.0".to_string(), "double".to_string())
    }
}