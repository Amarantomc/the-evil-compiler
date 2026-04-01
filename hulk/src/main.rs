use lalrpop_util::lalrpop_mod;

pub mod ast;
pub mod expr_visitor;
lalrpop_mod!(grammar);
pub mod codegen;
pub mod codegen_visitor;
fn main() {
    let expr = grammar::ExprParser::new();

    let input = "(2 + 2)* 4 > (5+5)";

    match expr.parse(input) {
        Ok(ast) => {
            println!("AST generado: {:?}\n", ast);

            match codegen::generate_ir_and_execute(&ast, "hulk_module", Some("output.ll")) {
                Ok(result) => println!("Resultado de ejecución JIT: {}", result),
                Err(e) => eprintln!("Error durante codegen/JIT: {}", e),
            }
        }
        Err(e) => eprintln!("Error al parsear: {}", e),
    }
}
