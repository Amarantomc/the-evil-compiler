use lalrpop_util::lalrpop_mod; 

pub mod ast;
lalrpop_mod!(grammar); 

fn main() {

    let expr = grammar::ExprParser::new();
    
    let resultado = expr.parse("(2 + 2)* 4").unwrap();
    println!("El resultado es: {}", format!("{:?}", resultado));
}
