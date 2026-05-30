use std::collections::HashMap;
use crate::{
    expr_visitor::ExprVisitor,
    nodes::{
        binaryop_node::BinaryOp,
        block_node::BlockNode,
        destassing_node::DestAssignNode,
        for_node::ForNode,
        funcall_node::FunCallNode,
        if_node::IfNode,
        let_node::LetNode,
        literal_node::Literal,
        member_access_node::{MemberAccessNode, MethodCallNode},
        program_node::{Program, Statement},
        type_decl_node::TypeDeclNode,
        expr_node::{Expr, HulkType},
        unaryop_node::UnaryOp,
        while_node::WhileNode,
        instantiation_node::InstantiationNode,
    },
};

// ---------------------------------------------------------------------------
// Información de tipo registrada por el checkeador
// ---------------------------------------------------------------------------

/// Atributo conocido de un tipo: (nombre, tipo declarado).
#[derive(Clone, Debug)]
pub struct FieldInfo {
    pub name: String,
    pub hulk_type: HulkType,
}

/// Método conocido de un tipo: firma completa.
#[derive(Clone, Debug)]
pub struct MethodInfo {
    pub name: String,
    pub param_types: Vec<HulkType>,
    pub return_type: HulkType,
}

/// Toda la información semántica de un tipo HULK declarado.
#[derive(Clone, Debug)]
pub struct TypeInfo {
    pub name: String,
    pub params: Vec<HulkType>,         // tipos de los parámetros del constructor
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
    pub parent: Option<String>,        // nombre del tipo padre (herencia)
}

// ---------------------------------------------------------------------------
// Entorno (scope stack)
// ---------------------------------------------------------------------------

pub struct Environment {
    /// Pila de scopes de variables: el último es el más interno.
    scopes: Vec<HashMap<String, HulkType>>,
    /// Funciones globales: nombre → (param_types, return_type)
    pub functions: HashMap<String, (Vec<HulkType>, HulkType)>,
    /// Tipos declarados
    pub types: HashMap<String, TypeInfo>,
    /// Tipo del objeto actual dentro de un método (nombre del tipo HULK).
    pub self_type: Option<String>,
    /// Tipo padre del método actual (para `base()`)
    pub current_parent: Option<String>,
    /// Nombre del método actual (para `base()`)
    pub current_method: Option<String>,
}

impl Environment {
    pub fn new() -> Self {
        let mut env = Self {
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            types: HashMap::new(),
            self_type: None,
            current_parent: None,
            current_method: None,
        };
        env.register_builtins();
        env
    }

    /// Registra las funciones built-in del lenguaje HULK.
    fn register_builtins(&mut self) {
        // print(x) -> Any (devuelve lo que recibe, simplificado como Unknown)
        self.functions.insert("print".to_string(), (vec![HulkType::Unknown], HulkType::Unknown));
        // sqrt(x) -> Number
        self.functions.insert("sqrt".to_string(), (vec![HulkType::Number], HulkType::Number));
        // sin(x) -> Number
        self.functions.insert("sin".to_string(), (vec![HulkType::Number], HulkType::Number));
        // cos(x) -> Number
        self.functions.insert("cos".to_string(), (vec![HulkType::Number], HulkType::Number));
        // exp(x) -> Number
        self.functions.insert("exp".to_string(), (vec![HulkType::Number], HulkType::Number));
        // log(base, x) -> Number
        self.functions.insert("log".to_string(), (vec![HulkType::Number, HulkType::Number], HulkType::Number));
        // rand() -> Number
        self.functions.insert("rand".to_string(), (vec![], HulkType::Number));
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn define(&mut self, name: String, t: HulkType) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, t);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&HulkType> {
        for scope in self.scopes.iter().rev() {
            if let Some(t) = scope.get(name) {
                return Some(t);
            }
        }
        None
    }

    pub fn assign(&mut self, name: &str, t: HulkType) -> bool {
        for scope in self.scopes.iter_mut().rev() {
            if scope.contains_key(name) {
                scope.insert(name.to_string(), t);
                return true;
            }
        }
        false
    }

