//! # Inferidor de tipos para HULK — enfoque por restricciones + worklist
//!
//! ## Arquitectura general
//!
//! El inferidor opera en tres etapas bien separadas:
//!
//! ### Etapa 1 — Registro de declaraciones (forward pass)
//! Se recorre el programa una sola vez para registrar en el entorno la firma
//! *estática* de cada tipo y función:  nombres, jerarquía de herencia, aridad de
//! constructores y anotaciones de tipo explícitas.  Las posiciones sin anotar se
//! representan con variables de tipo frescas `TypeVar(id)`.
//!
//! ### Etapa 2 — Generación de restricciones (constraint generation)
//! Se recorre el AST de manera bottom-up generando *restricciones de igualdad*
//! entre tipos:
//!
//! ```text
//! Eq(T1, T2)          -- T1 y T2 deben ser el mismo tipo
//! Conform(T1, T2)     -- T1 debe conformar (ser subtipo) a T2
//! ```
//!
//! Cada nodo del AST recibe una variable de tipo fresca que representa su
//! resultado.  Las restricciones se acumulan en `constraints`.
//!
//! ### Etapa 3 — Resolución iterativa (worklist + unificación)
//! Las restricciones se procesan en un worklist.  En cada iteración:
//!
//! 1. Se extrae una restricción.
//! 2. Se aplica la sustitución actual (`subst`) a ambos lados.
//! 3. Se intenta **unificar** los dos tipos resultantes.
//! 4. Si la unificación produce un nuevo enlace `α → T`, se guarda en `subst`
//!    y se re-encolan las restricciones que mencionan `α` para reevaluarlas.
//! 5. Se repite hasta que el worklist se vacíe.
//!
//! ### Etapa 4 — Anotación del AST
//! Se recorre el AST una última vez sustituyendo cada `TypeVar` con el tipo
//! concreto aprendido, o `Unknown` si no se pudo resolver.

use std::{collections::{HashMap, HashSet, VecDeque}, env::args};

use crate::{
    expr_visitor::ExprVisitor,
    nodes::{
        binaryop_node::BinaryOp,
        block_node::BlockNode,
        destassing_node::DestAssignNode,
        for_node::ForNode,
        funcall_node::FunCallNode,
        function_decl_node::FunctionDecl,
        if_node::IfNode,
        let_node::LetNode,
        literal_node::Literal,
        member_access_node::{MemberAccessNode, MethodCallNode},
        program_node::{Program, Statement},
        type_decl_node::TypeDeclNode,
        expr_node::{Expr, HulkType},
        unaryop_node::UnaryOp,
        while_node::WhileNode,
    },
};

// ============================================================================
// Tipo interno del inferidor: extiende HulkType con variables de tipo
// ============================================================================

/// Tipo interno usado durante la inferencia.
/// `HulkType::Unknown` nunca entra aquí directamente; las posiciones sin anotar
/// se representan con `InferType::Var(id)`.
#[derive(Debug, Clone, PartialEq)]
pub enum InferType {
    /// Tipo primitivo o de clase concreto.
    Concrete(HulkType),
    /// Variable de tipo fresca: representa un tipo aún desconocido.
    Var(u32),
}

impl InferType {
    fn number() -> Self { InferType::Concrete(HulkType::Number) }
    fn bool_t() -> Self { InferType::Concrete(HulkType::Bool) }
    fn string() -> Self { InferType::Concrete(HulkType::String) }
    fn class(name: &str) -> Self { InferType::Concrete(HulkType::Class(name.to_string())) }
    fn unknown() -> Self { InferType::Concrete(HulkType::Unknown) }

    /// Convierte un `HulkType` anotado en el AST a `InferType`.
    /// `Unknown` se traduce a una nueva variable — pero el llamador debe
    /// hacerlo explícito pasando un `id` fresco; aquí solo mapeamos los
    /// concretos.
    fn from_hulk(h: &HulkType) -> Self {
        match h {
            HulkType::Unknown => InferType::Concrete(HulkType::Unknown),
            other => InferType::Concrete(other.clone()),
        }
    }

    /// Devuelve `true` si es una variable de tipo.
    fn is_var(&self) -> bool { matches!(self, InferType::Var(_)) }

    /// Si es `Var`, devuelve el id; si no, `None`.
    fn var_id(&self) -> Option<u32> {
        if let InferType::Var(id) = self { Some(*id) } else { None }
    }
}

// ============================================================================
// Restricciones
// ============================================================================

/// Una restricción entre dos tipos internos.
#[derive(Debug, Clone)]
pub enum Constraint {
    /// Los dos tipos deben ser iguales (unificación exacta).
    Eq(InferType, InferType),
    /// `lhs` debe conformar (ser subtipo) de `rhs`.
    /// Cuando `lhs` es concreto y `rhs` también, se verifica directamente.
    /// Cuando alguno es `Var`, se mantiene en el worklist hasta resolver.
    Conform(InferType, InferType),
}

// ============================================================================
// Sustitución
// ============================================================================

/// Mapa de variables de tipo → tipo concreto (o Var si aún no resuelto).
/// Implementa union-find plano con path-compression inline.
#[derive(Default, Debug)]
pub struct Substitution {
    map: HashMap<u32, InferType>,
}

impl Substitution {
    /// Sigue la cadena de sustituciones hasta encontrar un tipo no-Var o una
    /// Var sin mapeo.
    pub fn apply(&self, t: &InferType) -> InferType {
        match t {
            InferType::Var(id) => {
                match self.map.get(id) {
                    Some(t2) if t2 != t => self.apply(t2),
                    _ => t.clone(),
                }
            }
            other => other.clone(),
        }
    }

