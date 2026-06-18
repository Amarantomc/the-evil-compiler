use std::fs;
use std::collections::HashMap;
use crate::nodes::function_decl_node::FunctionDecl;
use crate::nodes::literal_node::Literal;
use crate::nodes::program_node::{Program, Statement};
use crate::nodes::type_decl_node::{AttributeNode, TypeDeclNode};
use crate::nodes::expr_node::{HulkType, Expr};
 

/// Resultado de compilar una expresión: registro LLVM + tipo LLVM.
#[derive(Debug, Clone)]
pub struct GeneratorResult {
    pub register: String,
    pub llvm_type: String,
}

impl GeneratorResult {
    pub fn new(register: String, llvm_type: String) -> Self {
        Self { register, llvm_type }
    }
}



/// Descripción de un slot de VTable: nombre del método + firma LLVM completa.

#[derive(Debug, Clone)]
pub struct VTableSlot {
    
    pub method_name: String,
     
    pub impl_fn: String,
     
    pub fn_ptr_llvm_type: String,
     
    pub return_type: String,
}

/// Información de una clase registrada en el CodeGenerator.
#[derive(Debug, Clone)]
pub struct ClassMeta {
     
    pub parent: Option<String>,
 
    pub ctor_params: Vec<(String, String)>,
     
    pub own_fields: Vec<(String, String)>,
    
    pub vtable: Vec<VTableSlot>,
}

 
pub type BuiltinFn = fn(&mut CodeGenerator, &[GeneratorResult]) -> GeneratorResult;
pub struct CodeGenerator {
     
    pub code: Vec<String>,
    pub temp_counter: usize,
    pub label_counter: usize,
     
    pub scopes: Vec<HashMap<String, (String, String)>>,
    
    pub struct_layout: HashMap<String, Vec<(String, String)>>,
     
    pub class_meta: HashMap<String, ClassMeta>,
     
    pub current_type_context: Option<String>,
    pub global_decls: Vec<String>,
    pub current_method_context: Option<String>,
    
    pub emitted_tuple_types: std::collections::HashSet<String>,
    pub builtins: HashMap<String, BuiltinFn>,
}

impl CodeGenerator {
   pub fn new() -> Self {
        let mut builtins: HashMap<String, BuiltinFn> = HashMap::new();

        // --- PRINT ---
        builtins.insert("print".to_string(), |cg, args| {
    match args.first() {
        Some(arg) => {
            match arg.llvm_type.as_str() {
                "double" => cg.emit(format!(
                    "call i32 (ptr, ...) @printf(ptr @.fmt_double, double {})", arg.register)),
                "i1" => {
                    let s = cg.next_temp();
                    cg.emit(format!("{} = select i1 {}, ptr @.str_true, ptr @.str_false", s, arg.register));
                    cg.emit(format!("call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})", s));
                }
                _ => cg.emit(format!(
                    "call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr {})", arg.register)),
            }
            // print devuelve su argumento: mismo %registro y mismo tipo LLVM
            arg.clone()
        }
        None => GeneratorResult::new("0".to_string(), "double".to_string()),
    }
});

        // --- MATEMÁTICAS SIMPLES ---
        builtins.insert("sqrt".to_string(), |cg, args| {
            let res = cg.next_temp();
            cg.emit(format!("{} = call double @sqrt(double {})", res, args[0].register));
            GeneratorResult::new(res, "double".to_string())
        });
        builtins.insert("sin".to_string(), |cg, args| {
            let res = cg.next_temp();
            cg.emit(format!("{} = call double @sin(double {})", res, args[0].register));
            GeneratorResult::new(res, "double".to_string())
        });
        builtins.insert("cos".to_string(), |cg, args| {
            let res = cg.next_temp();
            cg.emit(format!("{} = call double @cos(double {})", res, args[0].register));
            GeneratorResult::new(res, "double".to_string())
        });
        builtins.insert("exp".to_string(), |cg, args| {
            let res = cg.next_temp();
            cg.emit(format!("{} = call double @exp(double {})", res, args[0].register));
            GeneratorResult::new(res, "double".to_string())
        });

        // --- LOG (Cambio de base) ---
        builtins.insert("log".to_string(), |cg, args| {
            let log_val = cg.next_temp();
            let log_base = cg.next_temp();
            let res = cg.next_temp();
            cg.emit(format!("{} = call double @log(double {})", log_val, args[1].register));
            cg.emit(format!("{} = call double @log(double {})", log_base, args[0].register));
            cg.emit(format!("{} = fdiv double {}, {}", res, log_val, log_base));
            GeneratorResult::new(res, "double".to_string())
        });

        // --- RAND ---
        builtins.insert("rand".to_string(), |cg, _args| {
            let rand_int = cg.next_temp();
            let rand_float = cg.next_temp();
            let res = cg.next_temp();
            cg.emit(format!("{} = call i32 @rand()", rand_int));
            cg.emit(format!("{} = sitofp i32 {} to double", rand_float, rand_int));
            cg.emit(format!("{} = fdiv double {}, 32767.0", res, rand_float));
            GeneratorResult::new(res, "double".to_string())
        });

        Self {
            code: Vec::new(),
            temp_counter: 0,
            label_counter: 0,
            scopes: vec![HashMap::new()],
            struct_layout: HashMap::new(),
            class_meta: HashMap::new(),
            current_type_context: None,
            global_decls: Vec::new(),
            current_method_context: None,
            emitted_tuple_types: std::collections::HashSet::new(),
            builtins, // AÑADE ESTO AL FINAL
        }
    
    }

     
    pub fn resolve_parent_method(&self, type_name: &str, method_name: &str) -> Option<String> {
        let meta = self.class_meta.get(type_name)?;
        let parent_name = meta.parent.as_deref()?;
        self.resolve_own_impl(parent_name, method_name)
    }
 
     
    fn resolve_own_impl(&self, type_name: &str, method_name: &str) -> Option<String> {
        let expected_impl = format!("@{}_{}", type_name, method_name);
        if let Some(meta) = self.class_meta.get(type_name) {
            // ¿Tiene este tipo su propia implementación del método?
            let has_own = meta.vtable.iter().any(|s| {
                s.method_name == method_name && s.impl_fn == expected_impl
            });
            if has_own {
                return Some(expected_impl);
            }
            // No la tiene; subir al padre.
            if let Some(ref parent) = meta.parent {
                return self.resolve_own_impl(parent, method_name);
            }
        }
        None
    }