    /// Resuelve el LCA (lowest common ancestor) de dos tipos en la jerarquía.
    pub fn lca(&self, a: &HulkType, b: &HulkType) -> HulkType {
        if a == b {
            return a.clone();
        }
        // Si alguno es Unknown (error previo), propagar Unknown
        if *a == HulkType::Unknown || *b == HulkType::Unknown {
            return HulkType::Unknown;
        }
        match (a, b) {
            (HulkType::Class(ca), HulkType::Class(cb)) => {
                let ancestors_a = self.ancestors(ca);
                let ancestors_b = self.ancestors(cb);
                for anc in &ancestors_a {
                    if ancestors_b.contains(anc) {
                        return HulkType::Class(anc.clone());
                    }
                }
                HulkType::Unknown
            }
            _ => HulkType::Unknown,
        }
    }

    /// Devuelve la cadena de ancestros de un tipo (incluido él mismo), de más específico a más general.
    fn ancestors(&self, type_name: &str) -> Vec<String> {
        let mut result = vec![type_name.to_string()];
        let mut current = type_name.to_string();
        loop {
            match self.types.get(&current).and_then(|ti| ti.parent.clone()) {
                Some(parent) => {
                    result.push(parent.clone());
                    current = parent;
                }
                None => break,
            }
        }
        result
    }

    /// Comprueba si `sub` conforma (es subtipo de) `sup`.
    pub fn conforms(&self, sub: &HulkType, sup: &HulkType) -> bool {
        if sub == sup {
            return true;
        }
        if *sup == HulkType::Unknown {
            return true; // Unknown acepta todo (para funciones sin tipo anotado)
        }
        match (sub, sup) {
            (HulkType::Class(cs), HulkType::Class(cp)) => {
                self.ancestors(cs).contains(cp)
            }
            _ => false,
        }
    }
}

// ---------------------------------------------------------------------------
// Checker principal
// ---------------------------------------------------------------------------

pub struct SemanticChecker {
    pub env: Environment,
    pub errors: Vec<String>,
}

impl SemanticChecker {
    pub fn new() -> Self {
        Self {
            env: Environment::new(),
            errors: Vec::new(),
        }
    }

    pub fn check_program(&mut self, program: &mut Program) {
        // Pasada 1: Recolectar declaraciones de tipos (sólo sus nombres)
        self.pass1_collect_type_names(program);
        // Pasada 2: Recolectar miembros de tipos (atributos y métodos)
        self.pass2_collect_type_members(program);
        // Pasada 3: Recolectar firmas de funciones globales
        self.pass3_collect_functions(program);
        // Pasada 4: Chequear cuerpos de todo
        self.pass4_check_bodies(program);
    }

