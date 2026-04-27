use std::collections::HashMap;
use crate::{expr_visitor::ExprVisitor, nodes::{binaryop_node::BinaryOp, block_node::BlockNode, destassing_node::DestAssignNode, for_node::ForNode, funcall_node::FunCallNode, if_node::IfNode, let_node::LetNode, literal_node::Literal, program_node::{Program, Statement}, typedexpr_node::{HulkType, TypedExpr}, unaryop_node::UnaryOp, while_node::WhileNode}};

// Entorno para guardar variables locales y firmas de funciones
pub struct Environment {
    pub variables: HashMap<String, HulkType>,
    pub functions: HashMap<String, (Vec<HulkType>, HulkType)>, // (Argumentos, Retorno)
}

impl Environment {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            functions: HashMap::new(),
        }
    }
}

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

    // Punto de entrada principal
    pub fn check_program(&mut self, program: &mut Program) {
        self.pass_1_collect_functions(program);
        self.pass_2_check_bodies(program);
    }

    // PASADA 1: Registrar firmas de funciones
    fn pass_1_collect_functions(&mut self, program: &Program) {
        for stmt in &program.statements {
            if let Statement::FunctionDecl(decl) = stmt {
                if let Literal::Id(ref name) = decl.name.value {
                    let mut param_types = Vec::new();
                    for (_, p_type) in &decl.params {
                        param_types.push(p_type.clone());
                    }
                    // Por ahora asumimos Unknown como retorno hasta inferir
                    self.env.functions.insert(name.clone(), (param_types, HulkType::Unknown));
                }
            }
        }
    }

    // PASADA 2: Chequear tipos iterando sobre las estructuras
    fn pass_2_check_bodies(&mut self, program: &mut Program) {
        for stmt in &mut program.statements {
            match stmt {
                Statement::FunctionDecl(decl) => {
                    // Guardar variables antiguas (clonamos para scope simple, podría usarse una pila de scopes)
                    let old_vars = self.env.variables.clone();
                    
                    for (p_name, p_type) in &decl.params {
                        if let Literal::Id(ref id) = p_name.value {
                            self.env.variables.insert(id.clone(), p_type.clone());
                        }
                    }

                    // Chequear el cuerpo
                    let inferred_type = decl.body.accept(self);
                    
                    // Actualizar el tipo de retorno inferido en la función
                    if let Literal::Id(ref name) = decl.name.value {
                        if let Some(func) = self.env.functions.get_mut(name) {
                            func.1 = inferred_type;
                        }
                    }

                    // Restaurar scope
                    self.env.variables = old_vars;
                }
                Statement::Expression(expr) => {
                    expr.return_type = expr.accept(self);
                }
            }
        }
    }
}

// Implementación del Visitor que devuelve el tipo resultante y reporta errores
impl ExprVisitor<HulkType> for SemanticChecker {
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
        match self.env.variables.get(id) {
            Some(t) => t.clone(),
            None => {
                self.errors.push(format!("Variable no declarada: {}", id));
                HulkType::Unknown
            }
        }
    }

    fn visit_binary_op(&mut self, left: &TypedExpr, op: &BinaryOp, right: &TypedExpr) -> HulkType {
        let left_type = left.accept(self);
        let right_type = right.accept(self);

        // Aquí expandes la lógica para validar según la operación (e.j + es Número o String)
        match op {
            BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div => {
                if left_type == HulkType::Number && right_type == HulkType::Number {
                    HulkType::Number
                } else {
                    self.errors.push("Operación aritmética requiere números".to_string());
                    HulkType::Unknown
                }
            }
            BinaryOp::Equal => HulkType::Bool,
            _ => HulkType::Unknown,
        }
    }

    fn visit_unary_op(&mut self, _op: &UnaryOp, expr: &TypedExpr) -> HulkType {
        expr.accept(self) // Simplificado: aquí validarías Not para booleanos, Neg para números
    }

    fn visit_let(&mut self, node: &LetNode) -> HulkType {
        let mut old_vars = Vec::new();

        for ((id_node, decl_type), value_expr) in &node.assignments {
            let mut val_type = value_expr.accept(self);
            
            // Si hay un tipo declarado, validamos
            if *decl_type != HulkType::Unknown && val_type != *decl_type {
                self.errors.push("Mismatch de tipos en let".to_string());
            } else if *decl_type != HulkType::Unknown {
                val_type = decl_type.clone();
            }

            if let Literal::Id(ref name) = id_node.value {
                let previous = self.env.variables.insert(name.clone(), val_type);
                old_vars.push((name.clone(), previous));
            }
        }

        let ret = node.body.accept(self);

        // Restaurar variables (drop out of scope)
        for (name, previous) in old_vars {
            if let Some(prev) = previous {
                self.env.variables.insert(name, prev);
            } else {
                self.env.variables.remove(&name);
            }
        }

        ret
    }

    fn visit_if(&mut self, node: &IfNode) -> HulkType {
        let cond_t = node.condition.accept(self);
        if cond_t != HulkType::Bool {
            self.errors.push("La condición del if debe ser booleana".to_string());
        }

        let if_t = node.if_branch.accept(self);
        let else_t = node.else_branch.accept(self);

        for (elif_cond, elif_branch) in &node.elif_branches {
            elif_cond.accept(self); // Check booleano
            elif_branch.accept(self); // Check branch
        }

        // Simplificado: aquí se buscaría el tipo común más específico 
        if if_t == else_t { if_t } else { HulkType::Unknown }
    }

    fn visit_while(&mut self, node: &WhileNode) -> HulkType {
        let cond_t = node.condition.accept(self);
        if cond_t != HulkType::Bool {
            self.errors.push("La condición del while debe ser booleana".to_string());
        }
        node.body.accept(self);
        // Si no devuelve valor estándar, puedes usar un tipo Unit (o devolver de la última exp)
        HulkType::Unknown 
    }

    fn visit_for(&mut self, node: &ForNode) -> HulkType {
        node.iterator.accept(self);
        // Introducir la variable iteradora (validar iter)
        node.body.accept(self)
    }

    fn visit_fun_call(&mut self, node: &FunCallNode) -> HulkType {
        if let Literal::Id(ref name) = node.name.value {
            if let Some((_expected_args, ret_type)) = self.env.functions.get(name).cloned() {
                // Iterar sobre los argumentos para validar
                for arg in &node.args {
                    arg.accept(self);
                }
                return ret_type;
            } else {
                self.errors.push(format!("Función no definida: {}", name));
            }
        }
        HulkType::Unknown
    }

    fn visit_dest_assign(&mut self, node: &DestAssignNode) -> HulkType {
        let expr_t = node.expr.accept(self);
        expr_t
    }

    fn visit_block(&mut self, node: &BlockNode) -> HulkType {
        let mut last_type = HulkType::Unknown;
        for expr in &node.expressions {
            last_type = expr.accept(self);
        }
        last_type
    }
}