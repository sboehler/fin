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

impl<V> Node<V> {
    pub fn post_order<F>(&self, mut f: F)
    where
        F: FnMut(&[&str]),
    {
        let mut path = Vec::new();
        self.post_order_rec(&mut path, &mut f);
    }

    fn post_order_rec<'a, 'b, F>(&'a self, v: &mut Vec<&'b str>, f: &mut F)
    where
        F: FnMut(&[&str]),
        'a: 'b,
    {
        self.children.iter().for_each(|(segment, child)| {
            v.push(segment);
            child.post_order_rec(v, f);
            v.pop();
        });
        f(v);
    }

    pub fn pre_order<F>(&self, mut f: F)
    where
        F: FnMut(&[&str]),
    {
        let mut path = Vec::new();
        self.pre_order_rec(&mut path, &mut f);
    }

    fn pre_order_rec<'a, 'b, F>(&'a self, v: &mut Vec<&'b str>, f: &mut F)
    where
        F: FnMut(&[&str]),
        'a: 'b,
    {
        f(v);
        self.children.iter().for_each(|(segment, child)| {
            v.push(segment);
            child.pre_order_rec(v, f);
            v.pop();
        });
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

impl<V: Monoid> Node<V> {}

pub trait Monoid {
    fn new() -> Self;
    fn combine(&self, other: &Self) -> Self;
}
