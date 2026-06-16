
use crate::nodes::expr_node::{Expr, HulkType};
 
/// Nodo para la construcción de una tupla: `(e1, e2, ...)`
///
/// Por qué nodo propio y no reutilizar BlockNode o FunCallNode:
///   - BlockNode semánticamente es "secuencia, retorna la última expresión".
///   - FunCallNode es una llamada con nombre.
///   - Una tupla es un producto de valores heterogéneos con índice numérico de acceso;
///     su identidad semántica es distinta y merece nodo propio para que el
///     type_inferrer y el codegen puedan tratarla de forma especializada.
#[derive(Debug)]
pub struct TupleNode {
    pub elements: Vec<Expr>,
    pub return_type: HulkType,
}
 
impl TupleNode {
    pub fn new(elements: Vec<Expr>) -> Self {
        TupleNode {
            elements,
            return_type: HulkType::Unknown,
        }
    }
    pub fn set_type(&mut self, t: HulkType) {
        self.return_type = t;
    }
}
 
/// Nodo para el acceso a un elemento de una tupla: `expr.N`
///
/// Por qué `index: usize` y no `member: LiteralNode`:
///   MemberAccessNode usa un identificador textual ("x.foo").
///   Para tuplas el índice es siempre un literal entero no negativo
///   conocido en tiempo de parseo ("p.0", "p.1").  Guardarlo como usize
///   simplifica el codegen (GEP directo) y la verificación semántica
///   (basta comparar index < len).
#[derive(Debug)]
pub struct TupleAccessNode {
    pub tuple: Box<Expr>,
    pub index: usize,
    pub return_type: HulkType,
}
 
impl TupleAccessNode {
    pub fn new(tuple: Expr, index: usize) -> Self {
        TupleAccessNode {
            tuple: Box::new(tuple),
            index,
            return_type: HulkType::Unknown,
        }
    }
    pub fn set_type(&mut self, t: HulkType) {
        self.return_type = t;
    }
}