use crate::codegen::{CodeGenerator, GeneratorResult};
use crate::expr_visitor::ExprVisitor;
use crate::nodes::binaryop_node::BinaryOp;
use crate::nodes::block_node::BlockNode;
use crate::nodes::destassing_node::DestAssignNode;
use crate::nodes::for_node::ForNode;
use crate::nodes::funcall_node::FunCallNode;
use crate::nodes::if_node::IfNode;
use crate::nodes::instantiation_node::InstantiationNode;
use crate::nodes::let_node::LetNode;
use crate::nodes::expr_node::{Expr, HulkType};
use crate::nodes::member_access_node::{MemberAccessNode, MethodCallNode};
use crate::nodes::tuple_node::{TupleAccessNode, TupleNode};
use crate::nodes::type_downcast_node::TypeDowncastNode;
use crate::nodes::type_test_node::TypeTestNode;
use crate::nodes::unaryop_node::UnaryOp;
use crate::nodes::while_node::WhileNode;
use crate::nodes::literal_node::Literal;

impl ExprVisitor<GeneratorResult> for CodeGenerator {

    fn visit_number(&mut self, n: f32) -> GeneratorResult {
         
        let text = n.to_string();
        let text = if text.contains('.') || text.contains('e') || text.contains('E') {
            text
        } else {
            format!("{}.0", text)
        };
        GeneratorResult::new(text, "double".to_string())
    }

    fn visit_bool(&mut self, b: bool) -> GeneratorResult {
        let val = if b { "1" } else { "0" };
        GeneratorResult::new(val.to_string(), "i1".to_string())
    }

