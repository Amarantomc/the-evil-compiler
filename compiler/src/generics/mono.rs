use std::collections::{HashMap, HashSet, VecDeque};
use crate::nodes::{
    expr_node::{Expr, HulkType},
    function_decl_node::FunctionDecl,
    type_decl_node::TypeDeclNode,
    program_node::{Program, Statement},
    literal_node::{Literal, LiteralNode},
};

/// Una instancia solicitada: nombre base + argumentos de tipo concretos.
#[derive(Clone, PartialEq, Eq, Hash)]
struct Request { base: String, args: Vec<String> } // args ya manglados

pub struct Monomorphizer {
    fn_templates:   HashMap<String, FunctionDecl>,
    type_templates: HashMap<String, TypeDeclNode>,
    queue: VecDeque<(String, Vec<HulkType>)>, // (base, args concretos)
    done:  HashSet<String>,                    // nombres manglados ya generados
    out_fns:   Vec<FunctionDecl>,
    out_types: Vec<TypeDeclNode>,
    pub errors: Vec<String>,
}

impl Monomorphizer {
    pub fn new() -> Self {
        Self {
            fn_templates: HashMap::new(),
            type_templates: HashMap::new(),
            queue: VecDeque::new(),
            done: HashSet::new(),
            out_fns: Vec::new(),
            out_types: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Reemplaza in-place `program.statements`: quita plantillas genéricas,
    /// reescribe usos a nombres manglados y agrega las instancias concretas.
    pub fn run(&mut self, program: &mut Program) {
        // 1. Separar plantillas de lo no genérico.
        let mut kept: Vec<Statement> = Vec::new();
        let mut roots: Vec<Statement> = Vec::new(); // funciones/tipos concretos + expresiones top-level
        for stmt in program.statements.drain(..) {
            match stmt {
                Statement::FunctionDecl(f) if f.is_generic() => {
                    self.fn_templates.insert(f.name.value.as_id(), f);
                }
                Statement::TypeDecl(t) if t.is_generic() => {
                    self.type_templates.insert(t.name.value.as_id(), t);
                }
                other => roots.push(other),
            }
        }

        // 2. Sembrar la worklist y reescribir usos en las raíces concretas.
        for stmt in &mut roots {
            match stmt {
                Statement::FunctionDecl(f) => self.scan_expr(&mut f.body),
                Statement::TypeDecl(t)     => {
                    for a in &mut t.attributes { self.scan_expr(&mut a.initializer); }
                    for m in &mut t.methods    { self.scan_expr(&mut m.body); }
                }
                Statement::Expression(e)   => self.scan_expr(e),
            }
        }
        kept.extend(roots);

        // 3. Punto fijo: procesar cada instancia pendiente.
        while let Some((base, args)) = self.queue.pop_front() {
            let mangled = mangle_name(&base, &args);
            if self.done.contains(&mangled) { continue; }
            self.done.insert(mangled.clone());

            if let Some(tpl) = self.fn_templates.get(&base).cloned() {
                let mut inst = specialize_fn(&tpl, &args, &mangled);
                self.scan_expr(&mut inst.body); // descubrir genéricos anidados
                self.out_fns.push(inst);
            } else if let Some(tpl) = self.type_templates.get(&base).cloned() {
                let mut inst = specialize_type(&tpl, &args, &mangled);
                for a in &mut inst.attributes { self.scan_expr(&mut a.initializer); }
                for m in &mut inst.methods    { self.scan_expr(&mut m.body); }
                self.out_types.push(inst);
            } else {
                self.errors.push(format!("Genérico '{}' no declarado.", base));
            }
        }

        // 4. Reensamblar: tipos concretos primero (igual que codegen espera),
        //    luego funciones, luego el resto de `kept`.
        let mut stmts: Vec<Statement> = Vec::new();
        for t in self.out_types.drain(..) { stmts.push(Statement::TypeDecl(t)); }
        for f in self.out_fns.drain(..)   { stmts.push(Statement::FunctionDecl(f)); }
        stmts.extend(kept);
        program.statements = stmts;
    }

    /// Recorre una expresión buscando usos de genéricos. Para cada uno:
    ///   - si tiene type_args concretos -> encola la instancia y reescribe el
    ///     nombre del nodo al manglado, vaciando type_args.
    fn scan_expr(&mut self, e: &mut Expr) {
        match e {
            Expr::FunCall(n) | Expr::Instantiation(n) => {
                let base = n.name.value.as_id();
                let is_generic = self.fn_templates.contains_key(&base)
                              || self.type_templates.contains_key(&base);
                for a in &mut n.args { self.scan_expr(a); }
                if is_generic && !n.type_args.is_empty() {
                    // collapse_generic por si un arg es a su vez Box<Number>.
                    let args: Vec<HulkType> = n.type_args.iter()
                        .map(|t| t.collapse_generic()).collect();
                    if args.iter().any(|t| t.contains_param()) {
                        // dentro de otra plantilla aún no resuelta: lo resolverá
                        // la especialización del padre vía subst_expr.
                        return;
                    }
                    let mangled = mangle_name(&base, &args);
                    self.queue.push_back((base.clone(), args));
                    n.name = LiteralNode::new(Literal::Id(mangled));
                    n.type_args.clear();
                }
            }
            Expr::MethodCall(n) => {
                self.scan_expr(&mut n.instance);
                for a in &mut n.call.args { self.scan_expr(a); }
            }
            Expr::Let(n) => {
                for ((_, _), init) in &mut n.assignments { self.scan_expr(init); }
                self.scan_expr(&mut n.body);
            }
            Expr::Binary(n)  => { self.scan_expr(&mut n.left); self.scan_expr(&mut n.right); }
            Expr::Unary(n)   => self.scan_expr(&mut n.expr),
            Expr::If(n)      => {
                self.scan_expr(&mut n.condition); self.scan_expr(&mut n.if_branch);
                for (c, b) in &mut n.elif_branches { self.scan_expr(c); self.scan_expr(b); }
                self.scan_expr(&mut n.else_branch);
            }
            Expr::While(n)   => { self.scan_expr(&mut n.condition); self.scan_expr(&mut n.body); }
            Expr::For(n)     => { self.scan_expr(&mut n.iterator); self.scan_expr(&mut n.body); }
            Expr::Block(n)   => for x in &mut n.expressions { self.scan_expr(x); },
            Expr::DestAssign(n) => { self.scan_expr(&mut n.target); self.scan_expr(&mut n.expr); }
            Expr::MemberAccess(n) => self.scan_expr(&mut n.instance),
            Expr::BaseCall(a)     => for x in a { self.scan_expr(x); },
            Expr::Tuple(n)        => for x in &mut n.elements { self.scan_expr(x); },
            Expr::TupleAccess(n)  => self.scan_expr(&mut n.tuple),
            Expr::TypeDowncast(n) => self.scan_expr(&mut n.expr),
            Expr::TypeTest(n)     => self.scan_expr(&mut n.expr),
            Expr::Literal(_) | Expr::SelfRef => {}
        }
    }
}

// --- nombres manglados ------------------------------------------------------

fn mangle_name(base: &str, args: &[HulkType]) -> String {
    if args.is_empty() { return base.to_string(); }
    format!("{}__{}", base,
        args.iter().map(|t| t.mangle()).collect::<Vec<_>>().join("_"))
}

// --- especialización de una función -----------------------------------------

fn specialize_fn(tpl: &FunctionDecl, args: &[HulkType], mangled: &str) -> FunctionDecl {
    let map: HashMap<String, HulkType> =
        tpl.generics.iter().cloned().zip(args.iter().cloned()).collect();

    let mut inst = tpl.clone();
    inst.generics = vec![];
    inst.name = LiteralNode::new(Literal::Id(mangled.to_string()));
    for (_, ty) in &mut inst.params { *ty = ty.subst(&map).collapse_generic(); }
    inst.return_type = inst.return_type.subst(&map).collapse_generic();
    subst_expr(&mut inst.body, &map);
    inst
}

// --- especialización de un tipo ---------------------------------------------

fn specialize_type(tpl: &TypeDeclNode, args: &[HulkType], mangled: &str) -> TypeDeclNode {
    let map: HashMap<String, HulkType> =
        tpl.generics.iter().cloned().zip(args.iter().cloned()).collect();

    let mut inst = tpl.clone();
    inst.generics = vec![];
    inst.name = LiteralNode::new(Literal::Id(mangled.to_string()));
    for (_, ty) in &mut inst.params { *ty = ty.subst(&map).collapse_generic(); }
    for a in &mut inst.attributes {
        a.type_annotation = a.type_annotation.subst(&map).collapse_generic();
        subst_expr(&mut a.initializer, &map);
    }
    for m in &mut inst.methods {
        for (_, ty) in &mut m.params { *ty = ty.subst(&map).collapse_generic(); }
        m.return_type = m.return_type.subst(&map).collapse_generic();
        subst_expr(&mut m.body, &map);
    }
    inst
}

/// Sustituye Param->concreto en las ANOTACIONES escritas por el usuario dentro
/// de un cuerpo (let con tipo, type_args de turbofish anidados). Los
/// return_type inferidos se recomputan luego al re-correr el inferidor.
fn subst_expr(e: &mut Expr, map: &HashMap<String, HulkType>) {
    match e {
        Expr::Let(n) => {
            for ((_, ty), init) in &mut n.assignments {
                *ty = ty.subst(map).collapse_generic();
                subst_expr(init, map);
            }
            subst_expr(&mut n.body, map);
        }
        Expr::FunCall(n) | Expr::Instantiation(n) => {
            for ta in &mut n.type_args { *ta = ta.subst(map); } // queda concreto; lo manglará scan_expr
            for a in &mut n.args { subst_expr(a, map); }
        }
        Expr::MethodCall(n) => {
            subst_expr(&mut n.instance, map);
            for ta in &mut n.call.type_args { *ta = ta.subst(map); }
            for a in &mut n.call.args { subst_expr(a, map); }
        }
        Expr::Binary(n)  => { subst_expr(&mut n.left, map); subst_expr(&mut n.right, map); }
        Expr::Unary(n)   => subst_expr(&mut n.expr, map),
        Expr::If(n)      => {
            subst_expr(&mut n.condition, map); subst_expr(&mut n.if_branch, map);
            for (c, b) in &mut n.elif_branches { subst_expr(c, map); subst_expr(b, map); }
            subst_expr(&mut n.else_branch, map);
        }
        Expr::While(n)   => { subst_expr(&mut n.condition, map); subst_expr(&mut n.body, map); }
        Expr::For(n)     => { subst_expr(&mut n.iterator, map); subst_expr(&mut n.body, map); }
        Expr::Block(n)   => for x in &mut n.expressions { subst_expr(x, map); },
        Expr::DestAssign(n) => { subst_expr(&mut n.target, map); subst_expr(&mut n.expr, map); }
        Expr::MemberAccess(n) => subst_expr(&mut n.instance, map),
        Expr::BaseCall(a)     => for x in a { subst_expr(x, map); },
        Expr::Tuple(n)        => for x in &mut n.elements { subst_expr(x, map); },
        Expr::TupleAccess(n)  => subst_expr(&mut n.tuple, map),
        Expr::TypeDowncast(n) => subst_expr(&mut n.expr, map),
        Expr::TypeTest(n)     => subst_expr(&mut n.expr, map),
        Expr::Literal(_) | Expr::SelfRef => {}
    }
}