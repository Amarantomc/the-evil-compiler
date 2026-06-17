use crate::nodes::funcall_node::FunCallNode;

// Reutilizamos FunCallNode para la instanciación ya que es sintácticamente igual a una llamada
pub type InstantiationNode = FunCallNode;