    fn visit_binary_op(
        &mut self,
        left: &mut Expr,
        op: &BinaryOp,
        right: & mut Expr,
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
            BinaryOp::Equal => return self.emit_equality(&l, &r, true),
            BinaryOp::Dist  => return self.emit_equality(&l, &r, false),
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

    fn visit_block(&mut self, node: &mut BlockNode) -> GeneratorResult {
        let mut last_res = GeneratorResult::new("0".to_string(), "double".to_string());
        self.push_scope();
        for expr in &mut node.expressions {
            last_res = expr.accept(self);
        }
        self.pop_scope();
        last_res
    }

    fn visit_while(&mut self, node: &mut WhileNode) -> GeneratorResult {
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

   fn visit_if(&mut self, node: &mut IfNode) -> GeneratorResult {
        let merge_label = self.next_label("if_merge");
        // (registro_valor, etiqueta_bloque) de cada rama que cae a merge.
        let mut incoming: Vec<(String, String)> = Vec::new();
        let mut phi_ty = String::from("double");

        // ---- rama if ----
        let then_label = self.next_label("if_then");
        let mut next_label = self.next_label("if_else");
        let cond = node.condition.accept(self);
        self.emit(format!("br i1 {}, label %{}, label %{}", cond.register, then_label, next_label));

        self.emit_label(then_label);
        let then_res = node.if_branch.accept(self);
        phi_ty = then_res.llvm_type.clone();
        let then_blk = self.last_block_label();
        self.emit(format!("br label %{}", merge_label));
        incoming.push((then_res.register, then_blk));

        // ---- ramas elif ----
        for (econd, ebody) in node.elif_branches.iter_mut() {
            self.emit_label(next_label.clone());
            let ethen = self.next_label("if_then");
            next_label = self.next_label("if_else");
            let ec = econd.accept(self);
            self.emit(format!("br i1 {}, label %{}, label %{}", ec.register, ethen, next_label));

            self.emit_label(ethen);
            let eres = ebody.accept(self);
            let eblk = self.last_block_label();
            self.emit(format!("br label %{}", merge_label));
            incoming.push((eres.register, eblk));
        }

        // ---- rama else ----
        self.emit_label(next_label);
        let else_res = node.else_branch.accept(self);
        let else_blk = self.last_block_label();
        self.emit(format!("br label %{}", merge_label));
        incoming.push((else_res.register, else_blk));

        // ---- merge + phi ----
        self.emit_label(merge_label);
        let res_reg = self.next_temp();
        let phi_args: Vec<String> = incoming.iter()
            .map(|(reg, blk)| format!("[ {}, %{} ]", reg, blk))
            .collect();
        self.emit(format!("{} = phi {} {}", res_reg, phi_ty, phi_args.join(", ")));
        GeneratorResult::new(res_reg, phi_ty)
    }

    fn visit_let(&mut self, node: &mut LetNode) -> GeneratorResult {
        self.push_scope();
        for ((name_node, hulk_type), expr) in &mut node.assignments {
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
        if id == "PI" {
            return GeneratorResult::new("3.141592653589793".to_string(), "double".to_string());
        }
        if id == "E" {
            return GeneratorResult::new("2.718281828459045".to_string(), "double".to_string());
        }
        
        if let Some((ptr, ty)) = self.resolve_variable(id) {
            let res_reg = self.next_temp();
            match ty.as_str() {
                "double" | "i1" => self.emit(format!("{} = load {}, ptr {}", res_reg, ty, ptr)),
                
                t if CodeGenerator::is_tuple_llvm_type(t) => {
                    self.emit(format!("{} = load {}, ptr {}", res_reg, ty, ptr))
                }
                _ => self.emit(format!("{} = load ptr, ptr {}", res_reg, ptr)),
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

    fn visit_dest_assign(&mut self, node: &mut DestAssignNode) -> GeneratorResult {
        let val = node.expr.accept(self);

        match &mut node.target.as_mut() {
            Expr::Literal(lit_node) => {
                if let Literal::Id(name) = &lit_node.value {
                   if let Some((ptr, ty)) = self.resolve_variable(name) {
                        let store_ty = CodeGenerator::llvm_operand_type(&ty);
                        self.emit(format!("store {} {}, ptr {}", store_ty, val.register, ptr));
                    } else {
                        self.emit(format!("; ERROR: variable '{}' no declarada en dest-assign", name));
                    }
                }
                val
            }
            Expr::MemberAccess(access) => {
                let inst_res = access.instance.accept(self);
                if let Literal::Id(field_name) = &access.member.value {
                    let mut class = inst_res.llvm_type.trim_start_matches('%').to_string();
                    if !self.class_meta.contains_key(&class) {
                        if let HulkType::Class(tn) = access.instance.static_type() {
                            class = tn;
                        } else if matches!(access.instance.as_ref(), Expr::SelfRef) {
                            if let Some(t) = self.current_type_context.clone() { class = t; }
                        }
                    }
                    let class_ty = format!("%{}", class);

                    let field_ptr = self.next_temp();
                    let index = self.get_field_index(&class_ty, field_name);
                    self.emit(format!(
                        "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {} ; campo {}",
                        field_ptr, class_ty, inst_res.register, index, field_name
                    ));
                    let store_ty = CodeGenerator::llvm_operand_type(&val.llvm_type);
                    self.emit(format!("store {} {}, ptr {}", store_ty, val.register, field_ptr));
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

    fn visit_fun_call(&mut self, node: &mut FunCallNode) -> GeneratorResult {
        let mut args = Vec::new();
        for arg_expr in &mut node.args {
            args.push(arg_expr.accept(self));
        }

        if let Literal::Id(name) = &node.name.value {
            // 1. ¿Es una función built-in (print, math, etc)?
            if let Some(builtin_fn) = self.builtins.get(name).copied() {
                return builtin_fn(self, &args);
            }

            let return_type = CodeGenerator::hulk_type_to_llvm(&node.return_type);
            let res_reg = self.next_temp();
            let arg_strings: Vec<String> = args.iter()
                .map(|a| format!("{} {}", CodeGenerator::llvm_operand_type(&a.llvm_type), a.register))
                .collect();
             
            self.emit(format!("{} = call {} @{}({})", res_reg, return_type, name, arg_strings.join(", ")));
            return GeneratorResult::new(res_reg, return_type);
        }

        GeneratorResult::new("0.0".to_string(), "double".to_string())
    }

     fn visit_for(&mut self, node: &mut ForNode) -> GeneratorResult {
         
        let (start_res, end_res) = match &mut node.iterator.as_mut() {
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

    fn visit_unary_op(&mut self, op: &UnaryOp, expr: &mut Expr) -> GeneratorResult {
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
        node: &mut InstantiationNode,
    ) -> GeneratorResult {
        let mut args = Vec::new();
        for arg_expr in &mut node.args {
            args.push(arg_expr.accept(self));
        }

        if let Literal::Id(type_name) = &node.name.value {
            let res_reg = self.next_temp();
            let arg_strings: Vec<String> = args.iter()
                .map(|a| if a.llvm_type == "double" || a.llvm_type == "i1"
                        || CodeGenerator::is_tuple_llvm_type(&a.llvm_type) {
                    
                    format!("{} {}", a.llvm_type, a.register)
                } else {
                    format!("ptr {}", a.register)
                })
                .collect();
            self.emit(format!(
                "{} = call ptr @{}_new({})",
                res_reg, type_name, arg_strings.join(", ")
            ));
            return GeneratorResult::new(res_reg, type_name.to_string());
        }
        GeneratorResult::new("null".to_string(), "ptr".to_string())
    }

     
       fn visit_member_access(
        &mut self,
        node: &mut MemberAccessNode,
    ) -> GeneratorResult {
        let inst_res = node.instance.accept(self);
 
        if let Literal::Id(field_name) = &node.member.value {
             
            let mut class = inst_res.llvm_type.trim_start_matches('%').to_string();
            if !self.class_meta.contains_key(&class) {
                if let crate::nodes::expr_node::HulkType::Class(tn) = node.instance.static_type() {
                    class = tn;
                } else if matches!(node.instance.as_ref(), Expr::SelfRef) {
                    if let Some(t) = self.current_type_context.clone() { class = t; }
                }
            }
 
            // Si aún no es una clase conocida, no podemos generar un GEP válido.
            if !self.class_meta.contains_key(&class) {
                self.emit(format!(
                    "; ERROR: receptor de '.{}' sin clase resuelta (llvm_type={})",
                    field_name, inst_res.llvm_type
                ));
                self.emit("call void @abort()".to_string());
                return GeneratorResult::new("0.0".to_string(), "double".to_string());
            }
 
            let class_ty = format!("%{}", class);
            // get_field_index ya suma 1 para saltar el vptr y tiene fallback seguro.
           let field_index = self.get_field_index(&class_ty, field_name);
            let field_ptr   = self.next_temp();
            let field_val   = self.next_temp();

            // Tipo de ALMACENAMIENTO del campo en el struct (lo que se carga):
            // "double", "i1", "ptr" (clases) o "%Tuple_...". Fallback: "double".
            let storage_ty = self
                .collect_all_fields(&class)
                .into_iter()
                .find(|(n, _)| n == field_name)
                .map(|(_, ty)| ty)
                .unwrap_or_else(|| "double".to_string());
 
            let result_ty = match node.return_type {
                HulkType::Class(ref name) => name.clone(),
                _ => storage_ty.clone(),
            };

            self.emit(format!(
                "{} = getelementptr inbounds %{}, ptr {}, i32 0, i32 {}",
                field_ptr, class, inst_res.register, field_index
            ));
            self.emit(format!("{} = load {} , ptr {}", field_val, storage_ty, field_ptr));
            return GeneratorResult::new(field_val, result_ty);
        }
        GeneratorResult::new("0.0".to_string(), "double".to_string())
    }

    
    fn visit_method_call(
        &mut self,
        node: &mut MethodCallNode,
    ) -> GeneratorResult {
        // ---- 0.  Evaluar receiver y argumentos ----------------------------
        let inst_res = node.instance.accept(self);

        let mut arg_results = Vec::new();
        for arg_expr in &mut node.call.args {
            arg_results.push(arg_expr.accept(self));
        }

        if let Literal::Id(method_name) = &node.call.name.value { 
            let mut type_name_raw = inst_res.llvm_type.trim_start_matches('%').to_string();
            if !self.class_meta.contains_key(&type_name_raw) {
                if let HulkType::Class(tn) = node.instance.static_type() {
                    type_name_raw = tn;
                } else if matches!(node.instance.as_ref(), Expr::SelfRef) {
                    if let Some(t) = self.current_type_context.clone() {
                        type_name_raw = t;
                    }
                }
            }

    let slot_index = self
        .get_vtable_slot_index(&type_name_raw, method_name)
        .unwrap_or_else(|| 0);

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
            let res_reg = self.next_temp();
            let return_type = self.get_vtable_slot(&type_name_raw, slot_index)
                .map(|s| s.return_type.clone())
                .unwrap_or_else(|| CodeGenerator::hulk_type_to_llvm(&node.return_type));

           let mut all_args = vec![format!("ptr {}", inst_res.register)];
            all_args.extend(arg_results.iter().map(|a| {
                format!("{} {}", CodeGenerator::llvm_operand_type(&a.llvm_type), a.register)
            }));

            self.emit(format!(
                "{} = call {} {}({})",
                res_reg, return_type, fn_ptr, all_args.join(", ")
            ));

            return GeneratorResult::new(res_reg, return_type);
        }

        GeneratorResult::new("0.0".to_string(), "double".to_string())
    }
    
    fn visit_base_call(&mut self, args: &mut [ Expr]) -> GeneratorResult {
        // Evaluar argumentos antes de cualquier emit para no intercalar instrucciones.
        let mut arg_results = Vec::new();
        for arg in args {
            arg_results.push(arg.accept(self));
        }

        // Recuperar contexto: ¿en qué clase y en qué método estamos?
        let type_name = match self.current_type_context.clone() {
            Some(t) => t,
            None => {
                self.emit("; ERROR: base() usado fuera de contexto de tipo".to_string());
                return GeneratorResult::new("0.0".to_string(), "double".to_string());
            }
        };
        let method_name = match self.current_method_context.clone() {
            Some(m) => m,
            None => {
                //self.emit("; ERROR: base() usado fuera de un método".to_string());
                return GeneratorResult::new("0.0".to_string(), "double".to_string());
            }
        };
 
        // Buscar el ancestro más cercano con implementación propia.
        let parent_fn = match self.resolve_parent_method(&type_name, &method_name) {
            Some(f) => f,
            None => {
                 
                return GeneratorResult::new("0.0".to_string(), "double".to_string());
            }
        };
 
        // self siempre es el primer argumento.
        let self_reg = match self.resolve_variable("%self") {
            Some((ptr, _)) => ptr,
            None => "%self".to_string(),
        };
        
        let mut all_args = vec![format!("ptr {}", self_reg)];
        all_args.extend(
            arg_results.iter().map(|a| format!("{} {}", CodeGenerator::llvm_operand_type(&a.llvm_type), a.register))
        );

        let slot_index = self
                .get_vtable_slot_index(&type_name, &method_name)
                .unwrap_or_else(|| {0 });

         let return_type = self.get_vtable_slot(&type_name, slot_index)
    .map(|s| s.return_type.clone())
    .unwrap_or_else(|| "ptr".to_string());
                
 
        let res_reg = self.next_temp();
        
        self.emit(format!(
            "{} = call {} {}({})",
            res_reg, return_type, parent_fn, all_args.join(", ")
        ));
 
        GeneratorResult::new(res_reg, return_type)
    }
    
    fn visit_type_downcast(&mut self, node: &mut TypeDowncastNode) -> GeneratorResult {
        let expr_res    = node.expr.accept(self);
        let target_name = node.target_type.value.as_id();
 
        // ---- Load vptr ---------------------------------------------------
        let vptr_field = self.next_temp();
        let vptr       = self.next_temp();
 
        self.emit(format!("; downcast: {} as {}", expr_res.register, target_name));
        self.emit(format!(
            "{} = getelementptr inbounds %{}, ptr {}, i32 0, i32 0",
            vptr_field, expr_res.llvm_type, expr_res.register
        ));
        self.emit(format!("{} = load ptr, ptr {}", vptr, vptr_field));
 
        // ---- Conformance check (same logic as `is`) ----------------------
        let subtypes = self.collect_subtypes(&target_name);
 
        let is_ok_reg = if subtypes.is_empty() {
            let r = self.next_temp();
            self.emit(format!("{} = add i1 0, 0  ; unknown target, always false", r));
            r
        } else {
            let mut cmp_regs: Vec<String> = Vec::new();
            for subtype in &subtypes {
                let cmp_reg = self.next_temp();
                self.emit(format!(
                    "{} = icmp eq ptr {}, @vtable_{}",
                    cmp_reg, vptr, subtype
                ));
                cmp_regs.push(cmp_reg);
            }
            cmp_regs[1..].iter().fold(cmp_regs[0].clone(), |acc, cmp| {
                let or_reg = self.next_temp();
                self.emit(format!("{} = or i1 {}, {}", or_reg, acc, cmp));
                or_reg
            })
        };
 
        // ---- Branch: ok → continue; fail → runtime error ----------------
        let ok_label   = self.next_label("cast_ok");
        let fail_label = self.next_label("cast_fail");
 
        self.emit(format!(
            "br i1 {}, label %{}, label %{}",
            is_ok_reg, ok_label, fail_label
        ));
 
        // cast_fail block
        self.emit_label(fail_label);
        self.emit("call void @hulk_cast_error()".to_string());
        self.emit("unreachable".to_string());
 
        // cast_ok block — pointer is valid, return it with target static type
        self.emit_label(ok_label);
 
        GeneratorResult::new(expr_res.register, target_name)
    }
    
    fn visit_type_test(&mut self, node: &mut TypeTestNode) -> GeneratorResult {
        // Evaluate the expression being tested.
        let expr_res = node.expr.accept(self);
        let target_name = node.target_type.value.as_id();
 
        // ---- Load vptr from the object ------------------------------------
        let vptr_field = self.next_temp();
        let vptr       = self.next_temp();
 
        self.emit(format!("; type test: {} is {}", expr_res.register, target_name));
        self.emit(format!(
            "{} = getelementptr inbounds %{}, ptr {}, i32 0, i32 0",
            vptr_field, expr_res.llvm_type, expr_res.register
        ));
        self.emit(format!("{} = load ptr, ptr {}", vptr, vptr_field));
 
        // ---- Collect all subtypes of target (including itself) ------------
        let subtypes = self.collect_subtypes(&target_name);
 
        if subtypes.is_empty() {
            // Target type not in class_meta: always false at runtime.
            let false_reg = self.next_temp();
            self.emit(format!("{} = add i1 0, 0  ; unknown target, always false", false_reg));
            return GeneratorResult::new(false_reg, "i1".to_string());
        }
 
        // ---- Compare vptr against each subtype's vtable ------------------
        let mut cmp_regs: Vec<String> = Vec::new();
        for subtype in &subtypes {
            let cmp_reg = self.next_temp();
            self.emit(format!(
                "{} = icmp eq ptr {}, @vtable_{}",
                cmp_reg, vptr, subtype
            ));
            cmp_regs.push(cmp_reg);
        }
 
        // ---- OR all comparisons together ---------------------------------
        let result_reg = cmp_regs[1..].iter().fold(cmp_regs[0].clone(), |acc, cmp| {
            let or_reg = self.next_temp();
            self.emit(format!("{} = or i1 {}, {}", or_reg, acc, cmp));
            or_reg
        });
 
        GeneratorResult::new(result_reg, "i1".to_string())
    }
    
    fn visit_tuple(&mut self, node: &mut TupleNode) -> GeneratorResult {
        // Evaluar elementos primero (orden de evaluación izquierda-derecha).
        let mut elem_results: Vec<GeneratorResult> = Vec::new();
        for elem in &mut node.elements {
            elem_results.push(elem.accept(self));
        } 
        let elem_hulk_types: Vec<HulkType> = if let HulkType::Tuple(ref types) = node.return_type {
            types.clone()
        } else {
            // Fallback defensivo: reconstruir desde los resultados LLVM.
            elem_results.iter().map(|r| match r.llvm_type.as_str() {
                "double" => HulkType::Number,
                "i1"     => HulkType::Bool,
                "ptr"    => HulkType::String,
                other if other.starts_with('%') => HulkType::Class(other.trim_start_matches('%').to_string()),
                _ => HulkType::Unknown,
            }).collect()
        };
 
        let struct_name = self.ensure_tuple_type_emitted(&elem_hulk_types);
        let llvm_struct_ty = format!("%{}", struct_name);
 
        let tuple_ptr = self.next_temp();
        self.emit(format!("{} = alloca {}", tuple_ptr, llvm_struct_ty));
 
        for (i, val) in elem_results.iter().enumerate() {
            let slot_ptr = self.next_temp();
            self.emit(format!(
                "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {} ; elemento {}",
                slot_ptr, llvm_struct_ty, tuple_ptr, i, i
            ));
            self.emit(format!("store {} {}, ptr {}", val.llvm_type, val.register, slot_ptr));
        }
  
        let tuple_val = self.next_temp();
        self.emit(format!("{} = load {}, ptr {}", tuple_val, llvm_struct_ty, tuple_ptr));
 
        GeneratorResult::new(tuple_val, llvm_struct_ty)
    }
    
    fn visit_tuple_access(&mut self, node: &mut TupleAccessNode) -> GeneratorResult {
        let tuple_res = node.tuple.accept(self);
 
        let elem_llvm_ty = CodeGenerator::hulk_type_to_llvm(&node.return_type);
 
         
        let tmp_ptr = self.next_temp();
        self.emit(format!("{} = alloca {}", tmp_ptr, tuple_res.llvm_type));
        self.emit(format!(
            "store {} {}, ptr {}",
            tuple_res.llvm_type, tuple_res.register, tmp_ptr
        ));
 
        let slot_ptr = self.next_temp();
        self.emit(format!(
            "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {} ; acceso .{}",
            slot_ptr, tuple_res.llvm_type, tmp_ptr, node.index, node.index
        ));
 
        let val_reg = self.next_temp();
        self.emit(format!("{} = load {}, ptr {}", val_reg, elem_llvm_ty, slot_ptr));
 
        GeneratorResult::new(val_reg, elem_llvm_ty)
    }
}