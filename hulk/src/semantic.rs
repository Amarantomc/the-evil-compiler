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
        typedexpr_node::{Expr, HulkType, TypedExpr},
        unaryop_node::UnaryOp,
        while_node::WhileNode,
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
    pub params: Vec<HulkType>,   // tipos de los parámetros del constructor
    pub fields: Vec<FieldInfo>,
    pub methods: Vec<MethodInfo>,
}

// ---------------------------------------------------------------------------
// Entorno
// ---------------------------------------------------------------------------

pub struct Environment {
    pub variables: HashMap<String, HulkType>,
    pub functions: HashMap<String, (Vec<HulkType>, HulkType)>,
    pub types: HashMap<String, TypeInfo>,
    /// Tipo del objeto actual dentro de un método (nombre del tipo HULK).
    pub self_type: Option<String>,
}

impl Environment {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            functions: HashMap::new(),
            types: HashMap::new(),
            self_type: None,
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
        self.pass_1_collect_types(program);
        self.pass_2_collect_functions(program);
        self.pass_3_check_bodies(program);
    }

    // PASADA 0: Registrar tipos y sus miembros
    fn pass_1_collect_types(&mut self, program: &Program) {
        for stmt in &program.statements {
            if let Statement::TypeDecl(decl) = stmt {
                self.register_type(decl);
            }
        }
    }

    fn register_type(&mut self, decl: &TypeDeclNode) {
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
                    return_type: HulkType::Unknown, // se infiere en pasada 3
                }
            }).collect();

            self.env.types.insert(type_name.clone(), TypeInfo {
                name: type_name.clone(),
                params,
                fields,
                methods,
            });
        }
    }

    // PASADA 1: Registrar firmas de funciones globales
    fn pass_2_collect_functions(&mut self, program: &Program) {
        for stmt in &program.statements {
            if let Statement::FunctionDecl(decl) = stmt {
                if let Literal::Id(ref name) = decl.name.value {
                    let param_types = decl.params.iter()
                        .map(|(_, t)| t.clone())
                        .collect();
                    self.env.functions.insert(name.clone(), (param_types, HulkType::Unknown));
                }
            }
        }
    }

    // PASADA 2: Chequear tipos de cuerpos
    fn pass_3_check_bodies(&mut self, program: &mut Program) {
        for stmt in &mut program.statements {
            match stmt {
                Statement::FunctionDecl(decl) => {
                    let old_vars = self.env.variables.clone();
                    for (p_name, p_type) in &decl.params {
                        if let Literal::Id(ref id) = p_name.value {
                            self.env.variables.insert(id.clone(), p_type.clone());
                        }
                    }
                    let inferred_type = decl.body.accept(self);
                    if let Literal::Id(ref name) = decl.name.value {
                        if let Some(func) = self.env.functions.get_mut(name) {
                            func.1 = inferred_type;
                        }
                    }
                    self.env.variables = old_vars;
                }
                Statement::TypeDecl(decl) => {
                    self.check_type_decl(decl);
                }
                Statement::Expression(expr) => {
                    expr.return_type = expr.accept(self);
                }
            }
        }
    }

    fn check_type_decl(&mut self, decl: &mut TypeDeclNode) {
        if let Literal::Id(ref type_name) = decl.name.value.clone() {
            // Entrar en el contexto del tipo: self tiene este tipo
            let old_self = self.env.self_type.take();
            self.env.self_type = Some(type_name.clone());

            // Chequear inicializadores de atributos (sin self en scope)
            let old_vars = self.env.variables.clone();
            for (p_name, p_type) in &decl.params {
                if let Literal::Id(ref id) = p_name.value {
                    self.env.variables.insert(id.clone(), p_type.clone());
                }
            }
            for attr in &mut decl.attributes {
                let inferred = attr.initializer.accept(self);
                if attr.type_annotation != HulkType::Unknown && inferred != attr.type_annotation {
                    self.errors.push(format!(
                        "Tipo incorrecto en atributo '{}': esperado {:?}, encontrado {:?}",
                        attr.name.value.as_id(), attr.type_annotation, inferred
                    ));
                }
            }
            self.env.variables = old_vars;

            // Chequear cuerpos de métodos (self en scope como tipo del tipo)
            for method in &mut decl.methods {
                let old_vars2 = self.env.variables.clone();
                for (p_name, p_type) in &method.params {
                    if let Literal::Id(ref id) = p_name.value {
                        self.env.variables.insert(id.clone(), p_type.clone());
                    }
                }
                let ret = method.body.accept(self);
                // Actualizar tipo de retorno en TypeInfo
                if let Some(ti) = self.env.types.get_mut(type_name) {
                    let method_name = method.name.value.as_id();
                    if let Some(m) = ti.methods.iter_mut().find(|m| m.name == method_name) {
                        m.return_type = ret;
                    }
                }
                self.env.variables = old_vars2;
            }

            self.env.self_type = old_self;
        }
    }
}

