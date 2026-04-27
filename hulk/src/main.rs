use lalrpop_util::lalrpop_mod;

pub mod ast;
pub mod expr_visitor;
lalrpop_mod!(grammar);
pub mod codegen;
pub mod codegen_visitor;
pub mod semantic;
pub  mod  nodes{
    pub mod function_decl_node;
    pub mod literal_node;}
fn main() {
    let input = "let x= \"peseta\", b= 7 + x  in x;";

    // 2. Parsear el código para obtener el AST
    let parser = grammar::ProgramParser::new();
    let mut program = match parser.parse(input) {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("Error de sintaxis: {:?}", e);
            std::process::exit(1);
        }
    };

    // 3. Ejecutar el chequeo semántico
    let mut checker = semantic::SemanticChecker::new();
    checker.check_program(&mut program);

    // 4. Reportar errores o confirmar éxito
    if !checker.errors.is_empty() {
        eprintln!("Se encontraron errores semánticos:");
        for error in &checker.errors {
            eprintln!("- {}", error);
        }
        std::process::exit(1);
    } else {
        println!("Chequeo semántico exitoso. El programa es válido.");
        // Opcional: imprimir el AST procesado
        // println!("{:#?}", program);
    }
}
