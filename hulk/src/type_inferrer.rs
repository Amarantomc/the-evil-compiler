//! # Inferidor de tipos para HULK — enfoque por restricciones + worklist
//!
//! ## Responsabilidad
//!
//! Este módulo **solo infiere tipos**: recorre el AST, genera restricciones,
//! las resuelve por unificación y anota cada nodo con el `HulkType` concreto
//! aprendido.  **No emite errores semánticos** (eso es responsabilidad de
//! `SemanticChecker`).  Solo registra errores estructurales irrecuperables que
//! impiden continuar la inferencia (e.g. función no declarada al momento de
//! generar restricciones).
//!
//! ## Etapas
//!
//! 1. **Registro de declaraciones** — firma estática de tipos y funciones.
//! 2. **Generación de restricciones** — recorrido bottom-up del AST.
//! 3. **Resolución iterativa** — worklist + unificación hasta punto fijo.
//! 4. **Anotación del AST** — sustituir `TypeVar` por el tipo concreto inferido.

use std::collections::{HashMap, VecDeque};

use crate::{
    expr_visitor::ExprVisitor,
    nodes::{
        binaryop_node::BinaryOp, block_node::BlockNode, destassing_node::DestAssignNode, expr_node::{Expr, HulkType}, for_node::ForNode, funcall_node::FunCallNode, function_decl_node::FunctionDecl, if_node::IfNode, let_node::LetNode, literal_node::Literal, member_access_node::{MemberAccessNode, MethodCallNode}, program_node::{Program, Statement}, tuple_node::{TupleAccessNode, TupleNode}, type_decl_node::TypeDeclNode, type_test_node::TypeTestNode, unaryop_node::UnaryOp, while_node::WhileNode
    },
};

// ============================================================================
// Tipo interno del inferidor
// ============================================================================

/// Tipo interno usado durante la inferencia.
/// Las posiciones sin anotar se representan con `InferType::Var(id)`.
#[derive(Debug, Clone, PartialEq)]
pub enum InferType {
    /// Tipo primitivo o de clase concreto.
    Concrete(HulkType),
    /// Variable de tipo fresca: representa un tipo aún desconocido.
    Var(u32),
}

impl InferType {
    pub fn number() -> Self { InferType::Concrete(HulkType::Number) }
    pub fn bool_t() -> Self { InferType::Concrete(HulkType::Bool) }
    pub fn string() -> Self { InferType::Concrete(HulkType::String) }
    pub fn class(name: &str) -> Self { InferType::Concrete(HulkType::Class(name.to_string())) }
    pub fn unknown() -> Self { InferType::Concrete(HulkType::Unknown) }

    fn from_hulk(h: &HulkType) -> Self {
        match h {
            HulkType::Unknown => InferType::Concrete(HulkType::Unknown),
            other => InferType::Concrete(other.clone()),
        }
    }

    pub fn is_var(&self) -> bool { matches!(self, InferType::Var(_)) }
}

// ============================================================================
// Restricciones
// ============================================================================

#[derive(Debug, Clone)]
pub enum Constraint {
    /// Los dos tipos deben ser iguales (unificación exacta).
    Eq(InferType, InferType),
    /// `lhs` debe conformar (ser subtipo) de `rhs`.
    Conform(InferType, InferType),
    TupleProject(InferType, usize, InferType),
}

// ============================================================================
// Sustitución (union-find plano)
// ============================================================================

#[derive(Default, Debug)]
pub struct Substitution {
    map: HashMap<u32, InferType>,
}

impl Substitution {
    /// Sigue la cadena de sustituciones hasta un tipo no-Var o Var sin mapeo.
    pub fn apply(&self, t: &InferType) -> InferType {
        match t {
            InferType::Var(id) => match self.map.get(id) {
                Some(t2) if t2 != t => self.apply(t2),
                _ => t.clone(),
            },
            other => other.clone(),
        }
    }

    /// Registra `var → ty`.  No pisa un binding concreto existente.
    pub fn bind(&mut self, var: u32, ty: InferType) -> bool {
        if InferType::Var(var) == ty { return false; }
        if let Some(existing) = self.map.get(&var) {
            if !existing.is_var() { return false; }
        }
        self.map.insert(var, ty);
        true
    }

    pub fn apply_to_constraint(&self, c: &Constraint) -> Constraint {
        match c {
            Constraint::Eq(a, b)     => Constraint::Eq(self.apply(a), self.apply(b)),
            Constraint::Conform(a, b) => Constraint::Conform(self.apply(a), self.apply(b)),
            Constraint::TupleProject(t, idx, r) => Constraint::TupleProject(self.apply(t), *idx, self.apply(r)),
        }
    }
}

// ============================================================================
// Información semántica compartida con el checker
// ============================================================================

#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub name: String,
    pub infer_type: InferType,
}

#[derive(Clone, Debug)]
pub struct MethodInfo {
    pub name: String,
    pub param_types: Vec<InferType>,
    pub return_type: InferType,
}

#[derive(Clone, Debug)]
pub struct TypeInfo {
    pub name: String,
    pub params: Vec<InferType>,
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub parent: Option<String>,
}

// ============================================================================
// Entorno
// ============================================================================

pub struct Environment {
    scopes: Vec<HashMap<String, InferType>>,
    pub functions: HashMap<String, (Vec<InferType>, InferType)>,
    pub types: HashMap<String, TypeInfo>,
    pub self_type: Option<String>,
    pub current_method: Option<String>,
}

