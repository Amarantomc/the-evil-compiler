
use crate::nodes::{
    expr_node::{Expr},
    function_decl_node::FunctionDecl,
    type_decl_node::TypeDeclNode,
    program_node::{Program, Statement},
    
};

pub fn promote_program(program: &mut Program) {
    for stmt in &mut program.statements {
        match stmt {
            Statement::FunctionDecl(f) => promote_function(f),
            Statement::TypeDecl(t)     => promote_type(t),
            Statement::Expression(_)   => {}
        }
    }
}

fn promote_function(f: &mut FunctionDecl) {
    let g = f.generics.clone();
    if g.is_empty() { return; }
    for (_, ty) in &mut f.params { *ty = ty.promote_params(&g); }
    f.return_type = f.return_type.promote_params(&g);
    promote_expr(&mut f.body, &g);
}

fn promote_type(t: &mut TypeDeclNode) {
    let g = t.generics.clone();
    if g.is_empty() { return; }
    for (_, ty) in &mut t.params { *ty = ty.promote_params(&g); }
    for a in &mut t.attributes { a.type_annotation = a.type_annotation.promote_params(&g); promote_expr(&mut a.initializer, &g); }
    for m in &mut t.methods {
        // Los métodos heredan los genéricos del tipo (T en scope).
        for (_, ty) in &mut m.params { *ty = ty.promote_params(&g); }
        m.return_type = m.return_type.promote_params(&g);
        promote_expr(&mut m.body, &g);
    }
}

/// Reescribe anotaciones de tipo escritas por el usuario dentro de un cuerpo:
/// let con anotación, type_args de turbofish, e is/as targets se dejan como
/// Class (son nombres concretos), pero las anotaciones `: T` sí se promueven.
fn promote_expr(e: &mut Expr, g: &[String]) {
    match e {
        Expr::Let(n) => {
            for ((_, ty), init) in &mut n.assignments {
                *ty = ty.promote_params(g);
                promote_expr(init, g);
            }
            promote_expr(&mut n.body, g);
        }
        Expr::Binary(n)  => { promote_expr(&mut n.left, g); promote_expr(&mut n.right, g); }
        Expr::Unary(n)   => promote_expr(&mut n.expr, g),
        Expr::If(n)      => {
            promote_expr(&mut n.condition, g);
            promote_expr(&mut n.if_branch, g);
            for (c, b) in &mut n.elif_branches { promote_expr(c, g); promote_expr(b, g); }
            promote_expr(&mut n.else_branch, g);
        }
        Expr::While(n)   => { promote_expr(&mut n.condition, g); promote_expr(&mut n.body, g); }
        Expr::For(n)     => { promote_expr(&mut n.iterator, g); promote_expr(&mut n.body, g); }
        Expr::Block(n)   => for x in &mut n.expressions { promote_expr(x, g); },
        Expr::DestAssign(n) => { promote_expr(&mut n.target, g); promote_expr(&mut n.expr, g); }
        Expr::FunCall(n) | Expr::Instantiation(n) => {
            for ta in &mut n.type_args { *ta = ta.promote_params(g); }
            for a in &mut n.args { promote_expr(a, g); }
        }
        Expr::MemberAccess(n) => promote_expr(&mut n.instance, g),
        Expr::MethodCall(n)   => {
            promote_expr(&mut n.instance, g);
            for ta in &mut n.call.type_args { *ta = ta.promote_params(g); }
            for a in &mut n.call.args { promote_expr(a, g); }
        }
        Expr::BaseCall(args)  => for a in args { promote_expr(a, g); },
        Expr::Tuple(n)        => for x in &mut n.elements { promote_expr(x, g); },
        Expr::TupleAccess(n)  => promote_expr(&mut n.tuple, g),
        Expr::TypeDowncast(n) => promote_expr(&mut n.expr, g),
        Expr::TypeTest(n)     => promote_expr(&mut n.expr, g),
        Expr::Literal(_) | Expr::SelfRef => {}
    }
}