    pub fn register_struct_layout(&mut self, type_name: String, fields: Vec<(String, String)>) {
        self.struct_layout.insert(type_name, fields);
    }
    
    pub fn get_field_index(&self, llvm_type: &str, field_name: &str) -> usize {
        let key = llvm_type.trim_start_matches('%');
        // Buscamos primero en class_meta (campos de datos reales).
        if let Some(meta) = self.class_meta.get(key) {
            // Construimos la lista completa de campos de datos incluyendo
            // los heredados del padre, en orden padre-primero.
            let all_fields = self.collect_all_fields(key);
            if let Some(pos) = all_fields.iter().position(|(n, _)| n == field_name) {
                // +1 porque el índice 0 del struct LLVM es el vptr.
                return pos + 1;
            }
        }
        // Fallback al struct_layout (compatibilidad).
        if let Some(fields) = self.struct_layout.get(key) {
            if let Some(pos) = fields.iter().position(|(n, _)| n == field_name) {
                return pos + 1; // +1 por vptr
            }
        }
        1 // fallback seguro: campo de datos en índice 1
    }

     
    pub fn collect_all_fields(&self, type_name: &str) -> Vec<(String, String)> {
        let mut result = Vec::new();
        if let Some(meta) = self.class_meta.get(type_name) {
            if let Some(ref parent_name) = meta.parent.clone() {
                result.extend(self.collect_all_fields(parent_name));
            }
            result.extend(meta.own_fields.clone());
        }
        result
    }

    /// Resuelve el índice de un slot de VTable para el método `method_name`
    /// en la clase `type_name`.  Retorna None si el método no existe.
    pub fn get_vtable_slot_index(&self, type_name: &str, method_name: &str) -> Option<usize> {
        let meta = self.class_meta.get(type_name)?;
        meta.vtable.iter().position(|s| s.method_name == method_name)
    }