impl Environment {
    pub fn new() -> Self {
        let mut env = Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            types: HashMap::new(),
            self_type: None,
            current_method: None,
        };
        env.register_builtins();
        env
    }

    fn register_builtins(&mut self) {
        let num = || InferType::number();
        let unk = || InferType::unknown();
        self.functions.insert("print".into(), (vec![unk()], unk()));
        self.functions.insert("sqrt".into(),  (vec![num()], num()));
        self.functions.insert("sin".into(),   (vec![num()], num()));
        self.functions.insert("cos".into(),   (vec![num()], num()));
        self.functions.insert("exp".into(),   (vec![num()], num()));
        self.functions.insert("log".into(),   (vec![num(), num()], num()));
        self.functions.insert("rand".into(),  (vec![], num()));
        self.functions.insert("range".into(), (vec![num(), num()], num()));
    }

    pub fn push_scope(&mut self) { self.scopes.push(HashMap::new()); }
    pub fn pop_scope(&mut self)  { self.scopes.pop(); }

    pub fn define(&mut self, name: String, t: InferType) {
        if let Some(s) = self.scopes.last_mut() { s.insert(name, t); }
    }

    pub fn lookup(&self, name: &str) -> Option<&InferType> {
        for scope in self.scopes.iter().rev() {
            if let Some(t) = scope.get(name) { return Some(t); }
        }
        None
    }

    pub fn assign(&mut self, name: &str, t: InferType) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), t);
                return true;
            }
        }
        false
    }

    // ------------------------------------------------------------------
    // Relación de conformidad (subtipado)
    // ------------------------------------------------------------------

    pub fn conforms_concrete(&self, sub: &HulkType, sup: &HulkType) -> bool {
        if sub == sup { return true; }
        if matches!(sub, HulkType::Unknown) || matches!(sup, HulkType::Unknown) {
            return true;
        }
        match (sub, sup) {
            (HulkType::Number, _) | (_, HulkType::Number) => return false,
            (HulkType::String, _) | (_, HulkType::String) => return false,
            (HulkType::Bool,   _) | (_, HulkType::Bool)   => return false,
            _ => {}
        }
        if let (HulkType::Class(s), HulkType::Class(p)) = (sub, sup) {
            return self.is_subtype(s, p);
        }
        false
    }

    pub fn is_subtype(&self, child: &str, ancestor: &str) -> bool {
        if child == ancestor { return true; }
        if let Some(info) = self.types.get(child) {
            if let Some(ref parent) = info.parent {
                return self.is_subtype(parent, ancestor);
            }
        }
        false
    }

    /// Ancestro común más bajo de dos tipos concretos.
    pub fn lca(&self, a: &HulkType, b: &HulkType) -> HulkType {
        if a == b { return a.clone(); }
        if matches!(a, HulkType::Unknown) || matches!(b, HulkType::Unknown) {
            return HulkType::Unknown;
        }
        if self.conforms_concrete(a, b) { return b.clone(); }
        if self.conforms_concrete(b, a) { return a.clone(); }
        let a_ancs = self.ancestors(a);
        for anc in self.ancestors(b) {
            if a_ancs.contains(&anc) { return HulkType::Class(anc); }
        }
        HulkType::Unknown
    }

    fn ancestors(&self, t: &HulkType) -> Vec<String> {
        let mut res = Vec::new();
        if let HulkType::Class(name) = t {
            let mut cur = name.clone();
            loop {
                res.push(cur.clone());
                match self.types.get(&cur).and_then(|i| i.parent.clone()) {
                    Some(p) => cur = p,
                    None    => break,
                }
            }
        }
        res
    }

    // ------------------------------------------------------------------
    // Búsqueda de miembros en la jerarquía
    // ------------------------------------------------------------------

    pub fn lookup_field(&self, type_name: &str, field: &str) -> Option<FieldInfo> {
        if let Some(ti) = self.types.get(type_name) {
            for f in &ti.fields {
                if f.name == field { return Some(f.clone()); }
            }
            if let Some(ref p) = ti.parent {
                return self.lookup_field(p, field);
            }
        }
        None
    }

    pub fn lookup_method(&self, type_name: &str, method: &str) -> Option<MethodInfo> {
        if let Some(ti) = self.types.get(type_name) {
            for m in &ti.methods {
                if m.name == method { return Some(m.clone()); }
            }
            if let Some(ref p) = ti.parent {
                return self.lookup_method(p, method);
            }
        }
        None
    }

    pub fn update_method_return(
        &mut self,
        type_name: &str,
        method_name: &str,
        new_ret: InferType,
    ) {
        if let Some(ti) = self.types.get_mut(type_name) {
            for m in &mut ti.methods {
                if m.name == method_name { m.return_type = new_ret; return; }
            }
        }
    }

    pub fn update_field(
        &mut self,
        type_name: &str,
        field_name: &str,
        new_ty: InferType,
    ) {
        if let Some(ti) = self.types.get_mut(type_name) {
            for f in &mut ti.fields {
                if f.name == field_name { f.infer_type = new_ty; return; }
            }
        }
    }
}

// ============================================================================
// Generador de variables de tipo frescas
// ============================================================================

struct VarGen(u32);

impl VarGen {
    fn new() -> Self { VarGen(0) }

    fn fresh(&mut self) -> InferType {
        let id = self.0;
        self.0 += 1;
        InferType::Var(id)
    }

    /// Convierte una anotación del AST: `Unknown` → Var fresca; concreto → Concrete.
    fn from_annotation(&mut self, h: &HulkType) -> InferType {
        if *h == HulkType::Unknown { self.fresh() } else { InferType::from_hulk(h) }
    }
}

