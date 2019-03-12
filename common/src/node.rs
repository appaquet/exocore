use std::collections::HashMap;

pub type NodeID = String;

#[derive(PartialEq, Eq, Clone, Debug)]
pub struct Node {
    // TODO: PublicKey
    // TODO: NodeID = hash(publickey)
    // TODO: ACLs
    pub id: NodeID,
    //    address: String,
    //    is_me: bool,
}

impl Node {
    pub fn new(id: String) -> Node {
        Node { id }
    }

    #[inline]
    pub fn id(&self) -> &NodeID {
        &self.id
    }
}

pub struct Nodes {
    nodes: HashMap<NodeID, Node>,
}

impl Nodes {
    pub fn new() -> Nodes {
        Nodes {
            nodes: HashMap::new(),
        }
    }

    pub fn add(&mut self, node: Node) {
        self.nodes.insert(node.id.clone(), node);
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    #[inline]
    pub fn get(&self, node_id: &str) -> Option<&Node> {
        self.nodes.get(node_id)
    }
}

impl Default for Nodes {
    fn default() -> Self {
        Nodes::new()
    }
}