    /// Retorna la implementación concreta (nombre de función LLVM) para un
    /// slot de vtable dado su índice en la clase `type_name`.
    pub fn get_vtable_slot(&self, type_name: &str, slot: usize) -> Option<&VTableSlot> {
        self.class_meta.get(type_name)?.vtable.get(slot)
    }

    

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn define_variable(&mut self, name: String, register: String, ty: String) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, (register, ty));
        }
    }

    pub fn resolve_variable(&self, name: &str) -> Option<(String, String)> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(name) {
                return Some(v.clone());
            }
        }
        None
    }
 

    pub fn next_temp(&mut self) -> String {
        let t = format!("%t{}", self.temp_counter);
        self.temp_counter += 1;
        t
    }

    pub fn next_label(&mut self, prefix: &str) -> String {
        let l = format!("{}_{}", prefix, self.label_counter);
        self.label_counter += 1;
        l
    }

    pub fn emit(&mut self, instr: String) {
        self.code.push(format!("  {}", instr));
    }

    pub fn emit_label(&mut self, label: String) {
        self.code.push(format!("{}:", label));
    }

    pub fn emit_raw(&mut self, line: String) {
        self.code.push(line);
    }

    pub fn last_block_label(&self) -> String {
        for line in self.code.iter().rev() {
            let trimmed = line.trim_end();
            if trimmed.ends_with(':') && !trimmed.starts_with(' ') {
                return trimmed[..trimmed.len() - 1].to_string();
            }
        }
        "entry".to_string()
    }

    pub fn hulk_type_label(ty: &HulkType) -> String {
    match ty {
        HulkType::Class(n) => n.clone(),
        other => Self::hulk_type_to_llvm(other),
    }
}

    pub fn llvm_operand_type(ty: &str) -> String {
    match ty {
        "double" | "i1" | "ptr" => ty.to_string(),
        t if Self::is_tuple_llvm_type(t) => t.to_string(),
        _ => "ptr".to_string(), // nombres de clase ("Wallet", "%Wallet")
    }
}

    pub fn hulk_type_to_llvm(ty: &HulkType) -> String {
        match ty {
            HulkType::Number  => "double".to_string(),
            HulkType::Bool    => "i1".to_string(),
            HulkType::String  => "ptr".to_string(),
            HulkType::Class(_) => "ptr".to_string(),
            HulkType::Unknown  => "ptr".to_string(),
            HulkType::Tuple(elems) => format!("%{}", Self::tuple_struct_name(elems)),
            HulkType::Param(_) => "ptr".to_string(),
            HulkType::Generic(_, hulk_types) => "ptr".to_string(),
        }
    }
 
    pub fn is_tuple_llvm_type(ty: &str) -> bool {
        ty.trim_start_matches('%').starts_with("Tuple_")
    }

    pub fn tuple_struct_name(elems: &[HulkType]) -> String {
        let parts: Vec<String> = elems.iter().map(|e| {
            // Usamos una representación plana (sin '%') apta para nombre de tipo.
            match e {
                HulkType::Number => "double".to_string(),
                HulkType::Bool => "i1".to_string(),
                HulkType::String => "ptr".to_string(),
                HulkType::Class(n) => format!("cls{}", n),
                HulkType::Tuple(inner) => format!("tup_{}", Self::tuple_struct_name(inner)),
                HulkType::Unknown => "ptr".to_string(),
                HulkType::Param(n)      => format!("p{}", n),
                HulkType::Generic(n, a) => format!("cls{}__{}", n,
    a.iter().map(|t| Self::tuple_struct_name(std::slice::from_ref(t))).collect::<Vec<_>>().join("_")),
            }
        }).collect();
        format!("Tuple_{}", parts.join("_"))
    }

    pub fn ensure_tuple_type_emitted(&mut self, elems: &[HulkType]) -> String {
        let name = Self::tuple_struct_name(elems);
        if !self.emitted_tuple_types.contains(&name) {
            let field_types: Vec<String> = elems.iter()
                .map(Self::hulk_type_to_llvm)
                .collect();
            self.global_decls.push(format!(
                "%{} = type {{ {} }}",
                name, field_types.join(", ")
            ));
            self.emitted_tuple_types.insert(name.clone());
        }
        name
    }

    
    pub fn ensure_cstr(&mut self, val: &GeneratorResult) -> GeneratorResult {
        match val.llvm_type.as_str() {
            "ptr" => GeneratorResult::new(val.register.clone(), "ptr".to_string()),
            "double" => {
                let buf = self.next_temp();
                self.emit(format!("{} = call ptr @malloc(i64 32)", buf));
                self.emit(format!(
                    "call i32 (ptr, i64, ptr, ...) @snprintf(ptr {}, i64 32, ptr @.fmt_g, double {})",
                    buf, val.register
                ));
                GeneratorResult::new(buf, "ptr".to_string())
            }
            "i1" => {
                let p = self.next_temp();
                self.emit(format!(
                    "{} = select i1 {}, ptr @.str_true, ptr @.str_false",
                    p, val.register
                ));
                GeneratorResult::new(p, "ptr".to_string())
            }
            // Cualquier otro tipo (objetos, tuplas) ya es un puntero.
            _ => GeneratorResult::new(val.register.clone(), "ptr".to_string()),
        }
    }

    pub fn emit_single_concat(&mut self, l: &GeneratorResult, r: &GeneratorResult) -> GeneratorResult {
        let l = self.ensure_cstr(l);   // <- coerción
        let r = self.ensure_cstr(r);   // <- coerción
        let len_l  = self.next_temp();
        let len_r  = self.next_temp();
        let tot0   = self.next_temp();
        let tot1   = self.next_temp();
        let buf    = self.next_temp();
        self.emit(format!("{} = call i64 @strlen(ptr {})", len_l, l.register));
        self.emit(format!("{} = call i64 @strlen(ptr {})", len_r, r.register));
        self.emit(format!("{} = add i64 {}, {}", tot0, len_l, len_r));
        self.emit(format!("{} = add i64 {}, 1", tot1, tot0));
        self.emit(format!("{} = call ptr @malloc(i64 {})", buf, tot1));
        self.emit(format!("call ptr @strcpy(ptr {}, ptr {})", buf, l.register));
        self.emit(format!("call ptr @strcat(ptr {}, ptr {})", buf, r.register));
        GeneratorResult::new(buf, "ptr".to_string())
    }

    pub fn emit_spaced_concat(&mut self, l: &GeneratorResult, r: &GeneratorResult) -> GeneratorResult {
        let l = self.ensure_cstr(l);   // <- coerción
        let r = self.ensure_cstr(r);   // <- coerción
        let space_global = "@.str.space".to_string();
        if !self.global_decls.iter().any(|d| d.starts_with(&space_global)) {
            self.global_decls.push(
                "@.str.space = private unnamed_addr constant [2 x i8] c\" \\00\"".to_string()
            );
        }
        let len_l  = self.next_temp();
        let len_r  = self.next_temp();
        let tot0   = self.next_temp();
        let tot1   = self.next_temp();
        let tot2   = self.next_temp();
        let buf    = self.next_temp();
        self.emit(format!("{} = call i64 @strlen(ptr {})", len_l, l.register));
        self.emit(format!("{} = call i64 @strlen(ptr {})", len_r, r.register));
        self.emit(format!("{} = add i64 {}, {}", tot0, len_l, len_r));
        self.emit(format!("{} = add i64 {}, 1", tot1, tot0));
        self.emit(format!("{} = add i64 {}, 1", tot2, tot1));
        self.emit(format!("{} = call ptr @malloc(i64 {})", buf, tot2));
        self.emit(format!("call ptr @strcpy(ptr {}, ptr {})", buf, l.register));
        self.emit(format!("call ptr @strcat(ptr {}, ptr {})", buf, space_global));
        self.emit(format!("call ptr @strcat(ptr {}, ptr {})", buf, r.register));
        GeneratorResult::new(buf, "ptr".to_string())
    }
    pub fn collect_subtypes(&self, ancestor: &str) -> Vec<String> {
        let mut result = Vec::new();
        for name in self.class_meta.keys() {
            if self.is_subtype_in_meta(name, ancestor) {
                result.push(name.clone());
            }
        }
        // Sort for deterministic IR output (easier to test/diff).
        result.sort();
        result
    }
 
    // Walk class_meta to test `child` is-a `ancestor`.
    fn is_subtype_in_meta(&self, child: &str, ancestor: &str) -> bool {
        if child == ancestor { return true; }
        if let Some(meta) = self.class_meta.get(child) {
            if let Some(ref parent) = meta.parent {
                return self.is_subtype_in_meta(parent, ancestor);
            }
        }
        false
    }
     
    pub fn emit_equality(&mut self, l: &GeneratorResult, r: &GeneratorResult, want_equal: bool) -> GeneratorResult {
        let res_reg = self.next_temp();
        match l.llvm_type.as_str() {
            "ptr" => {
                let cmp = self.next_temp();
                self.emit(format!("{} = call i32 @strcmp(ptr {}, ptr {})", cmp, l.register, r.register));
                let pred = if want_equal { "eq" } else { "ne" };
                self.emit(format!("{} = icmp {} i32 {}, 0", res_reg, pred, cmp));
            }
            "i1" => {
                let pred = if want_equal { "eq" } else { "ne" };
                self.emit(format!("{} = icmp {} i1 {}, {}", res_reg, pred, l.register, r.register));
            }
            _ => {
                let pred = if want_equal { "oeq" } else { "une" };
                self.emit(format!("{} = fcmp {} double {}, {}", res_reg, pred, l.register, r.register));
            }
        }
        GeneratorResult::new(res_reg, "i1".to_string())
    }
}

 
fn build_vtable_for_class(
    class_name: &str,
    parent_meta: Option<&ClassMeta>,
    own_methods: &[FunctionDecl],
) -> Vec<VTableSlot> {
    // Comenzamos con una copia de la vtable del padre (si existe).
    // Esto garantiza que los índices heredados no cambian.
    let mut vtable: Vec<VTableSlot> = parent_meta
        .map(|m| m.vtable.clone())
        .unwrap_or_default();

    for method in own_methods {
        let method_name = method.name.value.as_id();
        let impl_fn = format!("@{}_{}", class_name, method_name);

        // Buscamos si el padre ya tiene un slot para este método (override).
        if let Some(slot) = vtable.iter_mut().find(|s| s.method_name == method_name) {
            // Override: actualizamos la implementación conservando el índice.
            slot.impl_fn = impl_fn;
            slot.return_type = CodeGenerator::hulk_type_to_llvm(&method.return_type);
        } else {
            // Método nuevo: agregamos un slot al final.
            vtable.push(VTableSlot {
                method_name: method_name.clone(),
                impl_fn,
                fn_ptr_llvm_type: "ptr".to_string(),
                return_type: CodeGenerator::hulk_type_to_llvm(&method.return_type),
            });
        }
    }
    vtable
}

 
fn compile_vtable_global(generator: &mut CodeGenerator, class_name: &str) {
    let vtable = match generator.class_meta.get(class_name) {
        Some(m) => m.vtable.clone(),
        None => return,
    };

    if vtable.is_empty() {
        // Si la clase no tiene métodos, emitimos un tipo vacío con un i8 de relleno
        // (LLVM no permite tipos struct vacíos).
        generator.emit_raw(format!("%VTable_{} = type {{ i8 }}", class_name));
        generator.emit_raw(format!(
            "@vtable_{} = global %VTable_{} {{ i8 0 }}", class_name, class_name
        ));
        generator.emit_raw("".to_string());
        return;
    }

    // Tipo de la VTable: N punteros a función (todos `ptr` en opaque-pointer mode).
    let slot_types: Vec<String> = vtable.iter().map(|_| "ptr".to_string()).collect();
    generator.emit_raw(format!(
        "%VTable_{} = type {{ {} }}",
        class_name,
        slot_types.join(", ")
    ));

    // Valor concreto: inicializar cada slot con la función que lo implementa.
    let slot_inits: Vec<String> = vtable
        .iter()
        .map(|s| format!("ptr {}", s.impl_fn))
        .collect();
    generator.emit_raw(format!(
        "@vtable_{} = global %VTable_{} {{ {} }}",
        class_name,
        class_name,
        slot_inits.join(", ")
    ));
    generator.emit_raw("".to_string());
}

 
fn compile_type_decl(generator: &mut CodeGenerator, decl: &mut TypeDeclNode) {
    let type_name = decl.name.value.as_id();
 
     
    let parent_name: Option<String> = decl.inheritance.as_ref()
        .map(|inh| inh.parent_name.value.as_id());
  
    let parent_meta_clone: Option<ClassMeta> = parent_name.as_deref()
        .and_then(|pn| generator.class_meta.get(pn))
        .cloned();
 
    // Campos propios (sólo los declarados en esta clase, no los heredados).
    let own_fields: Vec<(String, String)> = decl.attributes.iter().map(|attr| {
        let field_name = attr.name.value.as_id();
        let llvm_ty = CodeGenerator::hulk_type_to_llvm(&attr.type_annotation);
        (field_name, llvm_ty)
    }).collect();
 
    // Construimos la vtable lógica (orden: heredados primero, nuevos al final).
    let vtable = build_vtable_for_class(
        &type_name,
        parent_meta_clone.as_ref(),
        &decl.methods,
    );
 
    
    let effective_ctor_params: Vec<(String, String)> = if !decl.params.is_empty() {
        decl.params.iter().map(|(p_name, p_type)| {
            (p_name.value.as_id(), CodeGenerator::hulk_type_to_llvm(p_type))
        }).collect()
    } else {
        // El hijo no declara parámetros: hereda los del padre implícitamente.
        parent_name.as_deref()
            .and_then(|pn| generator.class_meta.get(pn))
            .map(|pm| pm.ctor_params.clone())
            .unwrap_or_default()
    };

    let meta = ClassMeta {
        parent: parent_name.clone(),
        ctor_params: effective_ctor_params.clone(),
        own_fields: own_fields.clone(),
        vtable: vtable.clone(),
    };
    generator.class_meta.insert(type_name.clone(), meta);
 
    
    let all_fields = generator.collect_all_fields(&type_name);
    generator.register_struct_layout(type_name.clone(), all_fields.clone());
 
     
    compile_vtable_global(generator, &type_name);
 
     
    let mut field_types: Vec<String> = vec!["ptr".to_string()]; // vptr en posición 0
    for (_, ty) in &all_fields {
        field_types.push(ty.clone());
    }
 
    generator.emit_raw(format!(
        "%{} = type {{ {} }}",
        type_name,
        field_types.join(", ")
    ));
    generator.emit_raw("".to_string());
 
    
    let ctor_params: Vec<String> = effective_ctor_params.iter().map(|(p_name, llvm_ty)| {
        format!("{} %param_{}", llvm_ty, p_name)
    }).collect();
 
    generator.emit_raw(format!(
        "define ptr @{}_new({}) {{",
        type_name, ctor_params.join(", ")
    ));
    generator.emit_raw("entry:".to_string());
 
    // Calcular tamaño con el "GEP null trick" clásico de LLVM.
    let size_ptr = generator.next_temp();
    let size_val = generator.next_temp();
    generator.emit(format!(
        "{} = getelementptr %{}, ptr null, i32 1",
        size_ptr, type_name
    ));
    generator.emit(format!("{} = ptrtoint ptr {} to i64", size_val, size_ptr));
 
    let self_ptr = generator.next_temp();
    generator.emit(format!("{} = call ptr @malloc(i64 {})", self_ptr, size_val));
 
    
    let vptr_field = generator.next_temp();
    generator.emit(format!(
        "{} = getelementptr inbounds %{}, ptr {}, i32 0, i32 0",
        vptr_field, type_name, self_ptr
    ));
    generator.emit(format!(
        "store ptr @vtable_{}, ptr {}",
        type_name, vptr_field
    ));
 
    // ---- Scope del constructor: parámetros disponibles (sin self) -------
    generator.push_scope();
    for (p_id, llvm_ty) in &effective_ctor_params {
        let ptr = generator.next_temp();
        generator.emit(format!("{} = alloca {}", ptr, llvm_ty));
        generator.emit(format!("store {} %param_{}, ptr {}", llvm_ty, p_id, ptr));
        generator.define_variable(p_id.clone(), ptr, llvm_ty.clone());
    }
 
     
    if let Some(ref parent_nm) = parent_name {
        if let Some( inh) = & mut decl.inheritance {
            if let Some( parent_args) = & mut inh.parent_args {
                // El hijo pasa explícitamente los argumentos al padre.
                let mut arg_strings =  vec![format!("ptr {}", self_ptr)];
                for arg_expr in  parent_args {
                    let res = arg_expr.accept(generator);
                    arg_strings.push(format!("{} {}", res.llvm_type, res.register));
                }
                generator.emit(format!(
                    "call void @{}_init_fields({})",
                    parent_nm, arg_strings.join(", ")
                ));
            }
            // Si parent_args es None: el hijo hereda los mismos parámetros implícitamente.
            // En ese caso propagamos todos los parámetros efectivos al padre.
            else {
                let mut arg_strings = vec![format!("ptr {}", self_ptr)];
                for (p_id, llvm_ty) in &effective_ctor_params {
                    // Leemos desde el alloca que ya generamos arriba.
                    if let Some((ptr, _)) = generator.resolve_variable(p_id) {
                        let loaded = generator.next_temp();
                        generator.emit(format!("{} = load {}, ptr {}", loaded, llvm_ty, ptr));
                        arg_strings.push(format!("{} {}", llvm_ty, loaded));
                    }
                }
                generator.emit(format!(
                    "call void @{}_init_fields({})",
                    parent_nm, arg_strings.join(", ")
                ));
            }
        }
    }
 
    // ---- Inicializar campos propios de T ----------------------------------
    // Usamos los índices en `all_fields` para calcular el GEP correcto.
    // El offset base de los campos propios de T = numero de campos del padre + 1 (vptr).
    let parent_field_count = parent_name.as_deref()
        .map(|pn| generator.collect_all_fields(pn).len())
        .unwrap_or(0);
 
    for (attr_idx, attr) in decl.attributes.iter_mut().enumerate() {
        // Índice real en el struct LLVM: 0=vptr, 1..parent_count=campos padre, etc.
        let struct_idx = 1 + parent_field_count + attr_idx;
        let field_llvm_ty = CodeGenerator::hulk_type_to_llvm(&attr.type_annotation);
 
        let init_val = attr.initializer.accept(generator);
 
        let field_ptr = generator.next_temp();
        generator.emit(format!(
            "{} = getelementptr inbounds %{}, ptr {}, i32 0, i32 {}",
            field_ptr, type_name, self_ptr, struct_idx
        ));

        
        generator.emit(format!(
            "store {} {}, ptr {}",
            field_llvm_ty, init_val.register, field_ptr
        ));
    }
 
    generator.pop_scope();
    generator.emit(format!("ret ptr {}", self_ptr));
    generator.emit_raw("}".to_string());
    generator.emit_raw("".to_string());
 
     
    let init_params: Vec<String> = {
        let mut v = vec!["ptr %self".to_string()];
        v.extend(effective_ctor_params.iter().map(|(p_name, llvm_ty)| {
            format!("{} %param_{}", llvm_ty, p_name)
        }));
        v
    };
 
    generator.emit_raw(format!(
        "define void @{}_init_fields({}) {{",
        type_name, init_params.join(", ")
    ));
    generator.emit_raw("entry:".to_string());
 
    // Si tenemos padre, primero inicializamos sus campos.
    if let Some(ref parent_nm) = parent_name {
        if let Some(inh) = &mut decl.inheritance {
            if let Some( parent_args) = &mut inh.parent_args {
                let mut arg_strings = vec!["ptr %self".to_string()];
                for arg_expr in  parent_args {
                    // Nota: en _init_fields los parámetros están como %param_xxx
                    // pero no tenemos scopes activos aquí; necesitamos un scope
                    // temporal.
                    generator.push_scope();
                    for (p_id, llvm_ty) in &effective_ctor_params {
                        let ptr = generator.next_temp();
                        generator.emit(format!("{} = alloca {}", ptr, llvm_ty));
                        generator.emit(format!("store {} %param_{}, ptr {}", llvm_ty, p_id, ptr));
                        generator.define_variable(p_id.clone(), ptr, llvm_ty.clone());
                    }
                    let res = arg_expr.accept(generator);
                    arg_strings.push(format!("{} {}", res.llvm_type, res.register));
                    generator.pop_scope();
                }
                generator.emit(format!(
                    "call void @{}_init_fields({})",
                    parent_nm, arg_strings.join(", ")
                ));
            } else {
                // Propagación implícita de parámetros.
                let mut arg_strings = vec!["ptr %self".to_string()];
                for (p_id, llvm_ty) in &effective_ctor_params {
                    arg_strings.push(format!("{} %param_{}", llvm_ty, p_id));
                }
                generator.emit(format!(
                    "call void @{}_init_fields({})",
                    parent_nm, arg_strings.join(", ")
                ));
            }
        }
    }
 
    // Inicializar campos propios de T.
    generator.push_scope();
    for (p_id, llvm_ty) in &effective_ctor_params {
        let ptr = generator.next_temp();
        generator.emit(format!("{} = alloca {}", ptr, llvm_ty));
        generator.emit(format!("store {} %param_{}, ptr {}", llvm_ty, p_id, ptr));
        generator.define_variable(p_id.clone(), ptr, llvm_ty.clone());
    }
 
    for (attr_idx, attr) in & mut decl.attributes.iter_mut().enumerate() {
        let struct_idx = 1 + parent_field_count + attr_idx;
        let field_llvm_ty = CodeGenerator::hulk_type_to_llvm(&attr.type_annotation);
        let init_val = attr.initializer.accept(generator);
        let field_ptr = generator.next_temp();
        generator.emit(format!(
            "{} = getelementptr inbounds %{}, ptr %self, i32 0, i32 {}",
            field_ptr, type_name, struct_idx
        ));
        generator.emit(format!(
            "store {} {}, ptr {}",
            field_llvm_ty, init_val.register, field_ptr
        ));
    }
    generator.pop_scope();
 
    generator.emit("ret void".to_string());
    generator.emit_raw("}".to_string());
    generator.emit_raw("".to_string());
 
   
    let old_type_ctx = generator.current_type_context.take();
    generator.current_type_context = Some(type_name.clone());
 
    for method in &mut decl.methods {
        let method_name = method.name.value.as_id();
        let return_type=CodeGenerator::hulk_type_to_llvm(&method.return_type);
        let mut method_params = vec!["ptr %self".to_string()];
       
        method_params.extend(method.params.iter().map(|(p_name, p_type)| {
            let llvm_ty = CodeGenerator::hulk_type_to_llvm(p_type);
            format!("{} %param_{}", llvm_ty, p_name.value.as_id())
        }));
 
        generator.emit_raw(format!(
            "define {} @{}_{} ({}) {{",return_type,
            type_name, method_name,
            method_params.join(", ")
        ));
        generator.emit_raw("entry:".to_string());
 
        generator.push_scope();
 
        // Exponer self en el scope.
        generator.define_variable(
            "%self".to_string(),
            "%self".to_string(),
            format!("%{}", type_name),
        );
  
        let all_fields_for_method = generator.collect_all_fields(&type_name);
        for (field_idx, (field_name, field_llvm_ty)) in &mut all_fields_for_method.iter().enumerate() {
            // Índice LLVM: +1 porque el índice 0 es el vptr.
            let struct_idx = field_idx + 1;
 
            // GEP: obtener puntero al campo dentro del struct.
            let field_gep = generator.next_temp();
            generator.emit(format!(
                "{} = getelementptr inbounds %{}, ptr %self, i32 0, i32 {}  ; campo {}",
                field_gep, type_name, struct_idx, field_name
            ));
 
            // Load: leer el valor actual del campo.
            let field_val = generator.next_temp();
            generator.emit(format!(
                "{} = load {}, ptr {}",
                field_val, field_llvm_ty, field_gep
            ));
 
            // Alloca + store: guardarlo como variable mutable en el scope
            // para que dest-assign (field := expr) también funcione.
            let field_ptr = generator.next_temp();
            generator.emit(format!("{} = alloca {}", field_ptr, field_llvm_ty));
            generator.emit(format!("store {} {}, ptr {}", field_llvm_ty, field_val, field_ptr));
 
            generator.define_variable(field_name.clone(), field_ptr, field_llvm_ty.clone());
        }
 
        // Exponer parámetros del método.
        
        for (p_name, p_type) in &method.params {
            let p_id = p_name.value.as_id();
            let storage_ty = CodeGenerator::hulk_type_to_llvm(p_type); // ptr para clases
            let label      = CodeGenerator::hulk_type_label(p_type);   // "Vector" para clases
            let ptr = generator.next_temp();
            generator.emit(format!("{} = alloca {}", ptr, storage_ty));
            generator.emit(format!("store {} %param_{}, ptr {}", storage_ty, p_id, ptr));
            generator.define_variable(p_id, ptr, label);
        }
        // Registrar el método activo para que visit_base_call pueda resolverlo.
        generator.current_method_context = Some(method_name.clone());
 
         
        let result = method.body.accept(generator); 
        let ret_ty = CodeGenerator::llvm_operand_type(&result.llvm_type);
        generator.emit(format!("ret {} {}", ret_ty, result.register));
 
        generator.current_method_context = None; // salir del contexto del método
        generator.pop_scope();
        generator.emit_raw("}".to_string());
        generator.emit_raw("".to_string());
    }
 
    generator.current_type_context = old_type_ctx;
}

 
fn compile_function_decl(generator: &mut CodeGenerator, decl: &mut FunctionDecl) {
    let name = decl.name.value.as_id();
    let return_type=CodeGenerator::hulk_type_to_llvm(&decl.return_type);
    
    let params: Vec<String> = decl.params.iter().map(|(p_name, p_type)| {
        let llvm_ty = CodeGenerator::hulk_type_to_llvm(p_type);
        let p_id = p_name.value.as_id();
        format!("{} %param_{}", llvm_ty, p_id)
    }).collect();

    generator.emit_raw(format!("define {} @{}({}) {{", return_type, name, params.join(", ")));
    generator.emit_raw("entry:".to_string());

    generator.push_scope();
    for (p_name, p_type) in &decl.params {
        let p_id = p_name.value.as_id();
        let storage_ty = CodeGenerator::hulk_type_to_llvm(p_type);
        let label      = CodeGenerator::hulk_type_label(p_type);
        let ptr = generator.next_temp();
        generator.emit(format!("{} = alloca {}", ptr, storage_ty));
        generator.emit(format!("store {} %param_{}, ptr {}", storage_ty, p_id, ptr));
        generator.define_variable(p_id, ptr, label);
    }

     let result = decl.body.accept(generator); // o method.body en el otro
    let ret_ty = CodeGenerator::llvm_operand_type(&result.llvm_type);
    generator.emit(format!("ret {} {}", ret_ty, result.register));
    generator.pop_scope();

    generator.emit_raw("}".to_string());
    generator.emit_raw("".to_string());
}

 
pub fn compile_hulk_program(
    program: &mut Program,
    _module_name: &str,
    ir_output: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut generator = CodeGenerator::new();

    let mut header: Vec<String> = vec![
        "; ModuleID = 'hulk'".to_string(),
        "target triple = \"x86_64-pc-linux-gnu\"".to_string(),
        //"target triple = \"x86_64-pc-windows-msvc\"".to_string(),
        "".to_string(),
        "declare ptr @malloc(i64)".to_string(),
        "declare i64 @strlen(ptr)".to_string(),
        "declare ptr @strcpy(ptr, ptr)".to_string(),
        "declare ptr @strcat(ptr, ptr)".to_string(),
        "declare i32 @strcmp(ptr, ptr)".to_string(),
        "declare void @abort() noreturn".to_string(),
        "".to_string(),
        "declare double @sqrt(double)".to_string(),
        "declare double @sin(double)".to_string(),
        "declare double @cos(double)".to_string(),
        "declare double @exp(double)".to_string(),
        "declare double @log(double)".to_string(),
        "declare i32 @rand()".to_string(),
        "".to_string(),
        "declare i32 @printf(ptr, ...)".to_string(),
         "declare i32 @snprintf(ptr, i64, ptr, ...)".to_string(),  
        "@.fmt_double = private unnamed_addr constant [4 x i8] c\"%g\\0A\\00\"".to_string(),
          "@.fmt_g      = private unnamed_addr constant [3 x i8] c\"%g\\00\"".to_string(),
        "@.fmt_str    = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\"".to_string(),
        "@.str_true   = private unnamed_addr constant [5 x i8] c\"true\\00\"".to_string(),
        "@.str_false  = private unnamed_addr constant [6 x i8] c\"false\\00\"".to_string(),
        "@.str_cast_error = private unnamed_addr constant [43 x i8] c\"HULK runtime error: invalid downcast (as)\\0A\\00\"".to_string(),
        "".to_string(),
    ];

    let mut type_decls: Vec<&mut TypeDeclNode> = Vec::new();
    let mut fun_decls:  Vec<&mut FunctionDecl> = Vec::new();
    let mut expressions: Vec<&mut Expr>   = Vec::new();

    for stmt in &mut program.statements {
        match stmt {
            Statement::TypeDecl(td)     => type_decls.push(td),
            Statement::FunctionDecl(fd) => fun_decls.push(fd),
            Statement::Expression(e)    => expressions.push(e),
        }
    }

    // PASADA 1: tipos (structs + vtables + constructores + métodos)
    for td in &mut type_decls {
        compile_type_decl(&mut generator,  td);
    }

    // PASADA 2: funciones globales
    for fd in &mut fun_decls {
        compile_function_decl(&mut generator,  fd);
    }

    generator.emit_raw("define void @hulk_cast_error() noreturn {".to_string());
    generator.emit_raw("entry:".to_string());
    generator.emit("call i32 (ptr, ...) @printf(ptr @.fmt_str, ptr @.str_cast_error)".to_string());
    generator.emit("call void @abort()".to_string());
    generator.emit("unreachable".to_string());
    generator.emit_raw("}".to_string());
    generator.emit_raw("".to_string());

    // PASADA 3: expresiones de top-level en @main
    generator.emit_raw("define i32 @main() {".to_string());
    generator.emit_raw("entry:".to_string());

    let mut top_exprs: Vec<GeneratorResult> = Vec::new();

    for e in &mut expressions{
        top_exprs.push(e.accept(&mut generator));
    }
     

    let last_result = top_exprs.last().cloned().unwrap_or_else(|| {
        GeneratorResult::new("0".to_string(), "i32".to_string())
    });

    if last_result.llvm_type == "i1" {
        let ret_reg = generator.next_temp();
        generator.emit(format!("{} = zext i1 {} to i32", ret_reg, last_result.register));
        generator.emit(format!("ret i32 {}", ret_reg));
    } else {
        generator.emit("ret i32 0".to_string());
    }

    generator.emit_raw("}".to_string());

    header.extend(generator.global_decls);
    header.extend(generator.code);
    let full_ir = header.join("\n");

    if let Some(path) = ir_output {
        fs::write(path, &full_ir)?;
    }

    Ok(full_ir)
}