// ============================================================================
// Inferidor principal
// ============================================================================

pub struct TypeInferrer {
    pub env: Environment,
    var_gen: VarGen,
    constraints: Vec<Constraint>,
    pub subst: Substitution,
    /// Errores estructurales que impiden continuar la inferencia.
    /// Los errores semánticos (conformidad, tipos incompatibles) se detectan
    /// en `SemanticChecker`, no aquí.
    pub inference_errors: Vec<String>,
    changed: bool,
}

impl TypeInferrer {
    pub fn new() -> Self {
        Self {
            env: Environment::new(),
            var_gen: VarGen::new(),
            constraints: Vec::new(),
            subst: Substitution::default(),
            inference_errors: Vec::new(),
            changed: false,
        }
    }

    // ========================================================================
    // Punto de entrada público
    // ========================================================================

    /// Ejecuta las cuatro etapas de inferencia y anota el AST.
    /// Devuelve `true` si no hubo errores estructurales (no semánticos).
    pub fn infer_program(&mut self, program: &mut Program) -> bool {
        self.register_declarations(program);

        for stmt in program.statements.iter_mut() {
            match stmt {
                Statement::FunctionDecl(decl) => self.gen_function_decl(decl),
                Statement::TypeDecl(decl)     => self.gen_type_decl(decl),
                Statement::Expression(expr)   => { expr.accept(self); }
            }
        }

        self.solve_constraints();

        for stmt in program.statements.iter_mut() {
            match stmt {
                Statement::FunctionDecl(decl) => self.annotate_function_decl(decl),
                Statement::TypeDecl(decl)     => self.annotate_type_decl(decl),
                Statement::Expression(expr)   => { self.annotate_expr(expr); }
            }
        }

        self.inference_errors.is_empty()
    }

    // ========================================================================
    // Etapa 1 — Registro de declaraciones
    // ========================================================================

    fn register_declarations(&mut self, program: &Program) {
        for stmt in &program.statements {
            if let Statement::TypeDecl(decl) = stmt { self.register_type(decl); }
        }
        for stmt in &program.statements {
            if let Statement::FunctionDecl(decl) = stmt { self.register_function(decl); }
        }
    }

    fn register_type(&mut self, decl: &TypeDeclNode) {
        let name = decl.name.value.as_id();

        let params: Vec<InferType> = decl.params.iter()
            .map(|(_, t)| self.var_gen.from_annotation(t))
            .collect();

        let fields: Vec<FieldInfo> = decl.attributes.iter()
            .map(|attr| FieldInfo {
                name: attr.name.value.as_id(),
                infer_type: self.var_gen.from_annotation(&attr.type_annotation),
            })
            .collect();

        let methods: Vec<MethodInfo> = decl.methods.iter()
            .map(|m| MethodInfo {
                name: m.name.value.as_id(),
                param_types: m.params.iter()
                    .map(|(_, t)| self.var_gen.from_annotation(t))
                    .collect(),
                return_type: self.var_gen.from_annotation(&m.return_type),
            })
            .collect();

        let parent = decl.inheritance.as_ref()
            .map(|inh| inh.parent_name.value.as_id());

        self.env.types.insert(name.clone(), TypeInfo { name, params, fields, methods, parent });
    }

    fn register_function(&mut self, decl: &FunctionDecl) {
        let name = decl.name.value.as_id();
        let params = decl.params.iter()
            .map(|(_, t)| self.var_gen.from_annotation(t))
            .collect();
        let ret = self.var_gen.from_annotation(&decl.return_type);
        self.env.functions.insert(name, (params, ret));
    }

    // ========================================================================
    // Etapa 2 — Generación de restricciones
    // ========================================================================

    fn gen_function_decl(&mut self, decl: &mut FunctionDecl) {
        self.env.push_scope();

        let fn_name = decl.name.value.as_id();
        let (param_vars, ret_var) = self.env.functions.get(&fn_name).cloned()
            .unwrap_or_else(|| (vec![], self.var_gen.fresh()));

        for ((param_node, _), pvar) in decl.params.iter().zip(param_vars.iter()) {
            if let Literal::Id(ref name) = param_node.value {
                self.env.define(name.clone(), pvar.clone());
            }
        }

        let body_ty = decl.body.accept(self);
        self.add_eq(body_ty, ret_var);

        self.env.pop_scope();
    }

    fn gen_type_decl(&mut self, decl: &mut TypeDeclNode) {
        let type_name = decl.name.value.as_id();
        self.env.self_type = Some(type_name.clone());

        // Atributos
        self.env.push_scope();
        let ctor_vars: Vec<InferType> = self.env.types.get(&type_name)
            .map(|ti| ti.params.clone())
            .unwrap_or_default();

        for ((param_node, _), pvar) in decl.params.iter().zip(ctor_vars.iter()) {
            if let Literal::Id(ref name) = param_node.value {
                self.env.define(name.clone(), pvar.clone());
            }
        }

        for attr in &mut decl.attributes {
            let attr_name = attr.name.value.as_id();
            let field_var = self.env.lookup_field(&type_name, &attr_name)
                .map(|f| f.infer_type.clone())
                .unwrap_or_else(|| self.var_gen.fresh());

            let init_ty = attr.initializer.accept(self);
            self.add_eq(init_ty, field_var);
        }

        self.env.pop_scope();

        // Métodos
        for method in &mut decl.methods {
            let method_name = method.name.value.as_id();
            self.env.current_method = Some(method_name.clone());
            self.env.push_scope();

            self.env.define("self".to_string(), InferType::class(&type_name));

            let method_param_vars: Vec<InferType> = self.env
                .lookup_method(&type_name, &method_name)
                .map(|mi| mi.param_types.clone())
                .unwrap_or_default();

            for ((param_node, _), pvar) in method.params.iter().zip(method_param_vars.iter()) {
                if let Literal::Id(ref name) = param_node.value {
                    self.env.define(name.clone(), pvar.clone());
                }
            }

            let ret_var = self.env.lookup_method(&type_name, &method_name)
                .map(|mi| mi.return_type.clone())
                .unwrap_or_else(|| self.var_gen.fresh());

            let body_ty = method.body.accept(self);
            self.add_eq(body_ty, ret_var);

            self.env.pop_scope();
            self.env.current_method = None;
        }

        self.env.self_type = None;
    }

