use inkwell::OptimizationLevel;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{InitializationConfig, Target};
use std::fs;
use std::io;

use crate::nodes::block_node::BlockNode;
use crate::nodes::program_node::{Program, Statement};
use crate::nodes::typedexpr_node::{Expr, TypedExpr};



pub struct CodeGenerator<'ctx> {
    pub context: &'ctx Context,
    pub module: Module<'ctx>,
    pub builder: Builder<'ctx>,
}

impl<'ctx> CodeGenerator<'ctx> {
    pub fn new(context: &'ctx Context, module_name: &str) -> Self {
        let module = context.create_module(module_name);
        let builder = context.create_builder();

        CodeGenerator {
            context,
            module,
            builder,
        }
    }

    /// Guarda el IR en un archivo .ll
    pub fn save_ir(&self, filename: &str) -> Result<(), Box<dyn std::error::Error>> {
        let ir_string = self.module.print_to_string().to_string();
        fs::write(filename, ir_string)?;
        println!("IR guardado en: {}", filename);
        Ok(())
    }

    /// Compila el módulo a ejecutable usando JIT
    pub fn compile_and_execute(&self) -> Result<i32, Box<dyn std::error::Error>> {
         
        let execution_engine = self
            .module
            .create_jit_execution_engine(OptimizationLevel::Aggressive)?;

        unsafe {
            // Busca la función main
            let main_function =
                execution_engine.get_function::<unsafe extern "C" fn() -> i32>("main")?;
            let result = main_function.call();
            Ok(result)
        }
    }

    /// Compila a LLVM IR y genera un objeto (.o)
    pub fn compile_to_object(&self, output_file: &str) -> Result<(), Box<dyn std::error::Error>> {
         
        Target::initialize_native(&InitializationConfig::default())?;

         
        let triple = inkwell::targets::TargetMachine::get_default_triple();
        self.module.set_triple(&triple);

        
        let target = Target::from_triple(&triple)?;

         
        let target_machine = target
            .create_target_machine(
                &triple,
                "generic",
                "",
                OptimizationLevel::Aggressive,
                inkwell::targets::RelocMode::Default,
                inkwell::targets::CodeModel::Default,
            )
            .expect("No se pudo crear la target machine");

        
        target_machine.write_to_file(
            &self.module,
            inkwell::targets::FileType::Object,
            output_file.as_ref(),
        )?;
        println!("Archivo objeto guardado en: {}", output_file);
        Ok(())
    }

     
    pub fn get_module(&self) -> &Module<'ctx> {
        &self.module
    }

    
    pub fn get_builder(&self) -> &Builder<'ctx> {
        &self.builder
    }

     
    pub fn get_context(&self) -> &'ctx Context {
        self.context
    }
}

// pub fn generate_ir_and_execute(
//     ast: &TypedExpr,
//     module_name: &str,
//     ir_output: Option<&str>,
// ) -> Result<i32, Box<dyn std::error::Error>> {
//     Target::initialize_native(&InitializationConfig::default())
//         .map_err(|e| Box::<dyn std::error::Error>::from(io::Error::new(io::ErrorKind::Other, e)))?;

//     let context = Context::create();
//     let mut code_gen = CodeGenerator::new(&context, module_name);

//     let i32_type = context.i32_type();
//     let fn_type = i32_type.fn_type(&[], false);
//     let function = code_gen.module.add_function("main", fn_type, None);
//     let basic_block = context.append_basic_block(function, "entry");
//     code_gen.builder.position_at_end(basic_block);

//     let result = ast.accept(&mut code_gen);
//     code_gen.builder.build_return(Some(&result))?;

//     if let Some(path) = ir_output {
//         code_gen.save_ir(path)?;
//     }

//     code_gen.compile_and_execute()
// }

pub fn compile_hulk_program(
    program: Program,
    module_name: &str,
    ir_output: Option<&str>,
) -> Result<i32, Box<dyn std::error::Error>> {
    // 1. Inicializar LLVM
    Target::initialize_native(&InitializationConfig::default())
        .map_err(|e| Box::<dyn std::error::Error>::from(io::Error::new(io::ErrorKind::Other, e)))?;

    let context = Context::create();
    let mut code_gen = CodeGenerator::new(&context, module_name);

    // 2. Preparar el punto de entrada 'main'
    let i32_type = context.i32_type();
    let fn_type = i32_type.fn_type(&[], false);
    let function = code_gen.module.add_function("main", fn_type, None);
    let basic_block = context.append_basic_block(function, "entry");
    code_gen.builder.position_at_end(basic_block);

    // 3. Convertir el programa en un bloque ejecutable
    // Separamos declaraciones de funciones de las expresiones top-level
    let mut expressions = Vec::new();
    for stmt in program.statements {
        match stmt {
            Statement::Expression(expr) => expressions.push(expr),
            Statement::FunctionDecl(_decl) => {
                // TODO: Implementar registro de funciones globales en el módulo
            }
        }
    }

    // Envolvemos todo en un bloque para reusar visit_block y obtener el último valor
    let top_level_block = TypedExpr::new(Expr::Block(BlockNode::new(expressions)));
    
    // 4. Generar el IR
    let result = top_level_block.accept(&mut code_gen);
    
    // Aseguramos que el retorno sea i32 (booleano o número en tu sistema actual)
    let return_val = if result.is_int_value() {
        result.into_int_value()
    } else {
        context.i32_type().const_int(0, false)
    };

    code_gen.builder.build_return(Some(&return_val)).unwrap();

    // 5. Opcionales y ejecución
    if let Some(path) = ir_output {
        code_gen.save_ir(path)?;
    }

    code_gen.compile_and_execute()
}
