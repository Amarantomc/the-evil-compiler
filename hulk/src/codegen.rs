use std::fs;
use std::collections::HashMap;
use crate::expr_visitor::ExprVisitor;
use crate::nodes::block_node::BlockNode;
use crate::nodes::function_decl_node::FunctionDecl;
use crate::nodes::literal_node::Literal;
use crate::nodes::program_node::{Program, Statement};
use crate::nodes::type_decl_node::TypeDeclNode;
use crate::nodes::typedexpr_node::{Expr, HulkType, TypedExpr};

// ---------------------------------------------------------------------------
// GeneratorResult
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// CodeGenerator
// ---------------------------------------------------------------------------

pub struct CodeGenerator {
    /// Instrucciones LLVM IR acumuladas.
    pub code: Vec<String>,
    pub temp_counter: usize,
    pub label_counter: usize,
    /// Tabla de símbolos por scope: nombre -> (registro/ptr LLVM, tipo LLVM).
    /// La clave especial "%self" guarda el puntero a la instancia actual.
    pub scopes: Vec<HashMap<String, (String, String)>>,
    /// Layout de structs: TypeName -> lista ordenada de (nombre_campo, tipo_llvm_campo).
    pub struct_layout: HashMap<String, Vec<(String, String)>>,
    /// Tipo HULK que se está compilando actualmente (para resolver nombres de métodos).
    pub current_type_context: Option<String>,
    pub global_decls: Vec<String>,
    
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            temp_counter: 0,
            label_counter: 0,
            scopes: vec![HashMap::new()],
            struct_layout: HashMap::new(),
            current_type_context: None,
            global_decls: Vec::new(),
        }
    }

    // --- Layout helpers ---

    pub fn register_struct_layout(&mut self, type_name: String, fields: Vec<(String, String)>) {
        self.struct_layout.insert(type_name, fields);
    }

    /// Retorna el índice (base-0) de `field_name` dentro del struct `llvm_type`.
    pub fn get_field_index(&self, llvm_type: &str, field_name: &str) -> usize {
         
        let key = llvm_type.trim_start_matches('%');
        if let Some(fields) = self.struct_layout.get(key) {
            if let Some(pos) = fields.iter().position(|(n, _)| n == field_name) {
                print!("FIELD {} , found at index {}\n", field_name,pos);
                return pos;
            }
        }
        0
    }

    // --- Scope helpers ---

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

    // --- IR emission helpers ---

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

    // --- Helpers de tipo HULK -> LLVM ---

    pub fn hulk_type_to_llvm(ty: &HulkType) -> String {
        match ty {
            HulkType::Number  => "double".to_string(),
            HulkType::Bool    => "i1".to_string(),
            HulkType::String  => "ptr".to_string(),
            HulkType::Class(_) => "ptr".to_string(),
            HulkType::Unknown  => "double".to_string(), // fallback conservador
        }
    }
}

// ---------------------------------------------------------------------------
// Compilación de declaraciones de función global
// ---------------------------------------------------------------------------

fn compile_function_decl(generator: &mut CodeGenerator, decl: &FunctionDecl) {
    let name = decl.name.value.as_id();

    // Construir lista de parámetros LLVM
    let params: Vec<String> = decl.params.iter().map(|(p_name, p_type)| {
        let llvm_ty = CodeGenerator::hulk_type_to_llvm(p_type);
        let p_id = p_name.value.as_id();
        format!("{} %param_{}", llvm_ty, p_id)
    }).collect();
    print!("Compilando función global '{}', parámetros: [{}]\n", name, params.join(", "));
    // Emitir encabezado de función (asumimos retorno double por defecto)
    generator.emit_raw(format!("define double @{}({}) {{", name, params.join(", ")));
    generator.emit_raw("entry:".to_string());

    // Nuevo scope: mapear parámetros a alloca para permitir shadowing
    generator.push_scope();
    for (p_name, p_type) in &decl.params {
        let p_id = p_name.value.as_id();
        let llvm_ty = CodeGenerator::hulk_type_to_llvm(p_type);
        let ptr = generator.next_temp();
        generator.emit(format!("{} = alloca {}", ptr, llvm_ty));
        generator.emit(format!("store {} %param_{}, ptr {}", llvm_ty, p_id, ptr));
        generator.define_variable(p_id, ptr, llvm_ty);
    }

    let result = decl.body.accept(generator);
    generator.emit(format!("ret {} {}", result.llvm_type, result.register));
    generator.pop_scope();

    generator.emit_raw("}".to_string());
    generator.emit_raw("".to_string());
}

// ---------------------------------------------------------------------------
// Compilación de declaraciones de tipo
//
// Para cada tipo T con campos f0..fN y métodos m0..mK se generatorera:
//
//   %T = type { <tipo_f0>, <tipo_f1>, ... }          ; definición del struct
//
//   define ptr @T_new(<params>) {                     ; constructor
//     %self = call ptr @malloc(i64 <sizeof>)
//     ; inicializar cada campo con GEP + store
//     ret ptr %self
//   }
//
//   define <ret> @T_m0(ptr %self, <params>) { ... }   ; métodos
// ---------------------------------------------------------------------------