    // ========================================================================
    // Helpers de restricciones
    // ========================================================================

    fn add_eq(&mut self, a: InferType, b: InferType) {
        self.constraints.push(Constraint::Eq(a, b));
    }

    fn add_conform(&mut self, sub: InferType, sup: InferType) {
        self.constraints.push(Constraint::Conform(sub, sup));
    }

    fn add_tuple_project(&mut self, tuple_ty: InferType, index: usize, result: InferType) {
        self.constraints.push(Constraint::TupleProject(tuple_ty, index, result));
    }

    // ========================================================================
    // Etapa 3 — Resolución iterativa (worklist + unificación)
    // ========================================================================

    fn solve_constraints(&mut self) {
        let all: Vec<Constraint> = self.constraints.drain(..).collect();
        let mut worklist: VecDeque<Constraint> = all.into();
        let mut stalled_count = 0usize;

        while let Some(raw) = worklist.pop_front() {
            let c = self.subst.apply_to_constraint(&raw);
            self.changed = false;

            if let Some(pending) = self.process_constraint(c) {
                worklist.push_back(pending);
                stalled_count += 1;
                if stalled_count > worklist.len() + 1 {
                    // Punto fijo: las restricciones restantes son irresolubles
                    // (las Conform con Vars sin resolver se descartan silenciosamente;
                    //  el checker detectará Unknown en el AST anotado si es relevante)
                    break;
                }
            } else if self.changed {
                stalled_count = 0;
            }
        }
    }

    /// Procesa una restricción.
    /// - `None`  → resuelta (o descartada).
    /// - `Some` → no se pudo resolver todavía; re-encolar.
    ///
    /// **No emite errores semánticos** — los conflictos de tipo se dejan pasar
    /// para que el `SemanticChecker` los detecte sobre el AST anotado.
    fn process_constraint(&mut self, c: Constraint) -> Option<Constraint> {
        match c {
            Constraint::Eq(a, b) => {
                let a = self.subst.apply(&a);
                let b = self.subst.apply(&b);

                match (&a, &b) {
                    (x, y) if x == y => None,

                    (InferType::Var(id), other) | (other, InferType::Var(id)) => {
                        let id = *id;
                        let other = other.clone();
                        let bound = self.subst.bind(id, other);
                        if bound { self.changed = true; }
                        None
                    }

                    // Concreto ≡ Concreto: intentar reconciliación por LCA.
                    // Los conflictos reales los detecta el SemanticChecker.
                    (InferType::Concrete(_), InferType::Concrete(_)) => None,
                }
            }

            Constraint::Conform(sub, sup) => {
                let sub = self.subst.apply(&sub);
                let sup = self.subst.apply(&sup);

                match (&sub, &sup) {
                    // Ambos concretos: verificación diferida al SemanticChecker.
                    (InferType::Concrete(_), InferType::Concrete(_)) => None,
                    // Alguno es Var: re-encolar hasta resolver.
                    _ => Some(Constraint::Conform(sub, sup)),
                }
            } 
            Constraint::TupleProject(tuple_ty, index, result) => {
                let tuple_ty = self.subst.apply(&tuple_ty);
                let result = self.subst.apply(&result);
                match &tuple_ty {
                    InferType::Concrete(HulkType::Tuple(elems)) => {
                        if let Some(elem_ty) = elems.get(index).cloned() {
                            let elem_infer = InferType::Concrete(elem_ty);
                            if let InferType::Var(id) = result {
                                let bound = self.subst.bind(id, elem_infer);
                                if bound { self.changed = true; }
                            }
                        }
                        None
                    }
                    InferType::Concrete(_) => None, // tipo concreto pero no-tupla: error semántico ya cubierto por SemanticChecker
                    InferType::Var(_) => Some(Constraint::TupleProject(tuple_ty, index, result)), // aún no resuelto, reintentar
                }
        }
    }
    }

    // ========================================================================
    // Etapa 4 — Anotación del AST
    // ========================================================================

    pub fn resolve(&self, t: &InferType) -> HulkType {
        match self.subst.apply(t) {
            InferType::Concrete(h) => h,
            InferType::Var(_)      => HulkType::Unknown,
        }
    }

    fn annotate_function_decl(&mut self, decl: &mut FunctionDecl) {
        let fn_name = decl.name.value.as_id();

        if let Some((_, ret_var)) = self.env.functions.get(&fn_name).cloned() {
            decl.return_type = self.resolve(&ret_var);
        }

        self.env.push_scope();
        if let Some((param_vars, _)) = self.env.functions.get(&fn_name).cloned() {
            for ((param_node, param_ty), pvar) in
                decl.params.iter_mut().zip(param_vars.iter())
            {
                let resolved = self.resolve(pvar);
                *param_ty = resolved.clone();
                if let Literal::Id(ref name) = param_node.value {
                    self.env.define(name.clone(), InferType::Concrete(resolved));
                }
            }
        }

        self.annotate_expr(&mut decl.body);
        self.env.pop_scope();
    }

