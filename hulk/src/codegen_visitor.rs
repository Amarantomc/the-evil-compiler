use crate::codegen::{CodeGenerator, GeneratorResult};
use crate::expr_visitor::ExprVisitor;
use crate::nodes::binaryop_node::BinaryOp;
use crate::nodes::block_node::BlockNode;
use crate::nodes::destassing_node::DestAssignNode;
use crate::nodes::for_node::ForNode;
use crate::nodes::funcall_node::FunCallNode;
use crate::nodes::if_node::IfNode;
use crate::nodes::let_node::LetNode;
use crate::nodes::typedexpr_node::{HulkType, TypedExpr};
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
        self.push_scope();
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
        self.push_scope();
        
        for ((name_node, _hulk_type), expr) in &node.assignments {
            let val = expr.accept(self);
            if let Literal::Id(name) = &name_node.value {
                let ptr = self.next_temp();
                match &_hulk_type {
                    HulkType::Class(class_name) => {
                        self.emit(format!("{} = alloca {}", ptr, "ptr"));
                        self.emit(format!("store {} {}, ptr {}", "ptr", val.register, ptr));
                        
                    }
                    _=> {
                        self.emit(format!("{} = alloca {}", ptr, val.llvm_type));
                self.emit(format!("store {} {}, ptr {}", val.llvm_type, val.register, ptr));
                    }
                }
                
                
                self.define_variable(name.clone(), ptr,val.llvm_type );
            }
        }
        
        let res = node.body.accept(self);
        print!("{:?}\n", self.scopes);
        self.pop_scope();
        res
    }

    fn visit_id(&mut self, id: &str) -> GeneratorResult {
        if let Some((ptr, ty)) = self.resolve_variable(id) {
            let res_reg = self.next_temp();
            match ty.as_str() {
                "double" | "i1" => self.emit(format!("{} = load {}, ptr {}", res_reg, ty, ptr)),
                _ => self.emit(format!("{} = load ptr, ptr {}", res_reg, ptr)),
                
            }
            //self.emit(format!("{} = load {}, ptr {}", res_reg, ty, ptr));
            GeneratorResult::new(res_reg, ty)
        } else {
            GeneratorResult::new("0.0".to_string(), "double".to_string())
        }
    }

    /// Carga el puntero de la instancia actual (`self`) desde la tabla de símbolos.
    /// En la convención adoptada, el generador almacena el puntero bajo la clave "%self".
    fn visit_self(&mut self) -> GeneratorResult {
        if let Some((ptr, ty)) = self.resolve_variable("%self") {
            // let res_reg = self.next_temp();
            // self.emit(format!("{} = load {}, ptr {}", res_reg, ty, ptr));
            GeneratorResult::new(ptr, ty)
        } else {
            // `self` fuera de un método: el semántico ya debería haber reportado error.
            self.emit("; ERROR: 'self' usado fuera de contexto de método".to_string());
            GeneratorResult::new("null".to_string(), "ptr".to_string())
        }
    }

    fn visit_dest_assign(&mut self, node: &DestAssignNode) -> GeneratorResult {
        use crate::nodes::typedexpr_node::Expr;

        let val = node.expr.accept(self);

        match &node.target.kind {
            // Asignación a un identificador local: x := expr
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
            // Asignación a atributo de instancia: self.attr := expr  o  inst.attr := expr
            Expr::MemberAccess(access) => {
                let inst_res = access.instance.accept(self);
                if let Literal::Id(field_name) = &access.member.value {
                    // Obtenemos el puntero al campo dentro del struct LLVM.
                    // Convención: el struct layout se conoce en tiempo de compilación;
                    // aquí emitimos un GEP canónico. El índice real se resuelve en una
                    // fase de layout (pendiente); usamos un comentario hasta entonces.
                    let field_ptr = self.next_temp();
                    self.emit(format!(
                        "; GEP para campo '{}' sobre instancia {}",
                        field_name, inst_res.register
                    ));
                    
                    let index=self.get_field_index(&inst_res.llvm_type, field_name);
                    self.emit(format!(
                        "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {} ; campo {}",
                        field_ptr,inst_res.llvm_type, inst_res.register, index,field_name
                    ));
                    self.emit(format!("store {} {}, ptr {}", val.llvm_type, val.register, field_ptr));
                }
                val
            }
            // self := ... está prohibido por la especificación; el semántico lo rechaza.
            Expr::SelfRef => {
                self.emit("; ERROR semántico: 'self' no es un target válido de asignación".to_string());
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
              // Verificamos si se pasó algún argumento
              if let Some(arg) = args.first() {
                let res_reg = self.next_temp();

                match arg.llvm_type.as_str() {
                    "double" => {
                        self.emit(format!("{} = call i32 (ptr, ...) @printf(ptr @.fmt_double, double {})", res_reg, arg.register));
                    },
                    "ptr" => {
                        // Asumimos que el puntero es una cadena de texto
                        self.emit(format!("{} = call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})", res_reg, arg.register));
                    },
                    "i1" => {
                        // Usamos `select` de LLVM para elegir la cadena "true" o "false" basada en el valor del booleano
                        let str_ptr = self.next_temp();
                        self.emit(format!("{} = select i1 {}, ptr @.str_true, ptr @.str_false", str_ptr, arg.register));
                        self.emit(format!("{} = call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})", res_reg, str_ptr));
                    },
                    _ => {
                        self.emit(format!("; ERROR: tipo no soportado para print: {}", arg.llvm_type));
                    }
                }
            }
            // Retornamos 0.0 por defecto para que las expresiones sigan funcionando
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
        let start_res = node.iterator.accept(self);
        
        let loop_cond = self.next_label("for_cond");
        let loop_body = self.next_label("for_body");
        let loop_end  = self.next_label("for_end");

        self.push_scope();
        if let Literal::Id(name) = &node.variable.value {
            let ptr = self.next_temp();
            self.emit(format!("{} = alloca double", ptr));
            self.emit(format!("store double {}, ptr {}", start_res.register, ptr));
            self.define_variable(name.clone(), ptr, "double".to_string());
        }

        self.emit_label(loop_cond.clone());
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
        // 1. Generamos un nombre único para la constante global
        let global_name = format!("@.str.{}", self.temp_counter);
        self.temp_counter += 1;

        // 2. Calculamos la longitud en bytes + 1 (por el terminador nulo \00)
        // Ojo: Esto asume ASCII/UTF-8 simple. Si 's' tiene caracteres de escape como \n
        // parseados literalmente, la longitud en LLVM podría variar, pero para empezar está perfecto.
        let len = s.len() + 1;

        // 3. Formateamos la constante global de LLVM
        let str_decl = format!(
            "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\"", 
            global_name, 
            len, 
            s
        );
        
        // 4. Lo guardamos en la nueva lista de declaraciones globales, no en el código local
        self.global_decls.push(str_decl);

        // 5. Retornamos el puntero a la cadena. Como usas opaque pointers en LLVM 15+, 'ptr' es ideal.
        GeneratorResult::new(global_name, "ptr".to_string())
    }
    
    /// Genera una llamada a `new <TypeName>(args)`.
    ///
    /// Convención de ABI adoptada:
    ///   - Cada tipo `T` genera una función `@T_new(args...) -> ptr` que
    ///     aloca el struct, inicializa los campos y devuelve el puntero.
    ///   - El codegen simplemente emite la llamada; la función la generará
    ///     la pasada de TypeDecl (pendiente de implementar).
    fn visit_instantiation(&mut self, node: &crate::nodes::instantiation_node::InstantiationNode) -> GeneratorResult {
        let mut args = Vec::new();
        for arg_expr in &node.args {
            args.push(arg_expr.accept(self));
        }

        if let Literal::Id(type_name) = &node.name.value {
            let res_reg = self.next_temp();
            let arg_strings: Vec<String> = args.iter()
                .map(|a| format!("{} {}", a.llvm_type, a.register))
                .collect();

            // Llamada a la función constructora generada para el tipo.
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
    /// Emite un `getelementptr` seguido de un `load`. El índice del campo
    /// dentro del struct se obtiene del `struct_layout` almacenado en el
    /// `CodeGenerator` (campo agregado junto a esta implementación).
    fn visit_member_access(&mut self, node: &crate::nodes::member_access_node::MemberAccessNode) -> GeneratorResult {
        let inst_res = node.instance.accept(self);
        print!("{:?}\n", inst_res);
        print!("{:?}\n", self.struct_layout);
        print!("{:?}\n", self.current_type_context);
        if let Literal::Id(field_name) = &node.member.value {
            let field_index = self.get_field_index(&inst_res.llvm_type, field_name);
            let field_ptr   = self.next_temp();
            let field_val   = self.next_temp();

            self.emit(format!(
                "{} = getelementptr inbounds {}, ptr {}, i32 0, i32 {}",
                field_ptr, inst_res.llvm_type, inst_res.register, field_index
            ));
            self.emit(format!(
                "{} = load double, ptr {}",
                field_val, field_ptr
            ));

            return GeneratorResult::new(field_val, "double".to_string());
        }

        GeneratorResult::new("0.0".to_string(), "double".to_string())
    }
    
    /// Emite una llamada a un método de instancia.
    ///
    /// Convención de ABI adoptada:
    ///   - El método `m` del tipo `T` se compila como `@T_m(ptr %self, args...) -> <ret>`.
    ///   - El `CodeGenerator` necesita el tipo de la instancia para resolver el nombre;
    ///     como en esta etapa el tipo vive en `inst_res.llvm_type` como `%T` (nombre LLVM
    ///     del struct), extraemos el nombre del tipo de ahí.
    fn visit_method_call(&mut self, node: &crate::nodes::member_access_node::MethodCallNode) -> GeneratorResult {
        // Evaluar la instancia (receiver)
        let inst_res = node.instance.accept(self);
        print!("{:?}\n", inst_res);
        // Evaluar argumentos
        let mut arg_results = Vec::new();
        for arg_expr in &node.call.args {
            arg_results.push(arg_expr.accept(self));
        }

        if let Literal::Id(method_name) = &node.call.name.value {
            let res_reg = self.next_temp();

            // Construir lista de argumentos: self primero, luego los demás
            let mut arg_strings = vec![format!("ptr {}", inst_res.register)];
            arg_strings.extend(arg_results.iter().map(|a| format!("{} {}", a.llvm_type, a.register)));

            // Derivar el nombre del tipo a partir del tipo LLVM de la instancia.
            // Si el tipo es "ptr" (objeto opaco) usamos el contexto del método.
            // La fase de layout completa reemplazará esta heurística.
            let type_name = inst_res.llvm_type;

            self.emit(format!(
                "{} = call double @{}_{}({})",
                res_reg, type_name, method_name, arg_strings.join(", ")
            ));

            return GeneratorResult::new(res_reg, "double".to_string());
        }

        GeneratorResult::new("0.0".to_string(), "double".to_string())
    }
}