    // -----------------------------------------------------------------------
    // PASADA 1: Registrar nombres de tipos (sin cuerpos)
    // Esto permite que tipos se referencien mutuamente.
    // -----------------------------------------------------------------------
    fn pass1_collect_type_names(&mut self, program: &Program) {
        for stmt in &program.statements {
            if let Statement::TypeDecl(decl) = stmt {
                if let Literal::Id(ref type_name) = decl.name.value {
                    // Verificar duplicado
                    if self.env.types.contains_key(type_name) {
                        self.errors.push(format!(
                            "Error semántico: tipo '{}' ya está declarado.", type_name
                        ));
                        continue;
                    }
                    // Registrar tipo vacío (se completará en pasada 2)
                    self.env.types.insert(type_name.clone(), TypeInfo {
                        name: type_name.clone(),
                        params: vec![],
                        fields: vec![],
                        methods: vec![],
                        parent: None,
                    });
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // PASADA 2: Registrar miembros (atributos y métodos) de cada tipo
    // -----------------------------------------------------------------------
    fn pass2_collect_type_members(&mut self, program: &Program) {
        for stmt in &program.statements {
            if let Statement::TypeDecl(decl) = stmt {
                self.register_type_members(decl);
            }
        }
    }

    fn register_type_members(&mut self, decl: &TypeDeclNode) {
        if let Literal::Id(ref type_name) = decl.name.value {
            let params: Vec<HulkType> = decl.params.iter()
                .map(|(_, t)| t.clone())
                .collect();

            let fields: Vec<FieldInfo> = decl.attributes.iter().map(|attr| {
                FieldInfo {
                    name: attr.name.value.as_id(),
                    hulk_type: attr.type_annotation.clone(),
                }
            }).collect();

            let methods: Vec<MethodInfo> = decl.methods.iter().map(|m| {
                MethodInfo {
                    name: m.name.value.as_id(),
                    param_types: m.params.iter().map(|(_, t)| t.clone()).collect(),
                    return_type: m.return_type.clone(),
                }
            }).collect();

            let parent: Option<String> = decl.inheritance.as_ref().map(|inh| {
                inh.parent_name.value.as_id()
            });

            // Validar que el padre existe
            if let Some(ref parent_name) = parent {
                if !self.env.types.contains_key(parent_name) {
                    self.errors.push(format!(
                        "Error semántico: el tipo '{}' hereda de '{}' que no está declarado.",
                        type_name, parent_name
                    ));
                }
            }

            if let Some(ti) = self.env.types.get_mut(type_name) {
                ti.params = params;
                ti.fields = fields;
                ti.methods = methods;
                ti.parent = parent;
            }
        }
    }

    // -----------------------------------------------------------------------
    // PASADA 3: Registrar firmas de funciones globales
    // -----------------------------------------------------------------------
    fn pass3_collect_functions(&mut self, program: &Program) {
        for stmt in &program.statements {
            if let Statement::FunctionDecl(decl) = stmt {
                if let Literal::Id(ref name) = decl.name.value {
                    if self.env.functions.contains_key(name) {
                        self.errors.push(format!(
                            "Error semántico: función '{}' ya está declarada.", name
                        ));
                        continue;
                    }
                    let param_types = decl.params.iter()
                        .map(|(_, t)| t.clone())
                        .collect();
                    // El return_type en FunctionDecl puede ser Unknown si no está anotado
                    self.env.functions.insert(name.clone(), (param_types, decl.return_type.clone()));
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // PASADA 4: Chequear cuerpos de funciones, tipos y expresiones globales
    // -----------------------------------------------------------------------
    fn pass4_check_bodies(&mut self, program: &mut Program) {
        for stmt in &mut program.statements {
            match stmt {
                Statement::FunctionDecl(decl) => {
                    // Crear scope con los parámetros
                    self.env.push_scope();
                    for (p_name, p_type) in &decl.params {
                        if let Literal::Id(ref id) = p_name.value {
                            self.env.define(id.clone(), p_type.clone());
                        }
                    }
                    let inferred = decl.body.accept(self);

                    // Si tenemos anotación de retorno, verificar conformancia
                    if decl.return_type != HulkType::Unknown
                        && !self.env.conforms(&inferred, &decl.return_type)
                    {
                        self.errors.push(format!(
                            "Error semántico en función '{}': se esperaba retorno {:?}, se infirió {:?}.",
                            decl.name.value.as_id(), decl.return_type, inferred
                        ));
                    }

                    // Actualizar el tipo de retorno inferido en el registro global
                    if let Literal::Id(ref name) = decl.name.value {
                        if let Some(func) = self.env.functions.get_mut(name) {
                            if func.1 == HulkType::Unknown {
                                func.1 = inferred;
                            }
                        }
                    }

                    self.env.pop_scope();
                }

                Statement::TypeDecl(decl) => {
                    self.check_type_decl(decl);
                }

                Statement::Expression(expr) => {
                    expr.accept(self);
                }
            }
        }
    }

    fn check_type_decl(&mut self, decl: &mut TypeDeclNode) {
        if let Literal::Id(ref type_name) = decl.name.value.clone() {
            let old_self = self.env.self_type.take();

            self.env.self_type = Some(type_name.clone());

            // Chequear inicializadores de atributos con los parámetros del constructor en scope
            self.env.push_scope();
            for (p_name, p_type) in &decl.params {
                if let Literal::Id(ref id) = p_name.value {
                    self.env.define(id.clone(), p_type.clone());
                }
            }

            for attr in &mut decl.attributes {
                let inferred = attr.initializer.accept(self);
                // Si tiene anotación, verificar
                if attr.type_annotation != HulkType::Unknown
                    && !self.env.conforms(&inferred, &attr.type_annotation)
                {
                    self.errors.push(format!(
                        "Error semántico en atributo '{}.{}': esperado {:?}, inferido {:?}.",
                        type_name, attr.name.value.as_id(), attr.type_annotation, inferred
                    ));
                }
                // Actualizar tipo del atributo si era Unknown
                if attr.type_annotation == HulkType::Unknown {
                    attr.set_type(inferred);
                }
            }
            self.env.pop_scope();

            // Determinar padre para base()
            let parent_name = self.env.types.get(type_name)
                .and_then(|ti| ti.parent.clone());

            // Chequear métodos
            for method in &mut decl.methods {
                self.env.push_scope();

                // Poner parámetros en scope
                for (p_name, p_type) in &method.params {
                    if let Literal::Id(ref id) = p_name.value {
                        self.env.define(id.clone(), p_type.clone());
                    }
                }

                // Configurar contexto para base()
                let old_parent = self.env.current_parent.take();
                let old_method = self.env.current_method.take();
                self.env.current_parent = parent_name.clone();
                self.env.current_method = Some(method.name.value.as_id());

                let inferred_ret = method.body.accept(self);

                // Verificar tipo de retorno del método si está anotado
                if method.return_type != HulkType::Unknown
                    && !self.env.conforms(&inferred_ret, &method.return_type)
                {
                    self.errors.push(format!(
                        "Error semántico en método '{}.{}': se esperaba {:?}, se infirió {:?}.",
                        type_name, method.name.value.as_id(), method.return_type, inferred_ret
                    ));
                }

                // Actualizar tipo de retorno en TypeInfo
                let method_name = method.name.value.as_id();
                if let Some(ti) = self.env.types.get_mut(type_name) {
                    if let Some(mi) = ti.methods.iter_mut().find(|m| m.name == method_name) {
                        if mi.return_type == HulkType::Unknown {
                            mi.return_type = inferred_ret;
                        }
                    }
                }

                self.env.current_parent = old_parent;
                self.env.current_method = old_method;
                self.env.pop_scope();
            }

            self.env.self_type = old_self;
        }
    }

    /// Busca un método subiendo por la jerarquía de herencia.
    fn lookup_method(&self, type_name: &str, method_name: &str) -> Option<MethodInfo> {
        let mut current = type_name.to_string();
        loop {
            if let Some(ti) = self.env.types.get(&current) {
                if let Some(mi) = ti.methods.iter().find(|m| m.name == method_name) {
                    return Some(mi.clone());
                }
                match &ti.parent {
                    Some(p) => current = p.clone(),
                    None => return None,
                }
            } else {
                return None;
            }
        }
    }

    /// Busca un campo subiendo por la jerarquía de herencia.
    fn lookup_field(&self, type_name: &str, field_name: &str) -> Option<FieldInfo> {
        let mut current = type_name.to_string();
        loop {
            if let Some(ti) = self.env.types.get(&current) {
                if let Some(fi) = ti.fields.iter().find(|f| f.name == field_name) {
                    return Some(fi.clone());
                }
                match &ti.parent {
                    Some(p) => current = p.clone(),
                    None => return None,
                }
            } else {
                return None;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ExprVisitor<HulkType>: núcleo del chequeo semántico
// ---------------------------------------------------------------------------

impl ExprVisitor<HulkType> for SemanticChecker {
    // ---- Literales ----

    fn visit_number(&mut self, _n: f32) -> HulkType {
        HulkType::Number
    }

    fn visit_bool(&mut self, _b: bool) -> HulkType {
        HulkType::Bool
    }

    fn visit_string(&mut self, _s: &str) -> HulkType {
        HulkType::String
    }

    fn visit_id(&mut self, id: &str) -> HulkType {
        match self.env.lookup(id) {
            Some(t) => t.clone(),
            None => {
                self.errors.push(format!(
                    "Error semántico: variable '{}' no declarada.", id
                ));
                HulkType::Unknown
            }
        }
    }

    // ---- self ----

    fn visit_self(&mut self) -> HulkType {
        match &self.env.self_type {
            Some(t) => HulkType::Class(t.clone()),
            None => {
                self.errors.push(
                    "Error semántico: 'self' usado fuera del cuerpo de un método.".to_string()
                );
                HulkType::Unknown
            }
        }
    }

    // ---- base() ----

    fn visit_base_call(&mut self, args: &[Expr]) -> HulkType {
        // base() delega al método del padre con el mismo nombre
        let parent_name = match &self.env.current_parent {
            Some(p) => p.clone(),
            None => {
                self.errors.push(
                    "Error semántico: 'base()' usado en un tipo sin padre.".to_string()
                );
                return HulkType::Unknown;
            }
        };

        let method_name = match &self.env.current_method {
            Some(m) => m.clone(),
            None => {
                self.errors.push(
                    "Error semántico: 'base()' usado fuera de un método.".to_string()
                );
                return HulkType::Unknown;
            }
        };

        // Evaluar argumentos
        let arg_types: Vec<HulkType> = args.iter()
            .map(|a| a.accept(self))
            .collect();

        match self.lookup_method(&parent_name, &method_name) {
            Some(mi) => {
                // Verificar aridad
                if arg_types.len() != mi.param_types.len() {
                    self.errors.push(format!(
                        "Error semántico: 'base()' en '{}': el método padre '{}.{}' espera {} argumento(s), se proveyeron {}.",
                        self.env.self_type.as_deref().unwrap_or("?"),
                        parent_name, method_name,
                        mi.param_types.len(), arg_types.len()
                    ));
                } else {
                    for (i, (got, expected)) in arg_types.iter().zip(mi.param_types.iter()).enumerate() {
                        if *expected != HulkType::Unknown && !self.env.conforms(got, expected) {
                            self.errors.push(format!(
                                "Error semántico: argumento {} de 'base()' en '{}.{}': esperado {:?}, obtenido {:?}.",
                                i + 1, parent_name, method_name, expected, got
                            ));
                        }
                    }
                }
                mi.return_type.clone()
            }
            None => {
                self.errors.push(format!(
                    "Error semántico: el tipo padre '{}' no define el método '{}'.",
                    parent_name, method_name
                ));
                HulkType::Unknown
            }
        }
    }

    // ---- Operador binario ----

    fn visit_binary_op(&mut self, left: &Expr, op: &BinaryOp, right: &Expr) -> HulkType {
        let left_t = left.accept(self);
        let right_t = right.accept(self);

        match op {
            // Aritmética: Number x Number -> Number
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul
            | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
                if left_t == HulkType::Number && right_t == HulkType::Number {
                    HulkType::Number
                } else {
                    self.errors.push(format!(
                        "Error semántico: operación aritmética requiere operandos numéricos, se obtuvo {:?} y {:?}.",
                        left_t, right_t
                    ));
                    HulkType::Unknown
                }
            }
            // Comparación de igualdad: cualquier tipo -> Bool
            BinaryOp::Equal | BinaryOp::Dist => HulkType::Bool,
            // Comparación relacional: Number x Number -> Bool
            BinaryOp::Great | BinaryOp::Less | BinaryOp::Gequa | BinaryOp::Lequa => {
                if left_t == HulkType::Number && right_t == HulkType::Number {
                    HulkType::Bool
                } else {
                    self.errors.push(format!(
                        "Error semántico: comparación relacional requiere operandos numéricos, se obtuvo {:?} y {:?}.",
                        left_t, right_t
                    ));
                    HulkType::Unknown
                }
            }
            // Lógica: Bool x Bool -> Bool
            BinaryOp::And | BinaryOp::Or => {
                if left_t == HulkType::Bool && right_t == HulkType::Bool {
                    HulkType::Bool
                } else {
                    self.errors.push(format!(
                        "Error semántico: operación lógica requiere operandos booleanos, se obtuvo {:?} y {:?}.",
                        left_t, right_t
                    ));
                    HulkType::Unknown
                }
            }
            // Concatenación: al menos uno debe ser String -> String
            BinaryOp::SingleConc | BinaryOp::SpacedConc => {
                let valid = matches!((&left_t, &right_t),
                    (HulkType::String, _) | (_, HulkType::String)
                    | (HulkType::Number, HulkType::Number)
                );
                if valid {
                    HulkType::String
                } else {
                    self.errors.push(format!(
                        "Error semántico: concatenación requiere al menos un operando String, se obtuvo {:?} y {:?}.",
                        left_t, right_t
                    ));
                    HulkType::Unknown
                }
            }
        }
    }

    // ---- Operador unario ----

    fn visit_unary_op(&mut self, op: &UnaryOp, expr: &Expr) -> HulkType {
        let t = expr.accept(self);
        match op {
            UnaryOp::Not => {
                if t != HulkType::Bool {
                    self.errors.push(format!(
                        "Error semántico: '!' requiere operando booleano, se obtuvo {:?}.", t
                    ));
                }
                HulkType::Bool
            }
            UnaryOp::Neg | UnaryOp::Plus => {
                if t != HulkType::Number {
                    self.errors.push(format!(
                        "Error semántico: operador unario numérico requiere Number, se obtuvo {:?}.", t
                    ));
                }
                HulkType::Number
            }
        }
    }

    // ---- let ----

    fn visit_let(&mut self, node: &LetNode) -> HulkType {
        // Cada binding crea un scope propio (scoping secuencial)
        let mut scopes_opened = 0;

        for ((id_node, decl_type), value_expr) in &node.assignments {
            let inferred = value_expr.accept(self);

            // Determinar tipo efectivo
            let effective_type = if *decl_type != HulkType::Unknown {
                if !self.env.conforms(&inferred, decl_type) {
                    self.errors.push(format!(
                        "Error semántico en let '{}': tipo declarado {:?} no conforma con tipo inferido {:?}.",
                        id_node.value.as_id(), decl_type, inferred
                    ));
                }
                decl_type.clone()
            } else {
                inferred
            };

            self.env.push_scope();
            scopes_opened += 1;

            if let Literal::Id(ref name) = id_node.value {
                self.env.define(name.clone(), effective_type);
            }
        }

        let ret = node.body.accept(self);

        for _ in 0..scopes_opened {
            self.env.pop_scope();
        }

        ret
    }

    // ---- if / elif / else ----

    fn visit_if(&mut self, node: &IfNode) -> HulkType {
        let cond_t = node.condition.accept(self);
        if cond_t != HulkType::Bool {
            self.errors.push(format!(
                "Error semántico: la condición del 'if' debe ser booleana, se obtuvo {:?}.", cond_t
            ));
        }

        let mut branch_type = node.if_branch.accept(self);

        for (elif_cond, elif_body) in &node.elif_branches {
            let ec_t = elif_cond.accept(self);
            if ec_t != HulkType::Bool {
                self.errors.push(format!(
                    "Error semántico: la condición del 'elif' debe ser booleana, se obtuvo {:?}.", ec_t
                ));
            }
            let eb_t = elif_body.accept(self);
            branch_type = self.env.lca(&branch_type, &eb_t);
        }

        let else_t = node.else_branch.accept(self);
        self.env.lca(&branch_type, &else_t)
    }

    // ---- while ----

    fn visit_while(&mut self, node: &WhileNode) -> HulkType {
        let cond_t = node.condition.accept(self);
        if cond_t != HulkType::Bool {
            self.errors.push(format!(
                "Error semántico: la condición del 'while' debe ser booleana, se obtuvo {:?}.", cond_t
            ));
        }
        node.body.accept(self);
        // while retorna Unknown (puede no ejecutarse)
        HulkType::Unknown
    }

    // ---- for ----

    fn visit_for(&mut self, node: &ForNode) -> HulkType {
        // El iterador debe ser algún tipo iterable; simplificado: aceptamos cualquier tipo
        node.iterator.accept(self);

        self.env.push_scope();
        if let Literal::Id(ref var_name) = node.variable.value {
            // La variable de iteración tiene tipo Unknown (depende del iterable)
            self.env.define(var_name.clone(), HulkType::Unknown);
        }
        let body_t = node.body.accept(self);
        self.env.pop_scope();

        body_t
    }

    // ---- llamada a función ----

    fn visit_fun_call(&mut self, node: &FunCallNode) -> HulkType {
        if let Literal::Id(ref name) = node.name.value {
            // Evaluar argumentos primero
            let arg_types: Vec<HulkType> = node.args.iter()
                .map(|a| a.accept(self))
                .collect();

            match self.env.functions.get(name).cloned() {
                Some((param_types, ret_type)) => {
                    // Chequear aridad (Unknown en param_types = print que acepta cualquier cosa)
                    if param_types.len() != arg_types.len()
                        && !(param_types.len() == 1 && param_types[0] == HulkType::Unknown)
                    {
                        self.errors.push(format!(
                            "Error semántico: la función '{}' espera {} argumento(s), se proveyeron {}.",
                            name, param_types.len(), arg_types.len()
                        ));
                    } else {
                        for (i, (got, expected)) in arg_types.iter().zip(param_types.iter()).enumerate() {
                            if *expected != HulkType::Unknown && !self.env.conforms(got, expected) {
                                self.errors.push(format!(
                                    "Error semántico: argumento {} de '{}': esperado {:?}, obtenido {:?}.",
                                    i + 1, name, expected, got
                                ));
                            }
                        }
                    }
                    ret_type
                }
                None => {
                    self.errors.push(format!(
                        "Error semántico: función '{}' no declarada.", name
                    ));
                    HulkType::Unknown
                }
            }
        } else {
            HulkType::Unknown
        }
    }

    // ---- asignación destructiva (:=) ----

    fn visit_dest_assign(&mut self, node: &DestAssignNode) -> HulkType {
        // El target no puede ser `self`
        if let Expr::SelfRef = node.target.as_ref() {
            self.errors.push(
                "Error semántico: 'self' no puede ser el target de una asignación ':='.".to_string()
            );
            return HulkType::Unknown;
        }

        let expr_type = node.expr.accept(self);

        match node.target.as_ref() {
            Expr::Literal(lit_node) => {
                if let Literal::Id(ref name) = lit_node.value {
                    let existing = self.env.lookup(name).cloned();
                    match existing {
                        Some(existing_type) => {
                            if !self.env.conforms(&expr_type, &existing_type) {
                                self.errors.push(format!(
                                    "Error semántico: asignación ':=' de '{}': tipo {:?} no conforma con tipo declarado {:?}.",
                                    name, expr_type, existing_type
                                ));
                            }
                            // Actualizar el tipo en el scope
                            self.env.assign(name, expr_type.clone());
                        }
                        None => {
                            self.errors.push(format!(
                                "Error semántico: variable '{}' no declarada en asignación ':='.", name
                            ));
                        }
                    }
                }
            }
            // target puede ser un MemberAccess (self.campo := ...)
            Expr::MemberAccess(ma_node) => {
                // Verificar que la instancia existe y el campo también
                let inst_type = ma_node.instance.accept(self);
                if let HulkType::Class(ref type_name) = inst_type {
                    if let Literal::Id(ref field_name) = ma_node.member.value {
                        match self.lookup_field(type_name, field_name) {
                            Some(fi) => {
                                if fi.hulk_type != HulkType::Unknown
                                    && !self.env.conforms(&expr_type, &fi.hulk_type)
                                {
                                    self.errors.push(format!(
                                        "Error semántico: asignación a '{}.{}': tipo {:?} no conforma con {:?}.",
                                        type_name, field_name, expr_type, fi.hulk_type
                                    ));
                                }
                            }
                            None => {
                                self.errors.push(format!(
                                    "Error semántico: el tipo '{}' no tiene atributo '{}'.",
                                    type_name, field_name
                                ));
                            }
                        }
                    }
                }
            }
            _ => {
                self.errors.push(
                    "Error semántico: target de ':=' inválido; debe ser una variable o un miembro.".to_string()
                );
            }
        }

        expr_type
    }

    // ---- bloque { e1; e2; ... eN } ----

    fn visit_block(&mut self, node: &BlockNode) -> HulkType {
        let mut last_type = HulkType::Unknown;
        for expr in &node.expressions {
            last_type = expr.accept(self);
        }
        last_type
    }

    // ---- instanciación: new Tipo(args...) ----

    fn visit_instantiation(&mut self, node: &InstantiationNode) -> HulkType {
        if let Literal::Id(ref type_name) = node.name.value {
            let arg_types: Vec<HulkType> = node.args.iter()
                .map(|a| a.accept(self))
                .collect();
         

            match self.env.types.get(type_name).cloned() {
                Some(ti) => {
                    let parent=ti.parent.clone();
                    let parent_const=self.env.types.get(&parent.unwrap_or_default()).and_then(|pti| Some(pti.params.clone()));
                    if arg_types.len() != ti.params.len() && (parent_const.is_some() && arg_types.len() != parent_const.as_ref().unwrap_or(&vec![]).len()) {
                        self.errors.push(format!(
                            "Error semántico: el tipo '{}' espera {} argumento(s) de constructor, se proveyeron {}.",
                            type_name, ti.params.len(), arg_types.len()
                        ));
                    } else {
                        for (i, (got, expected)) in arg_types.iter().zip(ti.params.iter()).enumerate() {
                            if *expected != HulkType::Unknown && !self.env.conforms(got, expected) {
                                self.errors.push(format!(
                                    "Error semántico: argumento {} del constructor de '{}': esperado {:?}, obtenido {:?}.",
                                    i + 1, type_name, expected, got
                                ));
                            }
                        }
                    }
                    HulkType::Class(type_name.clone())
                }
                None => {
                    self.errors.push(format!(
                        "Error semántico: tipo '{}' no declarado.", type_name
                    ));
                    HulkType::Unknown
                }
            }
        } else {
            HulkType::Unknown
        }
    }

    // ---- acceso a miembro: expr.campo ----

    fn visit_member_access(&mut self, node: &MemberAccessNode) -> HulkType {
        let inst_type = node.instance.accept(self);

        if let HulkType::Class(ref type_name) = inst_type {
            if let Literal::Id(ref field_name) = node.member.value {
                match self.lookup_field(type_name, field_name) {
                    Some(fi) => return fi.hulk_type,
                    None => {
                        self.errors.push(format!(
                            "Error semántico: el tipo '{}' no tiene atributo '{}'.",
                            type_name, field_name
                        ));
                    }
                }
            }
        } else if inst_type != HulkType::Unknown {
            self.errors.push(format!(
                "Error semántico: acceso a miembro sobre un tipo no-clase: {:?}.", inst_type
            ));
        }

        HulkType::Unknown
    }

    // ---- llamada a método: expr.metodo(args...) ----

    fn visit_method_call(&mut self, node: &MethodCallNode) -> HulkType {
        let inst_type = node.instance.accept(self);

        let arg_types: Vec<HulkType> = node.call.args.iter()
            .map(|a| a.accept(self))
            .collect();

        if let HulkType::Class(ref type_name) = inst_type {
            if let Literal::Id(ref method_name) = node.call.name.value {
                match self.lookup_method(type_name, method_name) {
                    Some(mi) => {
                        // Verificar aridad
                        if arg_types.len() != mi.param_types.len() {
                            self.errors.push(format!(
                                "Error semántico: el método '{}.{}' espera {} argumento(s), se proveyeron {}.",
                                type_name, method_name, mi.param_types.len(), arg_types.len()
                            ));
                        } else {
                            for (i, (got, expected)) in arg_types.iter().zip(mi.param_types.iter()).enumerate() {
                                if *expected != HulkType::Unknown && !self.env.conforms(got, expected) {
                                    self.errors.push(format!(
                                        "Error semántico: argumento {} de '{}.{}': esperado {:?}, obtenido {:?}.",
                                        i + 1, type_name, method_name, expected, got
                                    ));
                                }
                            }
                        }
                        return mi.return_type;
                    }
                    None => {
                        self.errors.push(format!(
                            "Error semántico: el tipo '{}' (ni sus ancestros) tiene el método '{}'.",
                            type_name, method_name
                        ));
                    }
                }
            }
        } else if inst_type != HulkType::Unknown {
            self.errors.push(format!(
                "Error semántico: llamada a método sobre tipo no-clase: {:?}.", inst_type
            ));
        }

        HulkType::Unknown
    }
}