    fn annotate_type_decl(&mut self, decl: &mut TypeDeclNode) {
        let type_name = decl.name.value.as_id();

        let ctor_vars: Vec<InferType> = self.env.types.get(&type_name)
            .map(|ti| ti.params.clone())
            .unwrap_or_default();

        for ((_, param_ty), pvar) in decl.params.iter_mut().zip(ctor_vars.iter()) {
            *param_ty = self.resolve(pvar);
        }

        // Atributos
        self.env.push_scope();
        let resolved_ctor: Vec<(String, HulkType)> = decl.params.iter()
            .map(|(n, t)| (n.value.as_id(), t.clone()))
            .collect();
        for (name, ty) in &resolved_ctor {
            self.env.define(name.clone(), InferType::Concrete(ty.clone()));
        }

        for attr in &mut decl.attributes {
            let attr_name = attr.name.value.as_id();
            if let Some(field) = self.env.lookup_field(&type_name, &attr_name) {
                let resolved = self.resolve(&field.infer_type);
                attr.type_annotation = resolved.clone();
                self.env.update_field(&type_name, &attr_name, InferType::Concrete(resolved));
            }
            self.annotate_expr(&mut attr.initializer);
        }
        self.env.pop_scope();

        // Métodos
        self.env.self_type = Some(type_name.clone());

        for method in &mut decl.methods {
            let method_name = method.name.value.as_id();

            let (resolved_ret, resolved_params): (HulkType, Vec<HulkType>) =
                if let Some(mi) = self.env.lookup_method(&type_name, &method_name) {
                    let ret = self.resolve(&mi.return_type);
                    let params: Vec<HulkType> = mi.param_types.iter()
                        .map(|p| self.resolve(p))
                        .collect();
                    (ret, params)
                } else {
                    (HulkType::Unknown, vec![])
                };

            method.return_type = resolved_ret.clone();
            self.env.update_method_return(
                &type_name,
                &method_name,
                InferType::Concrete(resolved_ret),
            );
            for ((_, param_ty), resolved) in
                method.params.iter_mut().zip(resolved_params.iter())
            {
                *param_ty = resolved.clone();
            }

            self.env.push_scope();
            self.env.define(
                "self".to_string(),
                InferType::Concrete(HulkType::Class(type_name.clone())),
            );
            for ((param_node, param_ty), _) in
                method.params.iter().zip(resolved_params.iter())
            {
                if let Literal::Id(ref name) = param_node.value {
                    self.env.define(name.clone(), InferType::Concrete(param_ty.clone()));
                }
            }
            self.env.current_method = Some(method_name.clone());

            self.annotate_expr(&mut method.body);

            self.env.pop_scope();
            self.env.current_method = None;
        }

        self.env.self_type = None;
    }

