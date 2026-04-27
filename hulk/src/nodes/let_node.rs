
#[derive(Debug)]
pub struct LetNode {
    pub assignments: Vec<((LiteralNode,HulkType), TypedExpr)>,
    pub body: Box<TypedExpr>,
}

impl LetNode {
    pub fn new(assignments: Vec<((LiteralNode,HulkType), TypedExpr)>, body: TypedExpr) -> Self {
        LetNode {
            assignments,
            body: Box::new(body),
        }
    }
}