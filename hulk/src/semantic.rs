//! # Chequeo semántico para HULK
//!
//! ## Responsabilidad
//!
//! Este módulo recibe el AST **ya anotado** por `TypeInferrer` y verifica todas
//! las reglas semánticas del lenguaje.  No infiere tipos: solo los lee de los
//! nodos y comprueba que sean correctos.
//!
//! ## Reglas verificadas
//!
//! ### Operaciones binarias
//! - Operandos de aritmética (`+`, `-`, `*`, `/`, `%`, `^`) deben ser `Number`.
//! - Operandos de comparación relacional (`<`, `>`, `<=`, `>=`) deben ser `Number`.
//! - Operandos de igualdad (`==`, `!=`) deben tener el mismo tipo.
//! - Operandos lógicos (`&`, `|`) deben ser `Bool`.
//! - Operandos de concatenación (`@`, `@@`) deben conformar a `String`.
//!
//! ### Operaciones unarias
//! - `!expr` → `expr` debe ser `Bool`.
//! - `-expr`, `+expr` → `expr` debe ser `Number`.
//!
//! ### Condicionales
//! - La condición de `if`/`elif`/`while` debe ser `Bool`.
//!
//! ### Variables y asignación
//! - La variable en `:=` debe estar declarada en el scope.
//! - El tipo del valor asignado debe conformar al tipo declarado.
//!
//! ### Funciones
//! - Toda función llamada debe estar declarada.
//! - El número de argumentos debe coincidir con la aridad.
//! - Cada argumento debe conformar al tipo del parámetro correspondiente.
//! - El tipo del cuerpo debe conformar al tipo de retorno declarado.
//!
//! ### Tipos y constructores
//! - Todo tipo instanciado debe estar declarado.
//! - El número de argumentos del constructor debe coincidir.
//! - Cada argumento debe conformar al parámetro correspondiente.
//!
//! ### Acceso a miembros
//! - Solo se puede acceder a campos/métodos sobre valores de tipo clase.
//! - El campo/método debe existir en la clase (o en algún ancestro).
//! - En llamadas a método, la aridad y tipos de argumentos deben ser correctos.
//!
//! ### `self` y `base()`
//! - `self` solo es válido dentro de un método de tipo.
//! - `base()` solo es válido dentro de un método de un tipo que tenga padre.
//! - `base()` solo es válido si el padre define el mismo método.

use std::collections::HashMap;

use crate::{
    nodes::{
        binaryop_node::BinaryOp,
        expr_node::{Expr, HulkType},
        function_decl_node::FunctionDecl,
        literal_node::Literal,
        program_node::{Program, Statement},
        type_decl_node::TypeDeclNode,
        unaryop_node::UnaryOp,
    }, type_inferrer::InferType
    

};
use crate::type_inferrer::Environment;

// ============================================================================
// Chequedor semántico
// ============================================================================

pub struct SemanticChecker {
    /// Errores semánticos encontrados.
    pub errors: Vec<String>,
    /// Referencia al entorno construido por el inferidor (tipos y funciones).
    /// El checker lo recibe tras la inferencia y lo usa en modo solo-lectura
    /// para verificar jerarquía, aridad, etc.
    env: Environment,
    /// Contexto: tipo de `self` en el método actual (`None` si no estamos en uno).
    self_type: Option<String>,
    /// Contexto: nombre del método en curso (para validar `base()`).
    current_method: Option<String>,
    /// Scope de variables locales: nombre → tipo ya resuelto.
    /// Se reconstruye al verificar funciones, tipos y expresiones `let`.
    scopes: Vec<HashMap<String, HulkType>>,
}

impl SemanticChecker {
    /// Crea un checker a partir del entorno producido por `TypeInferrer`.
    pub fn new(env: Environment) -> Self {
        Self {
            errors: Vec::new(),
            env,
            self_type: None,
            current_method: None,
            scopes: vec![HashMap::new()],
        }
    }

    // ========================================================================
    // Punto de entrada público
    // ========================================================================

    /// Verifica el AST ya anotado.  Devuelve `true` si no hay errores.
    pub fn check_program(&mut self, program: &Program) -> bool {
        for stmt in &program.statements {
            match stmt {
                Statement::FunctionDecl(decl) => self.check_function_decl(decl),
                Statement::TypeDecl(decl)     => self.check_type_decl(decl),
                Statement::Expression(expr)   => self.check_expr(expr),
            }
        }
        self.errors.is_empty()
    }

    // ========================================================================
    // Verificación de declaraciones de función
    // ========================================================================