    /// Anotación recursiva: resuelve TypeVars y setea `return_type` en cada nodo.
    fn annotate_expr(&mut self, expr: &mut Expr) {
        match expr {
            Expr::Literal(_) | Expr::SelfRef => {}
            Expr::Binary(node) => {
                self.annotate_expr(&mut node.left);
                self.annotate_expr(&mut node.right);
                node.return_type = self.infer_binary_type(&node.op);
            }
            Expr::Unary(node) => {
                self.annotate_expr(&mut node.expr);
                node.return_type = match node.op {
                    UnaryOp::Not              => HulkType::Bool,
                    UnaryOp::Neg | UnaryOp::Plus => HulkType::Number,
                };
            }
            Expr::Let(node) => {
                let mut scopes_opened = 0usize;

                for ((id_node, var_ty), init_expr) in &mut node.assignments {
                    self.annotate_expr(init_expr);

                    let resolved_ty = if *var_ty != HulkType::Unknown {
                        var_ty.clone()
                    } else {
                        self.type_of_expr(init_expr)
                    };
                    *var_ty = resolved_ty.clone();

                    self.env.push_scope();
                    scopes_opened += 1;
                    if let Literal::Id(ref name) = id_node.value {
                        self.env.define(name.clone(), InferType::Concrete(resolved_ty));
                    }
                }

                self.annotate_expr(&mut node.body);
                node.return_type = self.type_of_expr(&node.body);

                for _ in 0..scopes_opened { self.env.pop_scope(); }
            }
            Expr::If(node) => {
                self.annotate_expr(&mut node.condition);
                self.annotate_expr(&mut node.if_branch);
                let mut branch_ty = self.type_of_expr(&node.if_branch);
                for (cond, body) in &mut node.elif_branches {
                    self.annotate_expr(cond);
                    self.annotate_expr(body);
                    branch_ty = self.env.lca(&branch_ty, &self.type_of_expr(body));
                }
                self.annotate_expr(&mut node.else_branch);
                let else_ty = self.type_of_expr(&node.else_branch);
                node.return_type = self.env.lca(&branch_ty, &else_ty);
            }
            Expr::While(node) => {
                self.annotate_expr(&mut node.condition);
                self.annotate_expr(&mut node.body);
                node.return_type = self.type_of_expr(&node.body);
            }
            Expr::For(node) => {
                self.annotate_expr(&mut node.iterator);
                self.env.push_scope();
                if let Literal::Id(ref var_name) = node.variable.value.clone() {
                    self.env.define(var_name.clone(), InferType::Concrete(HulkType::Number));
                }
                self.annotate_expr(&mut node.body);
                node.return_type = self.type_of_expr(&node.body);
                self.env.pop_scope();
            }
            Expr::Block(node) => {
                for e in &mut node.expressions { self.annotate_expr(e); }
                node.return_type = node.expressions.last()
                    .map(|e| self.type_of_expr(e))
                    .unwrap_or(HulkType::Unknown);
            }
            Expr::FunCall(node) => {
                for arg in &mut node.args { self.annotate_expr(arg); }
                if node.name.value.as_id() == "print" {
                    node.return_type = self.type_of_expr(&node.args[0]);
                } else {
                    let fn_name = node.name.value.as_id();
                    node.return_type = self.env.functions.get(&fn_name)
                        .map(|(_, ret)| self.resolve(ret))
                        .unwrap_or(HulkType::Unknown);
                }
            }
            Expr::Instantiation(node) => {
                for arg in &mut node.args { self.annotate_expr(arg); }
                let type_name = node.name.value.as_id();
                node.return_type = HulkType::Class(type_name);
            }
            Expr::DestAssign(node) => {
                self.annotate_expr(&mut node.expr);
                self.annotate_expr(&mut node.target);
                node.return_type = self.type_of_expr(&node.expr);
            }
            Expr::MemberAccess(node) => {
                self.annotate_expr(&mut node.instance);
                let inst_ty = self.type_of_expr(&node.instance);
                let result = if let HulkType::Class(ref tn) = inst_ty {
                    if let Literal::Id(ref field_name) = node.member.value {
                        self.env.lookup_field(tn, field_name)
                            .map(|f| self.resolve(&f.infer_type))
                            .unwrap_or(HulkType::Unknown)
                    } else { HulkType::Unknown }
                } else { HulkType::Unknown };
                node.set_type(result);
            }
            Expr::MethodCall(node) => {
                self.annotate_expr(&mut node.instance);
                for arg in &mut node.call.args { self.annotate_expr(arg); }
                let inst_ty = self.type_of_expr(&node.instance);
                let result = if let HulkType::Class(ref tn) = inst_ty {
                    let method_name = node.call.name.value.as_id();
                    self.env.lookup_method(tn, &method_name)
                        .map(|mi| self.resolve(&mi.return_type))
                        .unwrap_or(HulkType::Unknown)
                } else { HulkType::Unknown };
                node.call.return_type = result.clone();
                node.set_type(result);
            }
            Expr::BaseCall(args) => {
                for arg in args.iter_mut() { self.annotate_expr(arg); }
            }
            Expr::TypeDowncast(node) => {
                self.annotate_expr(&mut node.expr);
                let target_name = node.target_type.value.as_id();
                node.return_type = HulkType::Class(target_name);
            }
            Expr::TypeTest(node) => {
                self.annotate_expr(&mut node.expr);
                node.return_type = HulkType::Bool;
            }
            Expr::Tuple(node) => {
                for e in &mut node.elements { self.annotate_expr(e); }
                let elem_types: Vec<HulkType> = node.elements.iter()
                    .map(|e| self.type_of_expr(e))
                    .collect();
                node.return_type = HulkType::Tuple(elem_types);
            }
            Expr::TupleAccess(node) => {
                self.annotate_expr(&mut node.tuple);
                let tuple_ty = self.type_of_expr(&node.tuple);
                let result = if let HulkType::Tuple(ref elems) = tuple_ty {
                    elems.get(node.index).cloned().unwrap_or(HulkType::Unknown)
                } else {
                    HulkType::Unknown
                };
                node.return_type = result;
            }
        }
    }

    /// Devuelve el tipo concreto de una expresión ya anotada.
    pub fn type_of_expr(&self, expr: &Expr) -> HulkType {
        match expr {
            Expr::Literal(n) => match &n.value {
                Literal::Number(_) => HulkType::Number,
                Literal::Bool(_)   => HulkType::Bool,
                Literal::Str(_)    => HulkType::String,
                Literal::Id(name)  => self.env.lookup(name)
                    .map(|t| self.resolve(t))
                    .unwrap_or(HulkType::Unknown),
            },
            Expr::SelfRef        => self.env.self_type.as_ref()
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

    fn infer_binary_type(&self, op: &BinaryOp) -> HulkType {
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul
            | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => HulkType::Number,
            BinaryOp::Equal | BinaryOp::Dist
            | BinaryOp::Great | BinaryOp::Less
            | BinaryOp::Gequa | BinaryOp::Lequa
            | BinaryOp::And | BinaryOp::Or => HulkType::Bool,
            BinaryOp::SingleConc | BinaryOp::SpacedConc => HulkType::String,
        }
    }
}

// ============================================================================
// ExprVisitor — generación de restricciones (Etapa 2)
// ============================================================================

impl ExprVisitor<InferType> for TypeInferrer {

    fn visit_number(&mut self, _n: f32) -> InferType { InferType::number() }
    fn visit_bool(&mut self, _b: bool) -> InferType  { InferType::bool_t() }
    fn visit_string(&mut self, _s: &str) -> InferType { InferType::string() }

    fn visit_id(&mut self, id: &str) -> InferType {
        match self.env.lookup(id) {
            Some(t) => t.clone(),
            None    => self.var_gen.fresh(),
        }
    }

    fn visit_self(&mut self) -> InferType {
        match &self.env.self_type {
            Some(name) => InferType::class(name),
            // No emitir error aquí; el SemanticChecker lo detectará
            None => self.var_gen.fresh(),
        }
    }

