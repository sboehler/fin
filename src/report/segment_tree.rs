use std::{collections::HashMap, iter};

pub struct Node<V> {
    pub children: HashMap<String, Node<V>>,
    pub value: V,
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
    pub fn iter_post(&self) -> impl Iterator<Item = (Vec<&str>, &V)> {
        self.iter_post_rec(Vec::new())
    }

    fn iter_post_rec<'a, 'b>(
        &'a self,
        parent: Vec<&'b str>,
    ) -> Box<dyn Iterator<Item = (Vec<&'b str>, &'b V)> + 'b>
    where
        'a: 'b,
    {
        let p = parent.clone();
        Box::new(
            self.children
                .iter()
                .flat_map(move |(segment, child)| {
                    let mut path = p.clone();
                    path.push(segment);
                    child.iter_post_rec(path)
                })
                .chain(iter::once((parent, &self.value))),
        )
    }

    pub fn iter_pre(&self) -> impl Iterator<Item = (Vec<&str>, &V)> {
        self.iter_pre_rec(Vec::new())
    }

    fn iter_pre_rec<'a, 'b>(
        &'a self,
        parent: Vec<&'b str>,
    ) -> Box<dyn Iterator<Item = (Vec<&'b str>, &'b V)> + 'b>
    where
        'a: 'b,
    {
        let p = parent.clone();
        Box::new(
            iter::once((parent, &self.value)).chain(self.children.iter().flat_map(
                move |(segment, child)| {
                    let mut path = p.clone();
                    path.push(segment);
                    child.iter_pre_rec(path)
                },
            )),
        )
    }
}

impl<V: Default> Node<V> {
    pub fn lookup_or_create_mut(&mut self, names: &[&str]) -> &mut V {
        &mut self.lookup_or_create_mut_node(names).value
    }

    pub fn lookup_or_create_mut_node(&mut self, names: &[&str]) -> &mut Node<V> {
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
