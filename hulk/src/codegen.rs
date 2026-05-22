use std::fs;
use crate::nodes::block_node::BlockNode;
use crate::nodes::program_node::{Program, Statement};
use crate::nodes::typedexpr_node::{Expr, TypedExpr};

/// Representa el resultado de una expresión en LLVM IR.
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

pub struct CodeGenerator {
    pub code: Vec<String>,       
    pub temp_counter: usize,      
    pub label_counter: usize,     
    /// Tabla de símbolos para manejar scopes: mapea nombre de variable a (registro LLVM, tipo LLVM)
    pub scopes: Vec<std::collections::HashMap<String, (String, String)>>,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            code: Vec::new(),
            temp_counter: 0,
            label_counter: 0,
            scopes: vec![std::collections::HashMap::new()], // Scope inicial
        }
    }

    /// Entra en un nuevo scope
    pub fn push_scope(&mut self) {
        self.scopes.push(std::collections::HashMap::new());
    }

    /// Sale del scope actual
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Define una variable en el scope actual
    pub fn define_variable(&mut self, name: String, register: String, ty: String) {
        if let Some(current_scope) = self.scopes.last_mut() {
            current_scope.insert(name, (register, ty));
        }
    }

    /// Busca una variable desde el scope más interno al más externo
    pub fn resolve_variable(&self, name: &str) -> Option<(String, String)> {
        for scope in self.scopes.iter().rev() {
            if let Some(res) = scope.get(name) {
                return Some(res.clone());
            }
        }
        None
    }

    pub fn next_temp(&mut self) -> String {
        let name = format!("%t{}", self.temp_counter);
        self.temp_counter += 1;
        name
    }

    pub fn next_label(&mut self, prefix: &str) -> String {
        let label = format!("{}_{}", prefix, self.label_counter);
        self.label_counter += 1;
        label
    }

    pub fn emit(&mut self, instr: String) {
        self.code.push(format!("  {}", instr));
    }

    pub fn emit_label(&mut self, label: String) {
        self.code.push(format!("{}:", label));
    }

    /// Retorna el nombre de la última etiqueta emitida (sin los dos puntos).
    pub fn last_block_label(&self) -> String {
        for line in self.code.iter().rev() {
            if line.ends_with(':') {
                return line[..line.len()-1].to_string();
            }
        }
        "entry".to_string()
    }
}

pub fn compile_hulk_program(
    program: Program,
    _module_name: &str, 
    ir_output: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let mut generator = CodeGenerator::new();

    let mut final_code = Vec::new();
    final_code.push("; ModuleID = 'hulk'".to_string());
    final_code.push("target triple = 'x86_64-pc-linux-gnu'".to_string());
    final_code.push("".to_string());

    generator.code.push("define i32 @main() {".to_string());
    generator.code.push("entry:".to_string());

    let mut expressions = Vec::new();
    for stmt in program.statements {
        match stmt {
            Statement::Expression(expr) => expressions.push(expr),
            Statement::FunctionDecl(_decl) => {}
            Statement::TypeDecl(type_decl_node) => todo!(),
        }
    }

    let top_level_block = TypedExpr::new(Expr::Block(BlockNode::new(expressions)));
    let result = top_level_block.accept(&mut generator);

    if result.llvm_type == "i1" {
        let ret_reg = generator.next_temp();
        generator.emit(format!("{} = zext i1 {} to i32", ret_reg, result.register));
        generator.emit(format!("ret i32 {}", ret_reg));
    } else {
        generator.emit("ret i32 0".to_string());
    }

    generator.code.push("}".to_string());

    final_code.extend(generator.code);
    let full_ir = final_code.join("\n");

    // 5. Opcionales y ejecución
    if let Some(path) = ir_output {
        fs::write(path, &full_ir)?;
    }

    Ok(full_ir)
}