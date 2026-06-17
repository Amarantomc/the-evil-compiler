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

fn main() {
    let input = "
    type Line (x,y) {
    x = x;
    y =y;

    DotProduct() => self.x.0 * self.y.0 + self.x.1 * self.y.1;
    SetX(x) => self.x := x;
    GetX() => self.x.0;
}
    let x = new Line((3,4),(5,5)) in 
    {
    print(x.DotProduct());
    };
    ";

    // ---- 1. Parseo --------------------------------------------------------
    let parser = grammar::ProgramParser::new();
    let mut program = match parser.parse(input) {
        Ok(ast) => ast,
        Err(e) => {
            eprintln!("Error de sintaxis: {:?}", e);
            std::process::exit(1);
        }
    };

    // ---- 2. Inferencia de tipos -------------------------------------------
    // El inferidor anota el AST con HulkType concretos y acumula solo errores
    // estructurales (e.g. función no encontrada durante la generación de
    // restricciones, que impediría seguir).
    
let mut inferrer = type_inferrer::TypeInferrer::new();
    inferrer.infer_program(&mut program);

    if !inferrer.inference_errors.is_empty() {
        eprintln!("Errores durante la inferencia de tipos:");
        for e in &inferrer.inference_errors {
            eprintln!("  - {}", e);
        }
        std::process::exit(1);
}

    // ---- 3. Chequeo semántico ---------------------------------------------
    // El checker recibe el entorno construido por el inferidor (jerarquía de
    // tipos, firmas de funciones) y el AST ya anotado, y verifica todas las
    // reglas semánticas sobre los tipos resueltos.
    
    // El entorno se mueve al checker: si necesitaras acceder a él después,
    // añade un campo público o un getter.
    let mut checker = semantic::SemanticChecker::new(inferrer.env);
    checker.check_program(&program);

    if !checker.errors.is_empty() {
        eprintln!("Errores semánticos:");
        for e in &checker.errors {
            eprintln!("  - {}", e);
        }
        std::process::exit(1);
    }

    // ---- 4. Generación de código ------------------------------------------
    println!("Inferencia y chequeo semántico exitosos. El programa es válido.");
    println!("{:#?}", program);

    match codegen::compile_hulk_program(&mut program, "hulk_module", Some("output.ll")) {
        Ok(_)  => println!("LLVM IR generado exitosamente."),
        Err(e) => eprintln!("Error generando IR: {}", e),
    }
}

