use std::{fs::File, io::Read, process::exit};

use lalrpop_util::lalrpop_mod;

pub mod expr_visitor;
lalrpop_mod!(grammar);
pub mod codegen;
pub mod codegen_visitor;
pub mod type_inferrer;      // antes: semantic
pub mod semantic;   // nuevo
pub mod nodes {
    pub mod expr_node;
    pub mod function_decl_node;
    pub mod literal_node;
    pub mod destassing_node;
    pub mod block_node;
    pub mod binaryop_node;
    pub mod unaryop_node;
    pub mod for_node;
    pub mod funcall_node;
    pub mod if_node;
    pub mod while_node;
    pub mod let_node;
    pub mod program_node;
    pub mod type_decl_node;
    pub mod member_access_node;
    pub mod instantiation_node;
    pub mod type_downcast_node;
    pub mod type_test_node;
    pub mod tuple_node;
}
pub  mod generics {
    pub mod promote;
    pub mod mono;
}
pub mod lexer {
    pub mod lexer;
    pub mod token;
}
use crate::{errors::{Diagnostic, Phase, from_parse_error}, lexer::lexer::Lexer};
pub mod errors;

/// Imprime un diagnóstico a stderr y termina con el código del contrato.
fn fail(d: Diagnostic) -> ! {
    eprintln!("{}", d.format());
    exit(d.phase.exit_code());
}
 
/// Imprime varios diagnósticos semánticos (una línea por error) y termina con 3.
fn fail_semantic(src: &str, msgs: &[String]) -> ! {
    for m in msgs {
        // Sin spans en el AST todavía -> (0,0), permitido por el contrato.
        eprintln!("{}", Diagnostic::new(Phase::Semantic, 0, 0, m.clone()).format());
    }
    let _ = src;
    exit(Phase::Semantic.exit_code());
}
fn main() {
    let path="test.hulk";
    let mut file = File::open(path).unwrap();
    let mut contents = String::new();
    file.read_to_string(&mut contents).unwrap();

    
    // ---- 2. Léxico + Sintáctico (un único error de LALRPOP) ----------------
    let parser = grammar::ProgramParser::new();
    let mut program = match parser.parse(Lexer::new(&contents)) {
        Ok(ast) => ast,
        Err(e) => fail(from_parse_error(&contents, e)), // decide LEXICAL vs SYNTACTIC
    };
 
    // ---- 3. Semántico: genéricos (promote/mono) + inferencia + checker -----
    generics::promote::promote_program(&mut program);
 
    let mut mono = generics::mono::Monomorphizer::new();
    mono.run(&mut program);
    if !mono.errors.is_empty() {
        fail_semantic(&contents, &mono.errors);
    }
 
    let mut inferrer = type_inferrer::TypeInferrer::new();
    inferrer.infer_program(&mut program);
    if !inferrer.inference_errors.is_empty() {
        fail_semantic(&contents, &inferrer.inference_errors);
    }
 
    let mut checker = semantic::SemanticChecker::new(inferrer.env);
    checker.check_program(&program);
    if !checker.errors.is_empty() {
        for e in &checker.errors {
            let (line, col) = errors::line_col(&contents, e.offset);
            eprintln!("{}", Diagnostic::new(Phase::Semantic, line, col, e.message.clone()).format());
        }
        exit(Phase::Semantic.exit_code());
    }
    // ---- 4. Generación de código ------------------------------------------
    println!("Inferencia y chequeo semántico exitosos. El programa es válido.");
     println!("{:#?}", program);

    match codegen::compile_hulk_program(&mut program, "hulk_module", Some("output.ll")) {
        Ok(_)  => println!("LLVM IR generado exitosamente."),
        Err(e) => eprintln!("Error generando IR: {}", e),
    }
}