// ---------------------------------------------------------------------------
// ExprVisitor<HulkType>
// ---------------------------------------------------------------------------

impl ExprVisitor<HulkType> for SemanticChecker {
    fn visit_number(&mut self, _n: f32) -> HulkType { HulkType::Number }
    fn visit_bool(&mut self, _b: bool) -> HulkType  { HulkType::Bool   }
    fn visit_string(&mut self, _s: &str) -> HulkType { HulkType::String }

    fn visit_id(&mut self, id: &str) -> HulkType {
        match self.env.variables.get(id) {
            Some(t) => t.clone(),
            None => {
                self.errors.push(format!("Variable no declarada: '{}'", id));
                HulkType::Unknown
            }
        }
    }

    /// Retorna el tipo de la instancia actual.
    /// Emite error si se usa fuera de un método (self_type == None).
    fn visit_self(&mut self) -> HulkType {
        match &self.env.self_type {
            Some(t) => HulkType::Class(t.clone()),
            None => {
                self.errors.push(
                    "'self' usado fuera del cuerpo de un método".to_string()
                );
                HulkType::Unknown
            }
        }
    }

    fn visit_binary_op(&mut self, left: &TypedExpr, op: &BinaryOp, right: &TypedExpr) -> HulkType {
        let left_type  = left.accept(self);
        let right_type = right.accept(self);

        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod | BinaryOp::Pow => {
                if left_type == HulkType::Number && right_type == HulkType::Number {
                    HulkType::Number
                } else {
                    self.errors.push("Operación aritmética requiere operandos numéricos".to_string());
                    HulkType::Unknown
                }
            }
            BinaryOp::Equal | BinaryOp::Dist => HulkType::Bool,
            BinaryOp::Great | BinaryOp::Less | BinaryOp::Gequa | BinaryOp::Lequa => {
                if left_type == HulkType::Number && right_type == HulkType::Number {
                    HulkType::Bool
                } else {
                    self.errors.push("Comparación requiere operandos numéricos".to_string());
                    HulkType::Unknown
                }
            }
            BinaryOp::And | BinaryOp::Or => {
                if left_type == HulkType::Bool && right_type == HulkType::Bool {
                    HulkType::Bool
                } else {
                    self.errors.push("Operación lógica requiere operandos booleanos".to_string());
                    HulkType::Unknown
                }
            }
            BinaryOp::SingleConc => todo!(),
            BinaryOp::SpacedConc => todo!(),
        }
    }

    fn visit_unary_op(&mut self, op: &UnaryOp, expr: &TypedExpr) -> HulkType {
        let t = expr.accept(self);
        match op {
            UnaryOp::Not => {
                if t != HulkType::Bool {
                    self.errors.push("'!' requiere un operando booleano".to_string());
                }
                HulkType::Bool
            }
            UnaryOp::Neg | UnaryOp::Plus => {
                if t != HulkType::Number {
                    self.errors.push("Negación/unario requiere un operando numérico".to_string());
                }
                HulkType::Number
            }
        }
    }

    fn visit_let(&mut self, node: &LetNode) -> HulkType {
        let mut old_vars = Vec::new();

        for ((id_node, decl_type), value_expr) in &node.assignments {
            let mut val_type = value_expr.accept(self);
            if *decl_type != HulkType::Unknown && val_type != *decl_type {
                self.errors.push(format!(
                    "Tipo incorrecto en let: declarado {:?}, inferido {:?}",
                    decl_type, val_type
                ));
            } else if *decl_type != HulkType::Unknown {
                val_type = decl_type.clone();
            }
            if let Literal::Id(ref name) = id_node.value {
                let previous = self.env.variables.insert(name.clone(), val_type);
                old_vars.push((name.clone(), previous));
            }
        }

        let ret = node.body.accept(self);

        for (name, previous) in old_vars {
            match previous {
                Some(prev) => { self.env.variables.insert(name, prev); }
                None       => { self.env.variables.remove(&name); }
            }
        }
        ret
    }

    fn visit_if(&mut self, node: &IfNode) -> HulkType {
        let cond_t = node.condition.accept(self);
        if cond_t != HulkType::Bool {
            self.errors.push("La condición del 'if' debe ser booleana".to_string());
        }
        let if_t   = node.if_branch.accept(self);
        let else_t = node.else_branch.accept(self);
        for (elif_cond, elif_branch) in &node.elif_branches {
            let et = elif_cond.accept(self);
            if et != HulkType::Bool {
                self.errors.push("La condición del 'elif' debe ser booleana".to_string());
            }
            elif_branch.accept(self);
        }
        if if_t == else_t { if_t } else { HulkType::Unknown }
    }

    fn visit_while(&mut self, node: &WhileNode) -> HulkType {
        let cond_t = node.condition.accept(self);
        if cond_t != HulkType::Bool {
            self.errors.push("La condición del 'while' debe ser booleana".to_string());
        }
        node.body.accept(self);
        HulkType::Unknown
    }

    fn visit_for(&mut self, node: &ForNode) -> HulkType {
        node.iterator.accept(self);
        node.body.accept(self)
    }

    fn visit_fun_call(&mut self, node: &FunCallNode) -> HulkType {
        if let Literal::Id(ref name) = node.name.value {
            if let Some((_expected_args, ret_type)) = self.env.functions.get(name).cloned() {
                for arg in &node.args { arg.accept(self); }
                return ret_type;
            } else {
                self.errors.push(format!("Función no definida: '{}'", name));
            }
        }
        HulkType::Unknown
    }

    fn visit_dest_assign(&mut self, node: &DestAssignNode) -> HulkType {
        // Detectar si el target es `self` (prohibido por la especificación)
        if let Expr::SelfRef = &node.target.kind {
            self.errors.push(
                "Error semántico: 'self' no es un target válido de asignación (':=')".to_string()
            );
            return HulkType::Unknown;
        }
        node.expr.accept(self)
    }

    fn visit_block(&mut self, node: &BlockNode) -> HulkType {
        let mut last_type = HulkType::Unknown;
        for expr in &node.expressions {
            last_type = expr.accept(self);
        }
        last_type
    }

    /// Verifica que el tipo exista y que los argumentos del constructor coincidan.
    fn visit_instantiation(&mut self, node: &crate::nodes::instantiation_node::InstantiationNode) -> HulkType {
        if let Literal::Id(ref type_name) = node.name.value {
            if let Some(type_info) = self.env.types.get(type_name).cloned() {
                // Chequear aridad del constructor
                if node.args.len() != type_info.params.len() {
                    self.errors.push(format!(
                        "El tipo '{}' espera {} argumento(s), se proveyeron {}",
                        type_name, type_info.params.len(), node.args.len()
                    ));
                } else {
                    for (arg_expr, expected_type) in node.args.iter().zip(type_info.params.iter()) {
                        let arg_type = arg_expr.accept(self);
                        if *expected_type != HulkType::Unknown && arg_type != *expected_type {
                            self.errors.push(format!(
                                "Argumento del constructor de '{}': esperado {:?}, encontrado {:?}",
                                type_name, expected_type, arg_type
                            ));
                        }
                    }
                }
                return HulkType::Class(type_name.clone());
            } else {
                self.errors.push(format!("Tipo no declarado: '{}'", type_name));
            }
        }
        HulkType::Unknown
    }

    /// Verifica que el miembro existe en el tipo de la instancia.
    fn visit_member_access(&mut self, node: &MemberAccessNode) -> HulkType {
        let inst_type = node.instance.accept(self);

        if let HulkType::Class(ref type_name) = inst_type {
            if let Literal::Id(ref field_name) = node.member.value {
                if let Some(type_info) = self.env.types.get(type_name) {
                    if let Some(field) = type_info.fields.iter().find(|f| &f.name == field_name) {
                        return field.hulk_type.clone();
                    } else {
                        self.errors.push(format!(
                            "El tipo '{}' no tiene atributo '{}'",
                            type_name, field_name
                        ));
                    }
                }
            }
        } else {
            self.errors.push(format!(
                "Acceso a miembro sobre tipo no-clase: {:?}",
                inst_type
            ));
        }

        HulkType::Unknown
    }

    /// Verifica que el método existe y que los argumentos coinciden.
    fn visit_method_call(&mut self, node: &MethodCallNode) -> HulkType {
        let inst_type = node.instance.accept(self);

        // Evaluar los argumentos en cualquier caso
        let arg_types: Vec<HulkType> = node.call.args.iter()
            .map(|a| a.accept(self))
            .collect();

        if let HulkType::Class(ref type_name) = inst_type {
            if let Literal::Id(ref method_name) = node.call.name.value {
                if let Some(type_info) = self.env.types.get(type_name).cloned() {
                    if let Some(method) = type_info.methods.iter().find(|m| &m.name == method_name) {
                        // Chequear aridad
                        if arg_types.len() != method.param_types.len() {
                            self.errors.push(format!(
                                "El método '{}.{}' espera {} argumento(s), se proveyeron {}",
                                type_name, method_name,
                                method.param_types.len(), arg_types.len()
                            ));
                        } else {
                            for (got, expected) in arg_types.iter().zip(method.param_types.iter()) {
                                if *expected != HulkType::Unknown && got != expected {
                                    self.errors.push(format!(
                                        "Argumento de '{}.{}': esperado {:?}, encontrado {:?}",
                                        type_name, method_name, expected, got
                                    ));
                                }
                            }
                        }
                        return method.return_type.clone();
                    } else {
                        self.errors.push(format!(
                            "El tipo '{}' no tiene método '{}'",
                            type_name, method_name
                        ));
                    }
                }
            }
        } else {
            self.errors.push(format!(
                "Llamada a método sobre tipo no-clase: {:?}",
                inst_type
            ));
        }

        HulkType::Unknown
    }
}