use std::fs;
use std::process::{exit, Command};

use lalrpop_util::lalrpop_mod;

pub mod expr_visitor;
lalrpop_mod!(grammar);
pub mod codegen;
pub mod codegen_visitor;
pub mod type_inferrer;
pub mod semantic;
pub mod errors;
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
pub mod generics {
    pub mod promote;
    pub mod mono;
}
pub mod lexer {
    pub mod lexer;
    pub mod token;
}

use crate::lexer::lexer::Lexer;
use crate::errors::{Diagnostic, Phase, from_parse_error};

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

/// Compila `output.ll` a un ejecutable `./output` (Linux x86_64).
/// Intenta `clang` directamente; si no está, cae a `llc` + `cc`.
fn build_output(ll_path: &str, out_path: &str) -> Result<(), String> {
    // 1) clang acepta .ll directamente y enlaza libc/libm.
    match Command::new("clang").args([ll_path, "-o", out_path, "-lm", "-O2"]).status() {
        Ok(s) if s.success() => return Ok(()),
        Ok(s) => return Err(format!("clang terminó con código {:?}", s.code())),
        Err(_) => { /* clang no disponible: probar llc + cc */ }
    }
    // 2) Fallback: llc -filetype=obj  +  cc.
    let obj = format!("{}.o", out_path);
    let llc = Command::new("llc")
        .args(["-filetype=obj", ll_path, "-o", &obj])
        .status()
        .map_err(|e| format!("no se pudo invocar llc: {}", e))?;
    if !llc.success() {
        return Err(format!("llc terminó con código {:?}", llc.code()));
    }
    let cc = Command::new("cc")
        .args([&obj, "-o", out_path, "-lm"])
        .status()
        .map_err(|e| format!("no se pudo invocar cc: {}", e))?;
    if !cc.success() {
        return Err(format!("cc terminó con código {:?}", cc.code()));
    }
    Ok(())
}

fn main() {
    // ---- 1. Argumentos y lectura del fuente --------------------------------
    let args: Vec<String> = std::env::args().collect();
    let path = match args.get(1) {
        Some(p) => p.clone(),
        None => fail(Diagnostic::new(Phase::Syntactic, 0, 0, "uso: ./hulk <archivo.hulk>")),
    };
    let src = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => fail(Diagnostic::new(Phase::Lexical, 0, 0,
                       format!("no se pudo leer '{}': {}", path, e))),
    };

    // ---- 2. Léxico + Sintáctico (un único error de LALRPOP) ----------------
    let parser =grammar::ProgramParser::new();
    let mut program = match parser.parse(Lexer::new(&src)) {
        Ok(ast) => ast,
        Err(e) => fail(from_parse_error(&src, e)), // decide LEXICAL vs SYNTACTIC
    };

    // ---- 3. Semántico: genéricos (promote/mono) + inferencia + checker -----
    generics::promote::promote_program(&mut program);

    let mut mono = generics::mono::Monomorphizer::new();
    mono.run(&mut program);
    if !mono.errors.is_empty() {
        fail_semantic(&src, &mono.errors);
    }
     let inh_cycles = semantic::detect_inheritance_cycles(&program);
    if !inh_cycles.is_empty() {
        fail_semantic(&src, &inh_cycles);
    }

    let mut inferrer = type_inferrer::TypeInferrer::new();
    inferrer.infer_program(&mut program);
    if !inferrer.inference_errors.is_empty() {
        fail_semantic(&src, &inferrer.inference_errors);
    }

    let mut checker = semantic::SemanticChecker::new(inferrer.env);
    checker.check_program(&program);
    if !checker.errors.is_empty() {
        for e in &checker.errors {
            let (line, col) = errors::line_col(&src, e.offset);
            eprintln!("{}", Diagnostic::new(Phase::Semantic, line, col, e.message.clone()).format());
        }
        exit(Phase::Semantic.exit_code());
    }

    // ---- 4. Codegen -> output.ll -------------------------------------------
    if let Err(e) = codegen::compile_hulk_program(&mut program, "hulk_module", Some("output.ll")) {
        // Error interno (no debería ocurrir si el semántico pasó).
        eprintln!("(0,0) SEMANTIC: error interno de generación de código: {}", e);
        exit(3);
    }

    // ---- 5. output.ll -> ./output ------------------------------------------
    if let Err(e) = build_output("output.ll", "output") {
        eprintln!("(0,0) SEMANTIC: no se pudo generar ./output: {}", e);
        exit(3);
    }

    // Éxito: existe ./output y el código de salida es 0.
    exit(0);
}