    fn check_function_decl(&mut self, decl: &FunctionDecl) {
        let fn_name = decl.name.value.as_id();

        self.push_scope();
        for (param_node, param_ty) in &decl.params {
            if let Literal::Id(ref name) = param_node.value {
                self.define_var(name.clone(), param_ty.clone());
            }
        }

        self.check_expr(&decl.body);

        // Verificar que el cuerpo conforma al tipo de retorno declarado
        let body_ty = self.type_of(& decl.body);
        let ret_ty  = &decl.return_type;
        if !self.conforms(&body_ty, ret_ty) {
            self.errors.push(format!(
                "Función '{}': el cuerpo tiene tipo {:?} pero se declaró retorno {:?}.",
                fn_name, body_ty, ret_ty
            ));
        }

        self.pop_scope();
    }

    // ========================================================================
    // Verificación de declaraciones de tipo
    // ========================================================================

    fn check_type_decl(&mut self, decl: &TypeDeclNode) {
        let type_name = decl.name.value.as_id();
        self.self_type = Some(type_name.clone());

        // ---- Atributos ----
        self.push_scope();
        // Cargar parámetros del constructor en el scope
        for (param_node, param_ty) in &decl.params {
            if let Literal::Id(ref name) = param_node.value {
                self.define_var(name.clone(), param_ty.clone());
            }
        }

        for attr in &decl.attributes {
            self.check_expr(&attr.initializer);

            let init_ty = self.type_of(&attr.initializer);
            let attr_ty = &attr.type_annotation;

            if *attr_ty != HulkType::Unknown && !self.conforms(&init_ty, attr_ty) {
                let attr_name = attr.name.value.as_id();
                self.errors.push(format!(
                    "Atributo '{}::{}': el inicializador tiene tipo {:?} pero se declaró {:?}.",
                    type_name, attr_name, init_ty, attr_ty
                ));
            }
        }
        self.pop_scope();

        // ---- Métodos ----
        for method in &decl.methods {
            let method_name = method.name.value.as_id();
            self.current_method = Some(method_name.clone());

            self.push_scope();
            self.define_var("self".to_string(), HulkType::Class(type_name.clone()));

            for (param_node, param_ty) in &method.params {
                if let Literal::Id(ref name) = param_node.value {
                    self.define_var(name.clone(), param_ty.clone());
                }
            }

            self.check_expr(&method.body);

            let body_ty = self.type_of(&method.body);
            let ret_ty  = &method.return_type;

            if *ret_ty != HulkType::Unknown && !self.conforms(&body_ty, ret_ty) {
                self.errors.push(format!(
                    "Método '{}::{}': el cuerpo tiene tipo {:?} pero se declaró retorno {:?}.",
                    type_name, method_name, body_ty, ret_ty
                ));
            }

            self.pop_scope();
            self.current_method = None;
        }

        self.self_type = None;
    }