    /// Registra `var → ty` en la sustitución.
    /// Si `var` ya estaba mapeado, solo actualiza si el nuevo tipo es más
    /// específico (i.e. concreto frente a Var).
    pub fn bind(&mut self, var: u32, ty: InferType) -> bool {
        // Evitar ciclos triviales: no bindear Var(x) → Var(x)
        if InferType::Var(var) == ty {
            return false;
        }
        // Si ya había un binding concreto, no pisar
        if let Some(existing) = self.map.get(&var) {
            if !existing.is_var() {
                return false;
            }
        }
        self.map.insert(var, ty);
        true
    }

    /// Aplica la sustitución a todos los tipos de una colección de constraints.
    pub fn apply_to_constraint(&self, c: &Constraint) -> Constraint {
        match c {
            Constraint::Eq(a, b) => Constraint::Eq(self.apply(a), self.apply(b)),
            Constraint::Conform(a, b) => Constraint::Conform(self.apply(a), self.apply(b)),
        }
    }
}

// ============================================================================
// Información semántica (reutilizada del inferidor anterior)
// ============================================================================

#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub name: String,
    /// Tipo del campo en términos de InferType (puede ser Var si no anotado).
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
    /// Pila de scopes: nombre de variable → InferType.
    scopes: Vec<HashMap<String, InferType>>,
    /// Funciones globales: nombre → (params, retorno).
    pub functions: HashMap<String, (Vec<InferType>, InferType)>,
    /// Tipos declarados.
    pub types: HashMap<String, TypeInfo>,
    /// Contexto actual: tipo de `self`.
    pub self_type: Option<String>,
    /// Contexto actual: nombre del método en curso (para `base()`).
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
        self.functions.insert("sqrt".into(),   (vec![num()], num()));
        self.functions.insert("sin".into(),    (vec![num()], num()));
        self.functions.insert("cos".into(),    (vec![num()], num()));
        self.functions.insert("exp".into(),    (vec![num()], num()));
        self.functions.insert("log".into(),    (vec![num(), num()], num()));
        self.functions.insert("rand".into(),   (vec![], num()));
        self.functions.insert("range".into(),  (vec![num(), num()], unk()));
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

    /// `true` si el tipo concreto `sub` conforma a `sup`.
    pub fn conforms_concrete(&self, sub: &HulkType, sup: &HulkType) -> bool {
        if sub == sup { return true; }
        // Unknown actúa como Object en cualquier dirección
        if matches!(sub, HulkType::Unknown) || matches!(sup, HulkType::Unknown) {
            return true;
        }
        // Primitivos incompatibles entre sí
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

    fn is_subtype(&self, child: &str, ancestor: &str) -> bool {
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
    // Helpers para buscar miembros en la jerarquía
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

    /// Actualiza el tipo de retorno de un método en TypeInfo (usado tras
    /// resolver variables).
    pub fn update_method_return(
        &mut self,
        type_name: &str,
        method_name: &str,
        new_ret: InferType,
    ) {
        if let Some(ti) = self.types.get_mut(type_name) {
            for m in &mut ti.methods {
                if m.name == method_name {
                    m.return_type = new_ret;
                    return;
                }
            }
        }
    }

    /// Actualiza el tipo de un campo.
    pub fn update_field(
        &mut self,
        type_name: &str,
        field_name: &str,
        new_ty: InferType,
    ) {
        if let Some(ti) = self.types.get_mut(type_name) {
            for f in &mut ti.fields {
                if f.name == field_name {
                    f.infer_type = new_ty;
                    return;
                }
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

    /// Genera una variable de tipo fresca.
    fn fresh(&mut self) -> InferType {
        let id = self.0;
        self.0 += 1;
        InferType::Var(id)
    }

    /// Convierte un `HulkType` del AST: si es `Unknown`, genera una Var fresca;
    /// si es concreto, lo envuelve directamente.
    fn from_annotation(&mut self, h: &HulkType) -> InferType {
        if *h == HulkType::Unknown {
            self.fresh()
        } else {
            InferType::from_hulk(h)
        }
    }
}

// ============================================================================
// Contexto de generación de restricciones
// ============================================================================
//
// Durante el recorrido del AST generamos restricciones.  El visitor retorna un
// `InferType` que representa "el tipo de esta expresión" (puede ser una Var
// fresca o un concreto si ya se sabe).

/// Estado completo del inferidor.
pub struct TypeInferrer {
    pub env: Environment,
    var_gen: VarGen,
    /// Restricciones generadas durante el recorrido del AST.
    constraints: Vec<Constraint>,
    /// Sustitución acumulada durante la resolución.
    subst: Substitution,
    /// Errores semánticos encontrados.
    pub errors: Vec<String>,
    /// Variables que dependen de cada Var: cuando bindamos Var(x) → T,
    /// re-encolamos las restricciones que mencionan x.
    /// (Para el worklist, simplemente re-procesamos todas si hubo cambio.)
    changed: bool,
}

impl TypeInferrer {
    pub fn new() -> Self {
        Self {
            env: Environment::new(),
            var_gen: VarGen::new(),
            constraints: Vec::new(),
            subst: Substitution::default(),
            errors: Vec::new(),
            changed: false,
        }
    }

    // ========================================================================
    // Punto de entrada
    // ========================================================================

    pub fn infer_program(&mut self, program: &mut Program) -> bool {
    //     let print_arg_var = self.var_gen.fresh();
    
    // // Agregar restricción: tipo_retorno == tipo_argumento
    //     self.env.functions.insert("print".into(), (vec![print_arg_var.clone()], print_arg_var));

        // ---- Etapa 1: registrar firmas ----
        self.register_declarations(program);

        // ---- Etapa 2: generar restricciones recorriendo el AST ----
        for stmt in program.statements.iter_mut() {
            match stmt {
                Statement::FunctionDecl(decl) => self.gen_function_decl(decl),
                Statement::TypeDecl(decl)     => self.gen_type_decl(decl),
                Statement::Expression(expr)   => { expr.accept(self); }
            }
        }

        // ---- Etapa 3: resolver restricciones (worklist hasta punto fijo) ----
        self.solve_constraints();

        // ---- Etapa 4: anotar el AST con los tipos resueltos ----
        for stmt in program.statements.iter_mut() {
            match stmt {
                Statement::FunctionDecl(decl) => self.annotate_function_decl(decl),
                Statement::TypeDecl(decl)     => self.annotate_type_decl(decl),
                Statement::Expression(expr)   => { self.annotate_expr(expr); }
            }
        }

        self.errors.is_empty()
    }

    // ========================================================================
    // Etapa 1 — Registro de declaraciones
    // ========================================================================

    fn register_declarations(&mut self, program: &Program) {
        // Primero todos los tipos (para que las referencias cruzadas funcionen)
        for stmt in &program.statements {
            if let Statement::TypeDecl(decl) = stmt {
                self.register_type(decl);
            }
        }
        // Luego funciones globales
        for stmt in &program.statements {
            if let Statement::FunctionDecl(decl) = stmt {
                self.register_function(decl);
            }
        }
    }

    fn register_type(&mut self, decl: &TypeDeclNode) {
        let name = decl.name.value.as_id();

        // Parámetros del constructor: Var fresca si sin anotar
        let params: Vec<InferType> = decl.params.iter()
            .map(|(_, t)| self.var_gen.from_annotation(t))
            .collect();

        // Campos
        let fields: Vec<FieldInfo> = decl.attributes.iter()
            .map(|attr| FieldInfo {
                name: attr.name.value.as_id(),
                infer_type: self.var_gen.from_annotation(&attr.type_annotation),
            })
            .collect();

        // Métodos: Var fresca para parámetros y retorno sin anotar
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

        self.env.types.insert(name.clone(), TypeInfo {
            name,
            params,
            fields,
            methods,
            parent,
        });
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

        // Definir parámetros en el scope con sus InferTypes registrados
        let fn_name = decl.name.value.as_id();
        let (param_vars, ret_var) = {
            let sig = self.env.functions.get(&fn_name).cloned()
                .unwrap_or_else(|| (vec![], self.var_gen.fresh()));
            (sig.0, sig.1)
        };

        for ((param_node, _), pvar) in decl.params.iter().zip(param_vars.iter()) {
            if let Literal::Id(ref name) = param_node.value {
                self.env.define(name.clone(), pvar.clone());
            }
        }

        // El tipo del cuerpo debe igualar el tipo de retorno declarado
        let body_ty = decl.body.accept(self);
        self.add_eq(body_ty, ret_var);

        self.env.pop_scope();
    }

    fn gen_type_decl(&mut self, decl: &mut TypeDeclNode) {
        let type_name = decl.name.value.as_id();
        self.env.self_type = Some(type_name.clone());

        // --- Atributos ---
        // Abrir scope con parámetros del constructor
        self.env.push_scope();
        let ctor_vars: Vec<InferType> = {
            self.env.types.get(&type_name)
                .map(|ti| ti.params.clone())
                .unwrap_or_default()
        };
        for ((param_node, _), pvar) in decl.params.iter().zip(ctor_vars.iter()) {
            if let Literal::Id(ref name) = param_node.value {
                self.env.define(name.clone(), pvar.clone());
            }
        }

        for attr in &mut decl.attributes {
            let attr_name = attr.name.value.as_id();
            // Tipo registrado del campo
            let field_var = self.env.lookup_field(&type_name, &attr_name)
                .map(|f| f.infer_type.clone())
                .unwrap_or_else(|| self.var_gen.fresh());

            // Tipo inferido del inicializador
            let init_ty = attr.initializer.accept(self);

            // Restricción: tipo del inicializador = tipo del campo
            self.add_eq(init_ty, field_var);
        }

        self.env.pop_scope();

        // --- Métodos ---
        for method in &mut decl.methods {
            let method_name = method.name.value.as_id();
            self.env.current_method = Some(method_name.clone());
            self.env.push_scope();

            // `self` disponible con el tipo de la clase
            self.env.define("self".to_string(), InferType::class(&type_name));

            // Parámetros del método
            let method_param_vars: Vec<InferType> = self.env
                .lookup_method(&type_name, &method_name)
                .map(|mi| mi.param_types.clone())
                .unwrap_or_default();

            for ((param_node, _), pvar) in method.params.iter().zip(method_param_vars.iter()) {
                if let Literal::Id(ref name) = param_node.value {
                    self.env.define(name.clone(), pvar.clone());
                }
            }

            // El cuerpo debe igualar el tipo de retorno del método
            let ret_var = self.env
                .lookup_method(&type_name, &method_name)
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
    // Helpers para agregar restricciones
    // ========================================================================

    fn add_eq(&mut self, a: InferType, b: InferType) {
        self.constraints.push(Constraint::Eq(a, b));
    }

    fn add_conform(&mut self, sub: InferType, sup: InferType) {
        self.constraints.push(Constraint::Conform(sub, sup));
    }

    // ========================================================================
    // Etapa 3 — Resolución iterativa (worklist + unificación)
    // ========================================================================

    fn solve_constraints(&mut self) {
        // El worklist comienza con todas las restricciones generadas.
        let all: Vec<Constraint> = self.constraints.drain(..).collect();
        let mut worklist: VecDeque<Constraint> = all.into();

        // Iteramos hasta que el worklist se vacíe.
        // Si en una pasada completa no aprendimos nada nuevo (`changed = false`)
        // y todavía quedan restricciones, las restricciones restantes no son
        // resolubles con la información disponible (las reportamos si son Eq
        // entre concretos incompatibles).
        let mut stalled_count = 0usize;

        while let Some(raw) = worklist.pop_front() {
            // Aplicar sustitución actual antes de procesar
            let c = self.subst.apply_to_constraint(&raw);

            self.changed = false;
            let re_enqueue = self.process_constraint(c);

            if let Some(pending) = re_enqueue {
                // No pudimos resolver; re-encolar
                worklist.push_back(pending);
                stalled_count += 1;
                // Si llevamos más iteraciones sin progreso que restricciones
                // pendientes, estamos en punto fijo: salir.
                if stalled_count > worklist.len() + 1 {
                    // Última oportunidad: verificar restricciones Conform pendientes
                    for leftover in worklist.drain(..) {
                        self.check_leftover(leftover);
                    }
                    break;
                }
            } else {
                // Progresamos: resetear contador de estancamiento
                if self.changed {
                    stalled_count = 0;
                }
            }
        }
    }

    /// Procesa una restricción.
    /// - Retorna `None` si la restricción quedó resuelta (o generó un error).
    /// - Retorna `Some(c)` si la restricción no pudo resolverse todavía.
    fn process_constraint(&mut self, c: Constraint) -> Option<Constraint> {
        match c {
            // ------------------------------------------------------------------
            // Igualdad: unificar A ≡ B
            // ------------------------------------------------------------------
            Constraint::Eq(a, b) => {
                let a = self.subst.apply(&a);
                let b = self.subst.apply(&b);
                print!("a: {:?}", a);
                print!("b: {:?}", b);
                match (&a, &b) {
                    // Mismos tipos → trivialmente satisfecha
                    (x, y) if x == y => None,

                    // Var(x) ≡ T  →  bind x → T
                    (InferType::Var(id), other) | (other, InferType::Var(id)) => {
                        let id = *id;
                        let other = other.clone();
                        // Occurs check: si `other` también es Var(id), ya los manejamos arriba
                        let new_bind = self.subst.bind(id, other);
                        if new_bind { self.changed = true; }
                        None
                    }

                    // Concreto ≡ Concreto
                    (InferType::Concrete(ca), InferType::Concrete(cb)) => {
                        if ca != cb {
                            // Intentar LCA como "reconciliación" para igualdades
                            // entre ramas de if: si son compatibles por herencia,
                            // unificamos al LCA.
                            let lca = self.env.lca(ca, cb);
                            if lca != HulkType::Unknown || 
                               matches!(ca, HulkType::Unknown) ||
                               matches!(cb, HulkType::Unknown)
                            {
                                // Compatible: no generamos error, los dejamos como están
                            } else {
                                self.errors.push(format!(
                                    "Error de tipo: se esperaba {:?} pero se obtuvo {:?}.",
                                    ca, cb
                                ));
                            }
                        }
                        None
                    }
                }
            }

            // ------------------------------------------------------------------
            // Conformidad: A ≤ B
            // ------------------------------------------------------------------
            Constraint::Conform(sub, sup) => {
                let sub = self.subst.apply(&sub);
                let sup = self.subst.apply(&sup);

                match (&sub, &sup) {
                    // Ambos concretos: verificar directamente
                    (InferType::Concrete(cs), InferType::Concrete(cp)) => {
                        if !self.env.conforms_concrete(cs, cp) {
                            self.errors.push(format!(
                                "Error de tipo: {:?} no conforma a {:?}.", cs, cp
                            ));
                        }
                        None
                    }
                    // Si alguno es Var, no podemos verificar todavía
                    _ => Some(Constraint::Conform(sub, sup)),
                }
            }
        }
    }

    /// Llamado sobre las restricciones que quedaron sin resolver al finalizar.
    fn check_leftover(&mut self, c: Constraint) {
        let c = self.subst.apply_to_constraint(&c);
        match c {
            Constraint::Conform(sub, sup) => {
                let sub = self.subst.apply(&sub);
                let sup = self.subst.apply(&sup);
                match (&sub, &sup) {
                    (InferType::Concrete(cs), InferType::Concrete(cp)) => {
                        if !self.env.conforms_concrete(cs, cp) {
                            self.errors.push(format!(
                                "Error de tipo: {:?} no conforma a {:?}.", cs, cp
                            ));
                        }
                    }
                    // Var sin resolver: el tipo no pudo inferirse
                    // (la spec permite esto: inferencia básica puede fallar en símbolos)
                    _ => {}
                }
            }
            Constraint::Eq(a, b) => {
                // Ya procesado antes; ignorar
                let _ = (a, b);
            }
        }
    }

    // ========================================================================
    // Etapa 4 — Anotación del AST
    // ========================================================================
    // Recorremos el AST y resolvemos cada TypeVar al tipo concreto aprendido.

    fn resolve(&self, t: &InferType) -> HulkType {
        match self.subst.apply(t) {
            InferType::Concrete(h) => h,
            InferType::Var(_)      => HulkType::Unknown, // no resuelto
        }
    }

    fn annotate_function_decl(&mut self, decl: &mut FunctionDecl) {
        let fn_name = decl.name.value.as_id();

        // Resolver el tipo de retorno
        if let Some((_, ret_var)) = self.env.functions.get(&fn_name).cloned() {
            decl.return_type = self.resolve(&ret_var);
        }

        // Resolver parámetros y construir scope para que el cuerpo los vea
        self.env.push_scope();
        if let Some((param_vars, _)) = self.env.functions.get(&fn_name).cloned() {
            for ((param_node, param_ty), pvar) in
                decl.params.iter_mut().zip(param_vars.iter())
            {
                let resolved = self.resolve(pvar);
                *param_ty = resolved.clone();
                if let Literal::Id(ref name) = param_node.value {
                    self.env.define(
                        name.clone(),
                        InferType::Concrete(resolved),
                    );
                }
            }
        }

        // Anotar el cuerpo (ahora los parámetros están en scope)
        self.annotate_expr(&mut decl.body);
        self.env.pop_scope();
    }

    fn annotate_type_decl(&mut self, decl: &mut TypeDeclNode) {
        let type_name = decl.name.value.as_id();

        // Resolver tipos de parámetros del constructor
        let ctor_vars: Vec<InferType> = self.env.types.get(&type_name)
            .map(|ti| ti.params.clone())
            .unwrap_or_default();
        for ((_, param_ty), pvar) in decl.params.iter_mut().zip(ctor_vars.iter()) {
            *param_ty = self.resolve(pvar);
        }

        // --- Anotar atributos ---
        // Scope con parámetros del constructor (para que el inicializador los vea)
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
                self.env.update_field(
                    &type_name,
                    &attr_name,
                    InferType::Concrete(resolved),
                );
            }
            self.annotate_expr(&mut attr.initializer);
        }
        self.env.pop_scope();

        // --- Anotar métodos ---
        self.env.self_type = Some(type_name.clone());

        for method in &mut decl.methods {
            let method_name = method.name.value.as_id();

            // Resolver tipo de retorno y parámetros
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

            // Scope del método: self + parámetros
            self.env.push_scope();
            self.env.define(
                "self".to_string(),
                InferType::Concrete(HulkType::Class(type_name.clone())),
            );
            for ((param_node, param_ty), _) in
                method.params.iter().zip(resolved_params.iter())
            {
                if let Literal::Id(ref name) = param_node.value {
                    self.env.define(
                        name.clone(),
                        InferType::Concrete(param_ty.clone()),
                    );
                }
            }
            self.env.current_method = Some(method_name.clone());

            self.annotate_expr(&mut method.body);

            self.env.pop_scope();
            self.env.current_method = None;
        }

        self.env.self_type = None;
    }

    /// Anotación recursiva de expresiones: resuelve TypeVars y setea
    /// `return_type` en cada nodo que lo tenga.
    ///
    /// IMPORTANTE: debe ser `&mut self` porque reconstruye los scopes del entorno
    /// al entrar en `Let`, `For`, cuerpos de tipo, etc. — esto es necesario para
    /// que `type_of_id` pueda hacer `env.lookup(name)` + `subst.apply()` y
    /// obtener el tipo concreto de cualquier variable (e.g., `p: Knight`).
    fn annotate_expr(&mut self, expr: &mut Expr) {
        match expr {
            // Literales atómicos: sin campo return_type propio; type_of_expr
            // los resuelve directamente por su variante.
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
                // FIX #2: reconstruir los scopes de cada binding para que el
                // cuerpo pueda resolver identificadores como `p`.
                let mut scopes_opened = 0usize;

                for ((id_node, var_ty), init_expr) in &mut node.assignments {
                    // 1. Anotar el inicializador en el scope actual
                    self.annotate_expr(init_expr);

                    // 2. Resolver el tipo de la variable usando la sustitución
                    let resolved_ty = if *var_ty != HulkType::Unknown {
                        // Había anotación explícita: ya resuelta en etapa 1
                        var_ty.clone()
                    } else {
                        // Sin anotación: usar el tipo del inicializador
                        self.type_of_expr(init_expr)
                    };
                    *var_ty = resolved_ty.clone();

                    // 3. Abrir scope y definir la variable con el tipo concreto
                    //    para que los nodos posteriores puedan hacer lookup.
                    self.env.push_scope();
                    scopes_opened += 1;
                    if let Literal::Id(ref name) = id_node.value {
                        self.env.define(
                            name.clone(),
                            InferType::Concrete(resolved_ty),
                        );
                    }
                }

                // 4. Anotar el cuerpo (ya ve todos los bindings en el scope)
                self.annotate_expr(&mut node.body);
                node.return_type = self.type_of_expr(&node.body);

                // 5. Cerrar los scopes abiertos
                for _ in 0..scopes_opened {
                    self.env.pop_scope();
                }
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
                // La variable de iteración es Number (producida por range)
                self.env.push_scope();
                if let Literal::Id(ref var_name) = node.variable.value.clone() {
                    self.env.define(
                        var_name.clone(),
                        InferType::Concrete(HulkType::Number),
                    );
                }
                self.annotate_expr(&mut node.body);
                node.return_type = self.type_of_expr(&node.body);
                self.env.pop_scope();
            }

            Expr::Block(node) => {
                for e in &mut node.expressions {
                    self.annotate_expr(e);
                }
                node.return_type = node.expressions.last()
                    .map(|e| self.type_of_expr(e))
                    .unwrap_or(HulkType::Unknown);
            }

            Expr::FunCall(node) => {
                 
                for arg in &mut node.args {
                    self.annotate_expr(arg);
                } 
                  if node.name.value.as_id() == "print" {
                     node.return_type=self.type_of_expr(&node.args[0]);
                  } else {
                    let fn_name = node.name.value.as_id();
                node.return_type = self.env.functions.get(&fn_name)
                    .map(|(_, ret)| self.resolve(ret))
                    .unwrap_or(HulkType::Unknown);
                }
                
            }

            Expr::Instantiation(node) => {
                for arg in &mut node.args {
                    self.annotate_expr(arg);
                }
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
                // FIX #3: leer inst_ty DESPUÉS de anotar, usando type_of_expr
                // que ahora puede resolver Literal::Id via env.lookup + subst.
                let inst_ty = self.type_of_expr(&node.instance);
                let result = if let HulkType::Class(ref tn) = inst_ty {
                    if let Literal::Id(ref fn_name) = node.member.value {
                        self.env.lookup_field(tn, fn_name)
                            .map(|f| self.resolve(&f.infer_type))
                            .unwrap_or(HulkType::Unknown)
                    } else { HulkType::Unknown }
                } else { HulkType::Unknown };
                node.set_type(result);
            }

            Expr::MethodCall(node) => {
                self.annotate_expr(&mut node.instance);
                for arg in &mut node.call.args {
                    self.annotate_expr(arg);
                }
                // FIX #3: type_of_expr ahora resuelve Literal::Id correctamente
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
                for arg in args.iter_mut() {
                    self.annotate_expr(arg);
                }
            }
        }
    }

    /// Devuelve el tipo concreto de una expresión ya anotada.
    ///
    /// FIX #1 + FIX #4: el caso `Literal::Id` ahora hace lookup en el scope
    /// del entorno (que annotate_expr mantiene vivo) y luego aplica `subst`
    /// para resolver cualquier TypeVar a su tipo concreto.
    fn type_of_expr(&self, expr: &Expr) -> HulkType {
        match expr {
            Expr::Literal(n) => match &n.value {
                Literal::Number(_) => HulkType::Number,
                Literal::Bool(_)   => HulkType::Bool,
                Literal::Str(_)    => HulkType::String,
                // FIX #1 y #4: en lugar de devolver Unknown, busca en el scope
                // y aplica la sustitución para resolver Var → tipo concreto.
                Literal::Id(name)  => {
                    self.env.lookup(name)
                        .map(|t| self.resolve(t))
                        .unwrap_or(HulkType::Unknown)
                }
            },
            Expr::SelfRef => self.env.self_type.as_ref()
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
// Implementación del visitor — Generación de restricciones (Etapa 2)
// ============================================================================
//
// Cada método retorna un `InferType` que representa el tipo de la expresión.
// Las restricciones se acumulan en `self.constraints`.

impl ExprVisitor<InferType> for TypeInferrer {

    // ------------------------------------------------------------------
    // Literales atómicos
    // ------------------------------------------------------------------

    fn visit_number(&mut self, _n: f32) -> InferType {
        InferType::number()
    }

    fn visit_bool(&mut self, _b: bool) -> InferType {
        InferType::bool_t()
    }

    fn visit_string(&mut self, _s: &str) -> InferType {
        InferType::string()
    }

    /// Identificador: busca en el scope.  Si no está definido, genera Var fresca
    /// (puede ser una función de 0 args referenciada como valor, etc.).
    fn visit_id(&mut self, id: &str) -> InferType {
        match self.env.lookup(id) {
            Some(t) => t.clone(),
            None    => self.var_gen.fresh(), // no declarado aún → Var fresca
        }
    }

    // ------------------------------------------------------------------
    // `self`
    // ------------------------------------------------------------------

    fn visit_self(&mut self) -> InferType {
        match &self.env.self_type {
            Some(name) => InferType::class(name),
            None => {
                self.errors.push(
                    "Error semántico: 'self' usado fuera de un tipo.".to_string()
                );
                self.var_gen.fresh()
            }
        }
    }

    // ------------------------------------------------------------------
    // Operadores binarios
    // ------------------------------------------------------------------

    fn visit_binary_op(
        &mut self,
        left: &mut Expr,
        op: &BinaryOp,
        right: &mut Expr,
    ) -> InferType {
        let lt = left.accept(self);
        let rt = right.accept(self);

        match op {
            // Aritmética: ambos operandos deben ser Number; resultado Number
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul
            | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
                self.add_eq(lt, InferType::number());
                self.add_eq(rt, InferType::number());
                InferType::number()
            }
            // Comparación relacional: operandos Number, resultado Bool
            BinaryOp::Great | BinaryOp::Less
            | BinaryOp::Gequa | BinaryOp::Lequa => {
                self.add_eq(lt, InferType::number());
                self.add_eq(rt, InferType::number());
                InferType::bool_t()
            }
            // Igualdad estructural: cualquier tipo, resultado Bool
            BinaryOp::Equal | BinaryOp::Dist => {
                // Los dos operandos deben tener el mismo tipo
                self.add_eq(lt, rt);
                InferType::bool_t()
            }
            // Lógica: ambos Bool
            BinaryOp::And | BinaryOp::Or => {
                self.add_eq(lt, InferType::bool_t());
                self.add_eq(rt, InferType::bool_t());
                InferType::bool_t()
            }
            // Concatenación: al menos uno String
            BinaryOp::SingleConc | BinaryOp::SpacedConc => {
                // No podemos forzar igualdad; generamos conform hacia String
                self.add_conform(lt, InferType::string());
                self.add_conform(rt, InferType::string());
                InferType::string()
            }
        }
    }

    // ------------------------------------------------------------------
    // Operador unario
    // ------------------------------------------------------------------

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

    // ------------------------------------------------------------------
    // `let`
    // ------------------------------------------------------------------

    fn visit_let(&mut self, node: &mut LetNode) -> InferType {
        let mut scopes = 0usize;

        for ((id_node, decl_ty), init_expr) in &mut node.assignments {
            let init_infer = init_expr.accept(self);

            // Tipo efectivo: si hay anotación explícita, igualar; si no, usar
            // una variable de tipo enlazada al inicializador.
            let var_ty: InferType = if *decl_ty != HulkType::Unknown {
                let annotated = InferType::from_hulk(decl_ty);
                // El inicializador debe conformar al tipo declarado
                self.add_conform(init_infer, annotated.clone());
                annotated
            } else {
                // Sin anotación: la variable tiene el mismo tipo que el init
                let fresh = self.var_gen.fresh();
                self.add_eq(fresh.clone(), init_infer);
                // Actualizar la anotación en el nodo (se concretará en etapa 4)
                fresh.clone()
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

    // ------------------------------------------------------------------
    // `if / elif / else`
    // ------------------------------------------------------------------

    fn visit_if(&mut self, node: &mut IfNode) -> InferType {
        let cond_t = node.condition.accept(self);
        self.add_eq(cond_t, InferType::bool_t());

        // Recolectar tipos de todas las ramas
        let mut branch_vars: Vec<InferType> = Vec::new();
        branch_vars.push(node.if_branch.accept(self));

        for (elif_cond, elif_body) in &mut node.elif_branches {
            let ec = elif_cond.accept(self);
            self.add_eq(ec, InferType::bool_t());
            branch_vars.push(elif_body.accept(self));
        }

        branch_vars.push(node.else_branch.accept(self));

        // El tipo del if es una Var fresca que se igualará al LCA de las ramas.
        // Como el LCA requiere tipos concretos, generamos una Var fresca y
        // restricciones de conformidad en ambas direcciones (debería ser igual).
        // En la práctica, para la misma rama concreta, se unificará.
        let result_var = self.var_gen.fresh();
        for bv in branch_vars {
            // Cada rama debe conformar al resultado
            self.add_conform(bv.clone(), result_var.clone());
            // Y el resultado conforma a cada rama (para fijar el LCA)
            self.add_conform(result_var.clone(), bv);
        }

        result_var
    }

    // ------------------------------------------------------------------
    // `while`
    // ------------------------------------------------------------------

    fn visit_while(&mut self, node: &mut WhileNode) -> InferType {
        let cond_t = node.condition.accept(self);
        self.add_eq(cond_t, InferType::bool_t());
        let body_t = node.body.accept(self);
        body_t
    }

    // ------------------------------------------------------------------
    // `for`
    // ------------------------------------------------------------------

    fn visit_for(&mut self, node: &mut ForNode) -> InferType {
        // El iterador (range) produce Numbers
        node.iterator.accept(self);

        self.env.push_scope();
        if let Literal::Id(ref var_name) = node.variable.value {
            self.env.define(var_name.clone(), InferType::number());
        }

        let body_t = node.body.accept(self);
        self.env.pop_scope();
        body_t
    }

    // ------------------------------------------------------------------
    // Llamada a función
    // ------------------------------------------------------------------

    fn visit_fun_call(&mut self, node: &mut FunCallNode) -> InferType {
        let arg_tys: Vec<InferType> = node.args.iter_mut()
            .map(|a| a.accept(self))
            .collect();
         print!("{:?}",arg_tys);
        if let Literal::Id(ref name) = node.name.value.clone() {
              
            
            match self.env.functions.get(name).cloned() {
                Some((param_tys, ret_ty)) => {
                    // Variadic (print): aceptamos cualquier tipo
                    let variadic = param_tys.len() == 1
                        && param_tys[0] == InferType::unknown();

                    if !variadic {
                        if param_tys.len() != arg_tys.len() {
                            self.errors.push(format!(
                                "Error de tipo: '{}' espera {} argumento(s), se dieron {}.",
                                name, param_tys.len(), arg_tys.len()
                            ));
                        } else {
                            for (arg_ty, param_ty) in arg_tys.into_iter().zip(param_tys) {
                                self.add_eq(arg_ty, param_ty);
                            }
                        }
                    }
                    ret_ty
                }
                None => {
                    self.errors.push(format!(
                        "Error semántico: función '{}' no declarada.", name
                    ));
                    self.var_gen.fresh()
                }
            }
        } else {
            self.var_gen.fresh()
        }
    }

    // ------------------------------------------------------------------
    // Asignación destructiva `:=`
    // ------------------------------------------------------------------

    fn visit_dest_assign(&mut self, node: &mut DestAssignNode) -> InferType {
        let expr_ty = node.expr.accept(self);

        match node.target.as_mut() {
            Expr::Literal(lit) => {
                if let Literal::Id(ref name) = lit.value.clone() {
                    match self.env.lookup(name).cloned() {
                        Some(existing_ty) => {
                            // El valor asignado debe conformar al tipo de la variable
                            self.add_conform(expr_ty.clone(), existing_ty);
                        }
                        None => {
                            self.errors.push(format!(
                                "Error semántico: variable '{}' no declarada en ':='.", name
                            ));
                        }
                    }
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
            _ => {
                self.errors.push(
                    "Error semántico: target de ':=' inválido.".to_string()
                );
            }
        }

        node.return_type = HulkType::Unknown; // se anotará en etapa 4
        expr_ty
    }

    // ------------------------------------------------------------------
    // Bloque
    // ------------------------------------------------------------------

    fn visit_block(&mut self, node: &mut BlockNode) -> InferType {
        let mut last = InferType::unknown();
        for e in &mut node.expressions {
            last = e.accept(self);
        }
        last
    }

    // ------------------------------------------------------------------
    // Instanciación `new Tipo(args...)`
    // ------------------------------------------------------------------

    fn visit_instantiation(&mut self, node: &mut FunCallNode) -> InferType {
        let arg_tys: Vec<InferType> = node.args.iter_mut()
            .map(|a| a.accept(self))
            .collect();

        if let Literal::Id(ref type_name) = node.name.value.clone() {
            match self.env.types.get(type_name).cloned() {
                Some(ti) => {
                    // Si el tipo tiene 0 params propios, tomar los del padre
                    let effective_params: Vec<InferType> = if ti.params.is_empty() {
                        ti.parent.as_ref()
                            .and_then(|p| self.env.types.get(p))
                            .map(|pt| pt.params.clone())
                            .unwrap_or_default()
                    } else {
                        ti.params.clone()
                    };

                    if arg_tys.len() != effective_params.len() && !effective_params.is_empty() {
                        self.errors.push(format!(
                            "Error de tipo: constructor de '{}' espera {} arg(s), se dieron {}.",
                            type_name, effective_params.len(), arg_tys.len()
                        ));
                    } else {
                        for (arg_ty, param_ty) in arg_tys.into_iter().zip(effective_params) {
                            // Los argumentos del constructor deben conformar a los params
                            self.add_conform(arg_ty.clone(), param_ty.clone());
                            // Además, los params aprenden el tipo de los argumentos
                            // (esto es lo que permite inferir el tipo de `firstname`)
                            self.add_eq(param_ty, arg_ty);
                        }
                    }
                    InferType::class(type_name)
                }
                None => {
                    self.errors.push(format!(
                        "Error semántico: tipo '{}' no declarado.", type_name
                    ));
                    self.var_gen.fresh()
                }
            }
        } else {
            self.var_gen.fresh()
        }
    }

    // ------------------------------------------------------------------
    // Acceso a miembro `expr.campo`
    // ------------------------------------------------------------------

    fn visit_member_access(&mut self, node: &mut MemberAccessNode) -> InferType {
        let inst_ty = node.instance.accept(self);

        let result = if let InferType::Concrete(HulkType::Class(ref tn)) = inst_ty {
            if let Literal::Id(ref fn_name) = node.member.value {
                match self.env.lookup_field(tn, fn_name) {
                    Some(fi) => fi.infer_type,
                    None => {
                        self.errors.push(format!(
                            "Error semántico: '{}' no tiene atributo '{}'.", tn, fn_name
                        ));
                        self.var_gen.fresh()
                    }
                }
            } else { self.var_gen.fresh() }
        } else if inst_ty == InferType::unknown() {
            self.var_gen.fresh()
        } else {
            self.errors.push(format!(
                "Error semántico: acceso a miembro sobre tipo no-clase {:?}.", inst_ty
            ));
            self.var_gen.fresh()
        };

        // Anotar ya el nodo para que el codegen pueda usarlo
        if let InferType::Concrete(h) = &inst_ty {
            node.set_type(h.clone());
        }
        result
    }

    // ------------------------------------------------------------------
    // Llamada a método `expr.metodo(args...)`
    // ------------------------------------------------------------------

    fn visit_method_call(&mut self, node: &mut MethodCallNode) -> InferType {
        let inst_ty = node.instance.accept(self);

        let arg_tys: Vec<InferType> = node.call.args.iter_mut()
            .map(|a| a.accept(self))
            .collect();

        let result = if let InferType::Concrete(HulkType::Class(ref tn)) = inst_ty {
            let method_name = node.call.name.value.as_id();
            match self.env.lookup_method(tn, &method_name) {
                Some(mi) => {
                    if mi.param_types.len() != arg_tys.len() {
                        self.errors.push(format!(
                            "Error de tipo: '{}::{}' espera {} arg(s), se dieron {}.",
                            tn, method_name, mi.param_types.len(), arg_tys.len()
                        ));
                    } else {
                        for (arg_ty, param_ty) in arg_tys.into_iter().zip(mi.param_types) {
                            self.add_conform(arg_ty, param_ty);
                        }
                    }
                    mi.return_type
                }
                None => {
                    self.errors.push(format!(
                        "Error semántico: '{}' no tiene método '{}'.", tn, method_name
                    ));
                    self.var_gen.fresh()
                }
            }
        } else {
            self.var_gen.fresh()
        };

        result
    }

    // ------------------------------------------------------------------
    // `base(args...)`
    // ------------------------------------------------------------------

    fn visit_base_call(&mut self, args: &mut [Expr]) -> InferType {
        for arg in args.iter_mut() {
            arg.accept(self);
        }

        let self_type = match &self.env.self_type {
            Some(t) => t.clone(),
            None => {
                self.errors.push("'base()' fuera de un tipo.".to_string());
                return self.var_gen.fresh();
            }
        };
        let method_name = match &self.env.current_method {
            Some(m) => m.clone(),
            None => {
                self.errors.push("'base()' fuera de un método.".to_string());
                return self.var_gen.fresh();
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
                return self.var_gen.fresh();
            }
        };

        match self.env.lookup_method(&parent_name, &method_name) {
            Some(mi) => mi.return_type,
            None => {
                self.errors.push(format!(
                    "El padre '{}' no tiene método '{}'.", parent_name, method_name
                ));
                self.var_gen.fresh()
            }
        }
    }
}