fn compile_type_decl(generator: &mut CodeGenerator, decl: &TypeDeclNode) {
    let type_name = decl.name.value.as_id();

    // ------------------------------------------------------------------
    // 1. Calcular layout de campos y registrarlo
    // ------------------------------------------------------------------
    let field_llvm_types: Vec<(String, String)> = decl.attributes.iter().map(|attr| {
        let field_name = attr.name.value.as_id();
        let llvm_ty = CodeGenerator::hulk_type_to_llvm(&attr.type_annotation);
        (field_name, llvm_ty)
    }).collect();

    generator.register_struct_layout(type_name.clone(), field_llvm_types.clone());

    // ------------------------------------------------------------------
    // 2. Emitir definición del struct LLVM
    //    %TypeName = type { field0_ty, field1_ty, ... }
    // ------------------------------------------------------------------
    let field_types_str: Vec<String> = field_llvm_types.iter()
        .map(|(_, ty)| ty.clone())
        .collect();

    generator.emit_raw(format!(
        "%{} = type {{ {} }}",
        type_name,
        if field_types_str.is_empty() { "i8".to_string() } // struct vacío no es válido en LLVM
        else { field_types_str.join(", ") }
    ));
    generator.emit_raw("".to_string());

    // ------------------------------------------------------------------
    // 3. Emitir el constructor: @TypeName_new(params...) -> ptr
    //
    //    Convención:
    //      - malloc(sizeof(%TypeName)) aloca el struct en el heap.
    //      - Los parámetros del tipo están disponibles en el scope de
    //        inicialización de atributos (sin self, según la spec).
    //      - Cada atributo se inicializa con su expresión y se guarda
    //        con un GEP + store.
    // ------------------------------------------------------------------
    let ctor_params: Vec<String> = decl.params.iter().map(|(p_name, p_type)| {
        let llvm_ty = CodeGenerator::hulk_type_to_llvm(p_type);
        format!("{} %param_{}", llvm_ty, p_name.value.as_id())
    }).collect();

    generator.emit_raw(format!(
        "define ptr @{}_new({}) {{",
        type_name,
        ctor_params.join(", ")
    ));
    generator.emit_raw("entry:".to_string());

    // Calcular tamaño del struct con getelementptr null trick (idioma LLVM clásico)
    // %size_ptr = getelementptr %T, ptr null, i32 1
    // %size     = ptrtoint ptr %size_ptr to i64
    let size_ptr = generator.next_temp();
    let size_val = generator.next_temp();
    generator.emit(format!(
        "{} = getelementptr %{}, ptr null, i32 1",
        size_ptr, type_name
    ));
    generator.emit(format!("{} = ptrtoint ptr {} to i64", size_val, size_ptr));

    // Llamar a malloc
    let self_ptr = generator.next_temp();
    generator.emit(format!("{} = call ptr @malloc(i64 {})", self_ptr, size_val));

    // Scope del constructor: parámetros del tipo disponibles (sin self)
    generator.push_scope();
    for (p_name, p_type) in &decl.params {
        let p_id = p_name.value.as_id();
        let llvm_ty = CodeGenerator::hulk_type_to_llvm(p_type);
        let ptr = generator.next_temp();
        generator.emit(format!("{} = alloca {}", ptr, llvm_ty));
        generator.emit(format!("store {} %param_{}, ptr {}", llvm_ty, p_id, ptr));
        generator.define_variable(p_id, ptr, llvm_ty);
    }

    // Inicializar cada atributo
    for (field_idx, attr) in decl.attributes.iter().enumerate() {
        let field_llvm_ty = &field_llvm_types[field_idx].1;

        // Evaluar la expresión inicializadora (sin self en scope, según spec)
        let init_val = attr.initializer.accept(generator);

        // GEP al campo dentro del struct
        let field_ptr = generator.next_temp();
        generator.emit(format!(
            "{} = getelementptr inbounds %{}, ptr {}, i32 0, i32 {}",
            field_ptr, type_name, self_ptr, field_idx
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

    // ------------------------------------------------------------------
    // 4. Emitir cada método: @TypeName_methodName(ptr %self, params...) -> ret
    // ------------------------------------------------------------------
    let old_type_ctx = generator.current_type_context.take();
    generator.current_type_context = Some(type_name.clone());

    for method in &decl.methods {
        let method_name = method.name.value.as_id();

        let mut method_params = vec!["ptr %self".to_string()];
        method_params.extend(method.params.iter().map(|(p_name, p_type)| {
            let llvm_ty = CodeGenerator::hulk_type_to_llvm(p_type);
            format!("{} %param_{}", llvm_ty, p_name.value.as_id())
        }));

        generator.emit_raw(format!(
            "define double @{}_{} ({}) {{",
            type_name, method_name,
            method_params.join(", ")
        ));
        generator.emit_raw("entry:".to_string());

        generator.push_scope();

        // Exponer self en el scope con la clave "%self"
        generator.define_variable(
            "%self".to_string(),
            "%self".to_string(),
            format!("%{}", type_name),
        );

        // Exponer parámetros del método
        for (p_name, p_type) in &method.params {
            let p_id = p_name.value.as_id();
            let llvm_ty = CodeGenerator::hulk_type_to_llvm(p_type);
            let ptr = generator.next_temp();
            generator.emit(format!("{} = alloca {}", ptr, llvm_ty));
            generator.emit(format!("store {} %param_{}, ptr {}", llvm_ty, p_id, ptr));
            generator.define_variable(p_id, ptr, llvm_ty);
        }

        let result = method.body.accept(generator);
        generator.emit(format!("ret {} {}", result.llvm_type, result.register));

        generator.pop_scope();
        generator.emit_raw("}".to_string());
        generator.emit_raw("".to_string());
    }

    generator.current_type_context = old_type_ctx;
}

// ---------------------------------------------------------------------------
// Punto de entrada: compile_hulk_program
//
// Pasadas:
//   1. Pasada de tipos  — emite structs LLVM, constructores _new y métodos.
//   2. Pasada de funciones globales — emite las funciones fuera de main.
//   3. Pasada de expresiones — las evalúa dentro de @main.
// ---------------------------------------------------------------------------

pub fn compile_hulk_program(
    program: Program,
    _module_name: &str,
    ir_output: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut generator = CodeGenerator::new();

    // Cabecera del módulo
    let mut header: Vec<String> = vec![
        "; ModuleID = 'hulk'".to_string(),
        "target triple = \"x86_64-pc-linux-gnu\"".to_string(),
        "".to_string(),
        "; declaración externa de malloc (para constructores)".to_string(),
        "declare ptr @malloc(i64)".to_string(),
        "".to_string(),
        "; --- Nativas / Built-ins ---".to_string(),
        "declare i32 @printf(ptr, ...)".to_string(),
        "@.fmt_double = private unnamed_addr constant [4 x i8] c\"%g\\0A\\00\"".to_string(),
        "".to_string(),
        "@.fmt_str = private unnamed_addr constant [4 x i8] c\"%s\\0A\\00\"".to_string(),
        "@.str_true = private unnamed_addr constant [5 x i8] c\"true\\00\"".to_string(),
        "@.str_false = private unnamed_addr constant [6 x i8] c\"false\\00\"".to_string(),
        "".to_string(),
    ];

    // Separar las sentencias por categoría manteniendo el orden original
    // para que las expresiones de top-level respeten la secuencia del programa.
    let mut type_decls: Vec<&TypeDeclNode> = Vec::new();
    let mut fun_decls:  Vec<&FunctionDecl> = Vec::new();
    let mut expressions: Vec<&TypedExpr>   = Vec::new();

    for stmt in &program.statements {
        match stmt {
            Statement::TypeDecl(td)     => type_decls.push(td),
            Statement::FunctionDecl(fd) => fun_decls.push(fd),
            Statement::Expression(e)    => expressions.push(e),
        }
    }
    print!("{:#?}\n", &fun_decls);
   
    // ------------------------------------------------------------------
    // PASADA 1 — Tipos
    // Primero emitimos solo las definiciones de struct para que el resto
    // del IR pueda referirse a ellos, luego los constructores y métodos.
    // ------------------------------------------------------------------
    for td in &type_decls {
        compile_type_decl(&mut generator, td);
    }

    // ------------------------------------------------------------------
    // PASADA 2 — Funciones globales
    // ------------------------------------------------------------------
    for fd in &fun_decls {
        compile_function_decl(&mut generator, fd);
    }

    // ------------------------------------------------------------------
    // PASADA 3 — Expresiones de top-level dentro de @main
    // ------------------------------------------------------------------
    generator.emit_raw("define i32 @main() {".to_string());
    generator.emit_raw("entry:".to_string());

    let top_exprs: Vec<GeneratorResult> = expressions
        .into_iter()
        .map(|e| {
            // Reconstruimos un bloque con los TypedExpr clonando el kind.
            // Como TypedExpr no implementa Clone, los compilamos directamente.
            e.accept(&mut generator)
        })
        .collect();

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
    
    // Ensamblar IR final: cabecera + globales + todo el código generado
    header.extend(generator.global_decls); // <--- NUEVO: Inyectar las strings aquí
    header.extend(generator.code);
    let full_ir = header.join("\n");

    if let Some(path) = ir_output {
        fs::write(path, &full_ir)?;
    }

    Ok(full_ir)

    // // Ensamblar IR final: cabecera + todo el código generatorerado
    // header.extend(generator.code);
    // let full_ir = header.join("\n");

    // if let Some(path) = ir_output {
    //     fs::write(path, &full_ir)?;
    // }

    // Ok(full_ir)
}