    // ========================================================================
    // Verificación de expresiones (recorrido del AST)
    // ========================================================================

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Literal(_) => {}
            Expr::SelfRef => {
                if self.self_type.is_none() {
                    self.errors.push(
                        "Error semántico: 'self' usado fuera de un tipo.".to_string()
                    );
                }
            }
            Expr::Binary(node) => {
                self.check_expr(&node.left);
                self.check_expr(&node.right);

                let lt = self.type_of(&node.left);
                let rt = self.type_of(&node.right);

                match &node.op {
                    BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul
                    | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
                        self.expect_type(&lt, &HulkType::Number, "operando izquierdo de aritmética");
                        self.expect_type(&rt, &HulkType::Number, "operando derecho de aritmética");
                    }
                    BinaryOp::Great | BinaryOp::Less
                    | BinaryOp::Gequa | BinaryOp::Lequa => {
                        self.expect_type(&lt, &HulkType::Number, "operando izquierdo de comparación");
                        self.expect_type(&rt, &HulkType::Number, "operando derecho de comparación");
                    }
                    BinaryOp::Equal | BinaryOp::Dist => {
                        if !self.conforms(&lt, &rt) && !self.conforms(&rt, &lt) {
                            self.errors.push(format!(
                                "Error de tipo: los operandos de '==' / '!=' tienen tipos incompatibles: {:?} y {:?}.",
                                lt, rt
                            ));
                        }
                    }
                    BinaryOp::And | BinaryOp::Or => {
                        self.expect_type(&lt, &HulkType::Bool, "operando izquierdo lógico");
                        self.expect_type(&rt, &HulkType::Bool, "operando derecho lógico");
                    }
                    BinaryOp::SingleConc | BinaryOp::SpacedConc => {
                        self.expect_conforms(&lt, &HulkType::String, "operando izquierdo de concatenación");
                        self.expect_conforms(&rt, &HulkType::String, "operando derecho de concatenación");
                    }
                }
            }
            Expr::Unary(node) => {
                self.check_expr(&node.expr);
                let t = self.type_of(&node.expr);
                match &node.op {
                    UnaryOp::Not => {
                        self.expect_type(&t, &HulkType::Bool, "operando de '!'");
                    }
                    UnaryOp::Neg | UnaryOp::Plus => {
                        self.expect_type(&t, &HulkType::Number, "operando de negación/positivo");
                    }
                }
            }
            Expr::Let(node) => {
                let mut scopes_opened = 0usize;

                for ((id_node, decl_ty), init_expr) in &node.assignments {
                    self.check_expr(init_expr);

                    let init_ty = self.type_of(init_expr);

                    // Si hay anotación explícita, verificar conformidad
                    if *decl_ty != HulkType::Unknown && !self.conforms(&init_ty, decl_ty) {
                        let var_name = id_node.value.as_id();
                        self.errors.push(format!(
                            "Variable '{}': el inicializador tiene tipo {:?} pero se declaró {:?}.",
                            var_name, init_ty, decl_ty
                        ));
                    }

                    // El tipo efectivo de la variable es el declarado (si existe)
                    // o el inferido del inicializador
                    let effective_ty = if *decl_ty != HulkType::Unknown {
                        decl_ty.clone()
                    } else {
                        init_ty
                    };

                    self.push_scope();
                    scopes_opened += 1;
                    if let Literal::Id(ref name) = id_node.value {
                        self.define_var(name.clone(), effective_ty);
                    }
                }

                self.check_expr(&node.body);

                for _ in 0..scopes_opened { self.pop_scope(); }
            }
            Expr::If(node) => {
                self.check_expr(&node.condition);
                let cond_ty = self.type_of(&node.condition);
                self.expect_type(&cond_ty, &HulkType::Bool, "condición de 'if'");

                self.check_expr(&node.if_branch);

                for (elif_cond, elif_body) in &node.elif_branches {
                    self.check_expr(elif_cond);
                    let ec_ty = self.type_of(elif_cond);
                    self.expect_type(&ec_ty, &HulkType::Bool, "condición de 'elif'");
                    self.check_expr(elif_body);
                }

                self.check_expr(&node.else_branch);
            }
            Expr::While(node) => {
                self.check_expr(&node.condition);
                let cond_ty = self.type_of(&node.condition);
                self.expect_type(&cond_ty, &HulkType::Bool, "condición de 'while'");
                self.check_expr(&node.body);
            }
            Expr::For(node) => {
                self.check_expr(&node.iterator);
                self.push_scope();
                if let Literal::Id(ref var_name) = node.variable.value {
                    self.define_var(var_name.clone(), HulkType::Number);
                }
                self.check_expr(&node.body);
                self.pop_scope();
            }
            Expr::Block(node) => {
                for e in &node.expressions { self.check_expr(e); }
            }
            Expr::FunCall(node) => {
                for arg in &node.args { self.check_expr(arg); }

                if let Literal::Id(ref fn_name) = node.name.value {
                    match self.env.functions.get(fn_name).cloned() {
                        None => {
                            self.errors.push(format!(
                                "Error semántico: función '{}' no declarada.", fn_name
                            ));
                        }
                        Some((param_tys, _ret_ty)) => {
                            let variadic = param_tys.len() == 1
                                && param_tys[0] == InferType::unknown();

                            if !variadic {
                                if param_tys.len() != node.args.len() {
                                    self.errors.push(format!(
                                        "Error de tipo: '{}' espera {} argumento(s), se dieron {}.",
                                        fn_name, param_tys.len(), node.args.len()
                                    ));
                                } else {
                                    for (arg, param_ty) in node.args.iter().zip(param_tys.iter()) {
                                        let arg_ty = self.type_of(arg);
                                        let expected = self.resolve_infer(param_ty);
                                        if !self.conforms(&arg_ty, &expected) {
                                            self.errors.push(format!(
                                                "Error de tipo en llamada a '{}': se esperaba {:?} pero se obtuvo {:?}.",
                                                fn_name, expected, arg_ty
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Expr::Instantiation(node) => {
                for arg in &node.args { self.check_expr(arg); }

                if let Literal::Id(ref type_name) = node.name.value {
                    match self.env.types.get(type_name).cloned() {
                        None => {
                            self.errors.push(format!(
                                "Error semántico: tipo '{}' no declarado.", type_name
                            ));
                        }
                        Some(ti) => {
                            let effective_params: Vec<_> = if ti.params.is_empty() {
                                ti.parent.as_ref()
                                    .and_then(|p| self.env.types.get(p))
                                    .map(|pt| pt.params.clone())
                                    .unwrap_or_default()
                            } else {
                                ti.params.clone()
                            };

                            if !effective_params.is_empty()
                                && node.args.len() != effective_params.len()
                            {
                                self.errors.push(format!(
                                    "Error de tipo: constructor de '{}' espera {} arg(s), se dieron {}.",
                                    type_name, effective_params.len(), node.args.len()
                                ));
                            } else {
                                for (arg, param_ty) in node.args.iter().zip(effective_params.iter()) {
                                    let arg_ty  = self.type_of(arg);
                                    let expected = self.resolve_infer(param_ty);
                                    if !self.conforms(&arg_ty, &expected) {
                                        self.errors.push(format!(
                                            "Error de tipo en constructor de '{}': se esperaba {:?} pero se obtuvo {:?}.",
                                            type_name, expected, arg_ty
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Expr::DestAssign(node) => {
                self.check_expr(&node.expr);
                let val_ty = self.type_of(&node.expr);

                match node.target.as_ref() {
                    Expr::Literal(lit) => {
                        if let Literal::Id(ref var_name) = lit.value {
                            match self.lookup_var(var_name) {
                                None => {
                                    self.errors.push(format!(
                                        "Error semántico: variable '{}' no declarada en ':='.", var_name
                                    ));
                                }
                                Some(declared_ty) => {
                                    if !self.conforms(&val_ty, &declared_ty) {
                                        self.errors.push(format!(
                                            "Error de tipo en ':=': variable '{}' es {:?} pero se asigna {:?}.",
                                            var_name, declared_ty, val_ty
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    Expr::MemberAccess(ma) => {
                        self.check_expr(&ma.instance);
                        let inst_ty = self.type_of(&ma.instance);
                        if let HulkType::Class(ref tn) = inst_ty {
                            if let Literal::Id(ref field_name) = ma.member.value {
                                match self.env.lookup_field(tn, field_name) {
                                    None => {
                                        self.errors.push(format!(
                                            "Error semántico: '{}' no tiene atributo '{}'.", tn, field_name
                                        ));
                                    }
                                    Some(fi) => {
                                        let field_ty = self.resolve_infer(&fi.infer_type);
                                        if !self.conforms(&val_ty, &field_ty) {
                                            self.errors.push(format!(
                                                "Error de tipo: asignación a '{}::{}' espera {:?} pero se obtuvo {:?}.",
                                                tn, field_name, field_ty, val_ty
                                            ));
                                        }
                                    }
                                }
                            }
                        } else if inst_ty != HulkType::Unknown {
                            self.errors.push(format!(
                                "Error semántico: acceso a miembro sobre tipo no-clase {:?}.", inst_ty
                            ));
                        }
                    }
                    _ => {
                        self.errors.push(
                            "Error semántico: el lado izquierdo de ':=' debe ser una variable o un campo.".to_string()
                        );
                    }
                }
            }
            Expr::MemberAccess(node) => {
                self.check_expr(&node.instance);
                let inst_ty = self.type_of(&node.instance);

                match &inst_ty {
                    HulkType::Class(tn) => {
                        if let Literal::Id(ref field_name) = node.member.value {
                            if self.env.lookup_field(tn, field_name).is_none() {
                                self.errors.push(format!(
                                    "Error semántico: '{}' no tiene atributo '{}'.", tn, field_name
                                ));
                            }
                        }
                    }
                    HulkType::Unknown => {} // silencioso: ya fallaron otros checks
                    other => {
                        self.errors.push(format!(
                            "Error semántico: acceso a miembro sobre tipo primitivo {:?}.", other
                        ));
                    }
                }
            }
            Expr::MethodCall(node) => {
                self.check_expr(&node.instance);
                for arg in &node.call.args { self.check_expr(arg); }

                let inst_ty = self.type_of(&node.instance);

                match &inst_ty {
                    HulkType::Class(tn) => {
                        let method_name = node.call.name.value.as_id();
                        match self.env.lookup_method(tn, &method_name) {
                            None => {
                                self.errors.push(format!(
                                    "Error semántico: '{}' no tiene método '{}'.", tn, method_name
                                ));
                            }
                            Some(mi) => {
                                if mi.param_types.len() != node.call.args.len() {
                                    self.errors.push(format!(
                                        "Error de tipo: '{}::{}' espera {} arg(s), se dieron {}.",
                                        tn, method_name, mi.param_types.len(), node.call.args.len()
                                    ));
                                } else {
                                    for (arg, param_ty) in
                                        node.call.args.iter().zip(mi.param_types.iter())
                                    {
                                        let arg_ty  = self.type_of(arg);
                                        let expected = self.resolve_infer(param_ty);
                                        if !self.conforms(&arg_ty, &expected) {
                                            self.errors.push(format!(
                                                "Error de tipo en '{}::{}': se esperaba {:?} pero se obtuvo {:?}.",
                                                tn, method_name, expected, arg_ty
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    HulkType::Unknown => {}
                    other => {
                        self.errors.push(format!(
                            "Error semántico: llamada a método sobre tipo primitivo {:?}.", other
                        ));
                    }
                }
            }
            Expr::BaseCall(args) => {
                for arg in args { self.check_expr(arg); }

                let self_type = match &self.self_type {
                    Some(t) => t.clone(),
                    None => {
                        self.errors.push("'base()' fuera de un tipo.".to_string());
                        return;
                    }
                };
                let method_name = match &self.current_method {
                    Some(m) => m.clone(),
                    None => {
                        self.errors.push("'base()' fuera de un método.".to_string());
                        return;
                    }
                };

                let parent_name = match self.env.types.get(&self_type)
                    .and_then(|ti| ti.parent.clone())
                {
                    Some(p) => p,
                    None => {
                        self.errors.push(format!(
                            "'base()' en '{}' que no tiene padre.", self_type
                        ));
                        return;
                    }
                };

                if self.env.lookup_method(&parent_name, &method_name).is_none() {
                    self.errors.push(format!(
                        "El padre '{}' no tiene método '{}'.", parent_name, method_name
                    ));
                }
            }
            Expr::TypeDowncast(node) => {
                self.check_expr(&node.expr);
 
                let expr_ty = self.type_of(&node.expr);
                let target_name = node.target_type.value.as_id();
 
                // Rule 1: source must be a class type.
                match &expr_ty {
                    HulkType::Class(_) | HulkType::Unknown => {}
                    other => {
                        self.errors.push(format!(
                            "Error semántico: el operador 'as' no puede aplicarse a tipo primitivo {:?}.",
                            other
                        ));
                        return; // no further checks make sense
                    }
                }
 
                // Rule 2: target type must exist.
                if !self.env.types.contains_key(&target_name) {
                    self.errors.push(format!(
                        "Error semántico: el tipo '{}' usado en 'as' no está declarado.",
                        target_name
                    ));
                    return;
                }
 
                // Rule 3: source and target must be related (one is an
                // ancestor of the other).
                if let HulkType::Class(ref source_name) = expr_ty {
                    let source_is_ancestor_of_target =
                        self.env.is_subtype(&target_name, source_name);
                    let target_is_ancestor_of_source =
                        self.env.is_subtype(source_name, &target_name);
 
                    if !source_is_ancestor_of_target && !target_is_ancestor_of_source {
                        self.errors.push(format!(
                            "Error semántico: 'as' entre tipos no relacionados: '{}' y '{}'.",
                            source_name, target_name
                        ));
                    }
                }
                // If expr_ty is Unknown we skip Rule 3 silently — the type
                // inferrer could not determine the source type, and the
                // runtime will handle it.
            }
        ,
        Expr::TypeTest(node) => {
            self.check_expr(&node.expr);

            let expr_ty = self.type_of(&node.expr);

            // Rule 1: source must be a class type (or Unknown to allow
            // partially-inferred programs to proceed).
            match &expr_ty {
                HulkType::Class(_) | HulkType::Unknown => {}
                other => {
                    self.errors.push(format!(
                        "Error semántico: el operador 'is' no puede aplicarse a tipo primitivo {:?}.",
                        other
                    ));
                }
            }

            // Rule 2: target type must exist.
            let target_name = node.target_type.value.as_id();
            if !self.env.types.contains_key(&target_name) {
                self.errors.push(format!(
                    "Error semántico: el tipo '{}' usado en 'is' no está declarado.",
                    target_name
                ));
            }
        },
        Expr::Tuple(node) => {
            for e in &node.elements { self.check_expr(e); }
        }
        Expr::TupleAccess(node) => {
            self.check_expr(&node.tuple);
            let tuple_ty = self.type_of(&node.tuple);
            match &tuple_ty {
                HulkType::Tuple(elems) => {
                    if node.index >= elems.len() {
                        self.errors.push(format!(
                            "Error semántico: índice de tupla {} fuera de rango (la tupla tiene {} elementos).",
                            node.index, elems.len()
                        ));
                    }
                }
                HulkType::Unknown => self.errors.push(format!("Error semántico : inferencia de tipo no resuelta en el simbolo del indice {} ", node.index)), 
                other => {
                    self.errors.push(format!(
                        "Error semántico: acceso por índice sobre tipo no-tupla {:?}.", other
                    ));
                }
            }
         
        }
    }
}

    // ========================================================================
    // Helpers de scope
    // ========================================================================

    fn push_scope(&mut self) { self.scopes.push(HashMap::new()); }
    fn pop_scope(&mut self)  { self.scopes.pop(); }

    fn define_var(&mut self, name: String, ty: HulkType) {
        if let Some(s) = self.scopes.last_mut() { s.insert(name, ty); }
    }

    fn lookup_var(&self, name: &str) -> Option<HulkType> {
        for scope in self.scopes.iter().rev() {
            if let Some(t) = scope.get(name) { return Some(t.clone()); }
        }
        None
    }

    // ========================================================================
    // Helpers de tipos
    // ========================================================================

    /// Devuelve el tipo de una expresión ya anotada.
    fn type_of(&self, expr: &Expr) -> HulkType {
        match expr {
            Expr::Literal(n) => match &n.value {
                Literal::Number(_) => HulkType::Number,
                Literal::Bool(_)   => HulkType::Bool,
                Literal::Str(_)    => HulkType::String,
                Literal::Id(name)  => self.lookup_var(name)
                    .unwrap_or(HulkType::Unknown),
            },
            Expr::SelfRef        => self.self_type.as_ref()
                .map(|n| HulkType::Class(n.clone()))
                .unwrap_or(HulkType::Unknown),
            Expr::Binary(n)        => n.return_type.clone(),
            Expr::Unary(n)         => n.return_type.clone(),
            Expr::Let(n)           => n.return_type.clone(),
            Expr::If(n)            => n.return_type.clone(),
            Expr::While(n)         => n.return_type.clone(),
            Expr::For(n)           => n.return_type.clone(),
            Expr::Block(n)         => n.return_type.clone(),
            Expr::FunCall(n)       => n.return_type.clone(),
            Expr::Instantiation(n) => n.return_type.clone(),
            Expr::DestAssign(n)    => n.return_type.clone(),
            Expr::MemberAccess(n)  => n.return_type.clone(),
            Expr::MethodCall(n)    => n.return_type.clone(),
            Expr::BaseCall(_)      => HulkType::Unknown,
            Expr::TypeDowncast(n) => n.return_type.clone(),
            Expr::TypeTest(n) => n.return_type.clone(),
            Expr::Tuple(n) => n.return_type.clone(),
            Expr::TupleAccess(n) => n.return_type.clone(),
        }
    }

    /// Convierte un `InferType` (del entorno del inferidor) a `HulkType`.
    fn resolve_infer(&self, t: &InferType) -> HulkType {
        match t {
            InferType::Concrete(h) => h.clone(),
            InferType::Var(_)      => HulkType::Unknown,
        }
    }

    /// `true` si `sub` conforma a `sup` (delegado al entorno del inferidor).
    fn conforms(&self, sub: &HulkType, sup: &HulkType) -> bool {
        self.env.conforms_concrete(sub, sup)
    }

    /// Emite un error si `actual` no es exactamente `expected`.
    fn expect_type(&mut self, actual: &HulkType, expected: &HulkType, ctx: &str) {
        if actual != &HulkType::Unknown && actual != expected {
            self.errors.push(format!(
                "Error de tipo en {}: se esperaba {:?} pero se obtuvo {:?}.",
                ctx, expected, actual
            ));
        }
    }

    /// Emite un error si `actual` no conforma a `expected`.
    fn expect_conforms(&mut self, actual: &HulkType, expected: &HulkType, ctx: &str) {
        if actual != &HulkType::Unknown && !self.conforms(actual, expected) {
            self.errors.push(format!(
                "Error de tipo en {}: {:?} no conforma a {:?}.",
                ctx, actual, expected
            ));
        }
    }
}