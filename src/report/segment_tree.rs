use std::collections::HashMap;

pub struct Node<V> {
    children: HashMap<String, Node<V>>,
    value: V,
}

impl<V: Default> Default for Node<V> {
    fn default() -> Self {
        Node {
            value: Default::default(),
            children: Default::default(),
        }
    }
}

impl<V: Default> Node<V> {
    pub fn lookup_or_create_mut(&mut self, names: &[&str]) -> &mut V {
        &mut self.lookup_or_create_mut_node(names).value
    }

    fn lookup_or_create_mut_node(&mut self, names: &[&str]) -> &mut Node<V> {
        match *names {
            [first, ref rest @ ..] => self
                .children
                .entry(first.into())
                .or_default()
                .lookup_or_create_mut_node(rest),
            [] => self,
        }
    }
}
