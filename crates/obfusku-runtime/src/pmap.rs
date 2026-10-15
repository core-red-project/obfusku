//! Persistent ordered map: an AVL tree with path copying (`ADR-025`).
//! Keys order by [`crate::ops::cmp_total`]; `get`/`insert`/`remove` are
//! O(log n) and share structure with the map they came from. A `Set` is a
//! `PMap` whose values are `Unit`.

use crate::ops::cmp_total;
use crate::value::Value;
use std::cmp::Ordering;
use std::rc::Rc;

#[derive(Debug)]
struct Node {
    key: Value,
    val: Value,
    left: Link,
    right: Link,
    height: u8,
    size: usize,
}

type Link = Option<Rc<Node>>;

fn height(l: &Link) -> u8 {
    l.as_ref().map_or(0, |n| n.height)
}

fn size(l: &Link) -> usize {
    l.as_ref().map_or(0, |n| n.size)
}

fn node(key: Value, val: Value, left: Link, right: Link) -> Rc<Node> {
    Rc::new(Node {
        height: 1 + height(&left).max(height(&right)),
        size: 1 + size(&left) + size(&right),
        key,
        val,
        left,
        right,
    })
}

fn balance(key: Value, val: Value, left: Link, right: Link) -> Rc<Node> {
    let (hl, hr) = (height(&left), height(&right));
    if hl > hr + 1 {
        let l = left.expect("left is taller");
        if height(&l.left) >= height(&l.right) {
            let new_right = node(key, val, l.right.clone(), right);
            node(
                l.key.clone(),
                l.val.clone(),
                l.left.clone(),
                Some(new_right),
            )
        } else {
            let lr = l.right.clone().expect("left-right is taller");
            let new_left = node(
                l.key.clone(),
                l.val.clone(),
                l.left.clone(),
                lr.left.clone(),
            );
            let new_right = node(key, val, lr.right.clone(), right);
            node(
                lr.key.clone(),
                lr.val.clone(),
                Some(new_left),
                Some(new_right),
            )
        }
    } else if hr > hl + 1 {
        let r = right.expect("right is taller");
        if height(&r.right) >= height(&r.left) {
            let new_left = node(key, val, left, r.left.clone());
            node(
                r.key.clone(),
                r.val.clone(),
                Some(new_left),
                r.right.clone(),
            )
        } else {
            let rl = r.left.clone().expect("right-left is taller");
            let new_left = node(key, val, left, rl.left.clone());
            let new_right = node(
                r.key.clone(),
                r.val.clone(),
                rl.right.clone(),
                r.right.clone(),
            );
            node(
                rl.key.clone(),
                rl.val.clone(),
                Some(new_left),
                Some(new_right),
            )
        }
    } else {
        node(key, val, left, right)
    }
}

fn insert(link: &Link, key: Value, val: Value) -> Rc<Node> {
    match link {
        None => node(key, val, None, None),
        Some(n) => match cmp_total(&key, &n.key) {
            Ordering::Less => balance(
                n.key.clone(),
                n.val.clone(),
                Some(insert(&n.left, key, val)),
                n.right.clone(),
            ),
            Ordering::Greater => balance(
                n.key.clone(),
                n.val.clone(),
                n.left.clone(),
                Some(insert(&n.right, key, val)),
            ),
            Ordering::Equal => node(key, val, n.left.clone(), n.right.clone()),
        },
    }
}

/// Removes the minimum, returning the remaining tree and that entry.
fn pop_min(n: &Rc<Node>) -> (Link, Value, Value) {
    match &n.left {
        None => (n.right.clone(), n.key.clone(), n.val.clone()),
        Some(l) => {
            let (rest, k, v) = pop_min(l);
            (
                Some(balance(n.key.clone(), n.val.clone(), rest, n.right.clone())),
                k,
                v,
            )
        }
    }
}

fn remove(link: &Link, key: &Value) -> Link {
    let n = link.as_ref()?;
    match cmp_total(key, &n.key) {
        Ordering::Less => Some(balance(
            n.key.clone(),
            n.val.clone(),
            remove(&n.left, key),
            n.right.clone(),
        )),
        Ordering::Greater => Some(balance(
            n.key.clone(),
            n.val.clone(),
            n.left.clone(),
            remove(&n.right, key),
        )),
        Ordering::Equal => match (&n.left, &n.right) {
            (None, r) => r.clone(),
            (l, None) => l.clone(),
            (l, Some(r)) => {
                let (rest, k, v) = pop_min(r);
                Some(balance(k, v, l.clone(), rest))
            }
        },
    }
}

#[derive(Debug, Clone, Default)]
pub struct PMap {
    root: Link,
}

impl PMap {
    #[must_use]
    pub fn new() -> PMap {
        PMap { root: None }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        size(&self.root)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    #[must_use]
    pub fn get(&self, key: &Value) -> Option<&Value> {
        let mut cur = self.root.as_ref();
        while let Some(n) = cur {
            match cmp_total(key, &n.key) {
                Ordering::Less => cur = n.left.as_ref(),
                Ordering::Greater => cur = n.right.as_ref(),
                Ordering::Equal => return Some(&n.val),
            }
        }
        None
    }

    #[must_use]
    pub fn insert(&self, key: Value, val: Value) -> PMap {
        PMap {
            root: Some(insert(&self.root, key, val)),
        }
    }

    #[must_use]
    pub fn remove(&self, key: &Value) -> PMap {
        PMap {
            root: remove(&self.root, key),
        }
    }

    /// Entries in ascending key order.
    #[must_use]
    pub fn entries(&self) -> Vec<(Value, Value)> {
        let mut out = Vec::with_capacity(self.len());
        // Explicit stack: no recursion depth concern, though the tree is
        // O(log n) deep anyway.
        let mut stack: Vec<&Rc<Node>> = Vec::new();
        let mut cur = self.root.as_ref();
        while cur.is_some() || !stack.is_empty() {
            while let Some(n) = cur {
                stack.push(n);
                cur = n.left.as_ref();
            }
            if let Some(n) = stack.pop() {
                out.push((n.key.clone(), n.val.clone()));
                cur = n.right.as_ref();
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(i: i64) -> Value {
        Value::Int(i)
    }

    #[test]
    fn insert_get_remove_keep_order_and_balance() {
        let mut m = PMap::new();
        for i in 0..2000 {
            m = m.insert(int((i * 7919) % 2003), int(i));
        }
        assert_eq!(m.len(), 2000);
        let keys: Vec<i64> = m
            .entries()
            .iter()
            .map(|(k, _)| match k {
                Value::Int(i) => *i,
                _ => unreachable!(),
            })
            .collect();
        assert!(keys.windows(2).all(|w| w[0] < w[1]));
        let k = (5 * 7919) % 2003;
        assert!(matches!(m.get(&int(k)), Some(Value::Int(5))));
        let m2 = m.remove(&int(k));
        assert_eq!(m2.len(), 1999);
        assert!(m2.get(&int(k)).is_none());
        assert!(m.get(&int(k)).is_some(), "old version unchanged");
    }

    #[test]
    fn replacing_a_key_does_not_grow_the_map() {
        let m = PMap::new().insert(int(1), int(1)).insert(int(1), int(2));
        assert_eq!(m.len(), 1);
        assert!(matches!(m.get(&int(1)), Some(Value::Int(2))));
    }
}