    fn visit_binary_op(
        &mut self,
        left: &mut Expr,
        op: &BinaryOp,
        right: &mut Expr,
    ) -> InferType {
        let lt = left.accept(self);
        let rt = right.accept(self);

        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul
            | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
                self.add_eq(lt, InferType::number());
                self.add_eq(rt, InferType::number());
                InferType::number()
            }
            BinaryOp::Great | BinaryOp::Less
            | BinaryOp::Gequa | BinaryOp::Lequa => {
                self.add_eq(lt, InferType::number());
                self.add_eq(rt, InferType::number());
                InferType::bool_t()
            }
            BinaryOp::Equal | BinaryOp::Dist => {
                self.add_eq(lt, rt);
                InferType::bool_t()
            }
            BinaryOp::And | BinaryOp::Or => {
                self.add_eq(lt, InferType::bool_t());
                self.add_eq(rt, InferType::bool_t());
                InferType::bool_t()
            }
            BinaryOp::SingleConc | BinaryOp::SpacedConc => {
                self.add_conform(lt, InferType::string());
                self.add_conform(rt, InferType::string());
                InferType::string()
            }
        }
    }

    fn visit_unary_op(&mut self, op: &UnaryOp, expr: &mut Expr) -> InferType {
        let t = expr.accept(self);
        match op {
            UnaryOp::Not => {
                self.add_eq(t, InferType::bool_t());
                InferType::bool_t()
            }
            UnaryOp::Neg | UnaryOp::Plus => {
                self.add_eq(t, InferType::number());
                InferType::number()
            }
        }
    }

    fn visit_let(&mut self, node: &mut LetNode) -> InferType {
        let mut scopes = 0usize;

        for ((id_node, decl_ty), init_expr) in &mut node.assignments {
            let init_infer = init_expr.accept(self);

            let var_ty: InferType = if *decl_ty != HulkType::Unknown {
                let annotated = InferType::from_hulk(decl_ty);
                self.add_conform(init_infer, annotated.clone());
                annotated
            } else {
                let fresh = self.var_gen.fresh();
                self.add_eq(fresh.clone(), init_infer);
                fresh
            };

            self.env.push_scope();
            scopes += 1;
            if let Literal::Id(ref name) = id_node.value {
                self.env.define(name.clone(), var_ty);
            }
        }

        let body_ty = node.body.accept(self);
        for _ in 0..scopes { self.env.pop_scope(); }
        body_ty
    }

    fn visit_if(&mut self, node: &mut IfNode) -> InferType {
        let cond_t = node.condition.accept(self);
        self.add_eq(cond_t, InferType::bool_t());

        let mut branch_vars: Vec<InferType> = vec![node.if_branch.accept(self)];

        for (elif_cond, elif_body) in &mut node.elif_branches {
            let ec = elif_cond.accept(self);
            self.add_eq(ec, InferType::bool_t());
            branch_vars.push(elif_body.accept(self));
        }

        branch_vars.push(node.else_branch.accept(self));

        let result_var = self.var_gen.fresh();
        for bv in branch_vars {
            self.add_conform(bv.clone(), result_var.clone());
            self.add_conform(result_var.clone(), bv);
        }

        result_var
    }

    fn visit_while(&mut self, node: &mut WhileNode) -> InferType {
        let cond_t = node.condition.accept(self);
        self.add_eq(cond_t, InferType::bool_t());
        node.body.accept(self)
    }

    fn visit_for(&mut self, node: &mut ForNode) -> InferType {
        node.iterator.accept(self);
        self.env.push_scope();
        if let Literal::Id(ref var_name) = node.variable.value {
            self.env.define(var_name.clone(), InferType::number());
        }
        let body_t = node.body.accept(self);
        self.env.pop_scope();
        body_t
    }

    fn visit_fun_call(&mut self, node: &mut FunCallNode) -> InferType {
        let arg_tys: Vec<InferType> = node.args.iter_mut()
            .map(|a| a.accept(self))
            .collect();

        if let Literal::Id(ref name) = node.name.value.clone() {
            match self.env.functions.get(name).cloned() {
                Some((param_tys, ret_ty)) => {
                    let variadic = param_tys.len() == 1
                        && param_tys[0] == InferType::unknown();

                    if !variadic && param_tys.len() == arg_tys.len() {
                        for (arg_ty, param_ty) in arg_tys.into_iter().zip(param_tys) {
                            self.add_eq(arg_ty, param_ty);
                        }
                    }
                    // Aridad incorrecta y función no declarada: el checker lo reporta.
                    ret_ty
                }
                None => self.var_gen.fresh(),
            }
        } else {
            self.var_gen.fresh()
        }
    }

    fn visit_dest_assign(&mut self, node: &mut DestAssignNode) -> InferType {
        let expr_ty = node.expr.accept(self);

        match node.target.as_mut() {
            Expr::Literal(lit) => {
                if let Literal::Id(ref name) = lit.value.clone() {
                    if let Some(existing_ty) = self.env.lookup(name).cloned() {
                        self.add_conform(expr_ty.clone(), existing_ty);
                    }
                    // Variable no declarada: el checker lo detecta
                }
            }
            Expr::MemberAccess(ma) => {
                let inst_ty = ma.instance.accept(self);
                if let InferType::Concrete(HulkType::Class(ref tn)) = inst_ty.clone() {
                    if let Literal::Id(ref fn_name) = ma.member.value.clone() {
                        if let Some(fi) = self.env.lookup_field(tn, fn_name) {
                            self.add_conform(expr_ty.clone(), fi.infer_type);
                        }
                    }
                }
                ma.set_type(
                    if let InferType::Concrete(h) = inst_ty { h }
                    else { HulkType::Unknown }
                );
            }
            _ => {} // target inválido: el checker lo detecta
        }

        node.return_type = HulkType::Unknown;
        expr_ty
    }

    fn visit_block(&mut self, node: &mut BlockNode) -> InferType {
        let mut last = InferType::unknown();
        for e in &mut node.expressions { last = e.accept(self); }
        last
    }

    fn visit_instantiation(&mut self, node: &mut FunCallNode) -> InferType {
        let arg_tys: Vec<InferType> = node.args.iter_mut()
            .map(|a| a.accept(self))
            .collect();

        if let Literal::Id(ref type_name) = node.name.value.clone() {
            match self.env.types.get(type_name).cloned() {
                Some(ti) => {
                    let effective_params: Vec<InferType> = if ti.params.is_empty() {
                        ti.parent.as_ref()
                            .and_then(|p| self.env.types.get(p))
                            .map(|pt| pt.params.clone())
                            .unwrap_or_default()
                    } else {
                        ti.params.clone()
                    };

                    if arg_tys.len() == effective_params.len() {
                        for (arg_ty, param_ty) in arg_tys.into_iter().zip(effective_params) {
                            self.add_conform(arg_ty.clone(), param_ty.clone());
                            self.add_eq(param_ty, arg_ty);
                        }
                    }
                    // Aridad incorrecta: el checker lo detecta
                    InferType::class(type_name)
                }
                None => self.var_gen.fresh(),
            }
        } else {
            self.var_gen.fresh()
        }
    }

    fn visit_member_access(&mut self, node: &mut MemberAccessNode) -> InferType {
        let inst_ty = node.instance.accept(self);

        let result = if let InferType::Concrete(HulkType::Class(ref tn)) = inst_ty {
            if let Literal::Id(ref field_name) = node.member.value {
                match self.env.lookup_field(tn, field_name) {
                    Some(fi) => fi.infer_type,
                    None     => self.var_gen.fresh(),
                }
            } else { self.var_gen.fresh() }
        } else if inst_ty == InferType::unknown() {
            self.var_gen.fresh()
        } else {
            self.var_gen.fresh()
        };

        if let InferType::Concrete(h) = &inst_ty { node.set_type(h.clone()); }
        result
    }

    fn visit_method_call(&mut self, node: &mut MethodCallNode) -> InferType {
        let inst_ty = node.instance.accept(self);

        let arg_tys: Vec<InferType> = node.call.args.iter_mut()
            .map(|a| a.accept(self))
            .collect();

        let result = if let InferType::Concrete(HulkType::Class(ref tn)) = inst_ty {
            let method_name = node.call.name.value.as_id();
            match self.env.lookup_method(tn, &method_name) {
                Some(mi) => {
                    if mi.param_types.len() == arg_tys.len() {
                        for (arg_ty, param_ty) in arg_tys.into_iter().zip(mi.param_types) {
                            self.add_conform(arg_ty, param_ty);
                        }
                    }
                    mi.return_type
                }
                None => self.var_gen.fresh(),
            }
        } else {
            self.var_gen.fresh()
        };

        result
    }

    fn visit_base_call(&mut self, args: &mut [Expr]) -> InferType {
        for arg in args.iter_mut() { arg.accept(self); }

        let self_type = match &self.env.self_type {
            Some(t) => t.clone(),
            None    => return self.var_gen.fresh(),
        };
        let method_name = match &self.env.current_method {
            Some(m) => m.clone(),
            None    => return self.var_gen.fresh(),
        };

        let parent_name = match self.env.types.get(&self_type)
            .and_then(|ti| ti.parent.clone())
        {
            Some(p) => p,
            None    => return self.var_gen.fresh(),
        };

        match self.env.lookup_method(&parent_name, &method_name) {
            Some(mi) => mi.return_type,
            None     => self.var_gen.fresh(),
        }
    }
    
    fn visit_type_downcast(&mut self, node: &mut crate::nodes::type_downcast_node::TypeDowncastNode) -> InferType {
        // Visit sub-expression for constraint generation.
        node.expr.accept(self);
 
        let target_name = node.target_type.value.as_id();
        let result_ty = HulkType::Class(target_name.clone());
 
        // Annotate node with the declared target type.
        node.return_type = result_ty;
 
        InferType::class(&target_name)
    }

    fn visit_type_test(&mut self, node: &mut TypeTestNode) -> InferType {
        // Visit sub-expression for constraint generation.
        node.expr.accept(self);
 
        // Annotate this node and return Bool.
        node.return_type = HulkType::Bool;
        InferType::bool_t()
    }
    
    fn visit_tuple(&mut self, node: &mut TupleNode) -> InferType {
        let elem_tys: Vec<InferType> = node.elements.iter_mut()
            .map(|e| e.accept(self))
            .collect();
        // Construir tipo tupla concreto solo si todos los elementos son concretos.
        let hulk_elems: Vec<HulkType> = elem_tys.iter().map(|t| {
            if let InferType::Concrete(h) = t { h.clone() } else { HulkType::Unknown }
        }).collect();
        InferType::Concrete(HulkType::Tuple(hulk_elems))
    }
    
    fn visit_tuple_access(&mut self, node: &mut TupleAccessNode) -> InferType {
        let tuple_ty = node.tuple.accept(self);
        let result_var = self.var_gen.fresh();
        self.add_tuple_project(tuple_ty, node.index, result_var.clone());
        result_var
    }
}