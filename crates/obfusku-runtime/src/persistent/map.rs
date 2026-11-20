//! [`PMap`]: the persistent ordered map behind `Value::Map` and
//! `Value::Set` (`ADR-025`).

use crate::ops::cmp_total;
use crate::value::Value;
use std::cmp::Ordering;
use std::rc::Rc;

/// One AVL node. `height` drives balancing and `size` makes `len` O(1).
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

/// Height of a subtree; an empty one has height 0.
fn height(l: &Link) -> u8 {
    l.as_ref().map_or(0, |n| n.height)
}

/// Entry count of a subtree.
fn size(l: &Link) -> usize {
    l.as_ref().map_or(0, |n| n.size)
}

/// Builds a node, computing its height and size from its children.
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

/// Builds a node and restores the AVL invariant (child heights differ by at
/// most one) with the single or double rotation the imbalance calls for.
/// Called by `insert` and `remove` on every node of the changed path.
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

/// Path-copying insert; an equal key replaces its entry.
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

/// Path-copying remove; a missing key leaves the tree as it was.
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

/// A persistent ordered map from [`Value`] to [`Value`].
///
/// An AVL tree, not a hash map, for two reasons that come from the language:
/// iteration order must be deterministic (a program's output may not depend
/// on a hasher seed), and keys are ordered by the generated total order
/// ([`cmp_total`]), which is defined for every orderable value but not
/// hashable in general (`NaN`, `-0.0`). Updates copy only the path to the
/// changed node, so older versions stay valid.
///
/// A `Set` is a `PMap` whose values are all `Unit`.
#[derive(Debug, Clone, Default)]
pub struct PMap {
    root: Link,
}

impl PMap {
    /// The empty map.
    #[must_use]
    pub fn new() -> PMap {
        PMap { root: None }
    }

    /// Number of entries; O(1).
    #[must_use]
    pub fn len(&self) -> usize {
        size(&self.root)
    }

    /// Whether the map has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    /// The value stored under `key`.
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

    /// A copy with `key` bound to `val`, replacing any earlier binding.
    #[must_use]
    pub fn insert(&self, key: Value, val: Value) -> PMap {
        PMap {
            root: Some(insert(&self.root, key, val)),
        }
    }

    /// A copy without `key`; the same map if it was absent.
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
    fn the_empty_map_has_no_entries() {
        let m = PMap::new();
        assert!(m.is_empty());
        assert_eq!(m.len(), 0);
        assert!(m.get(&int(1)).is_none());
        assert!(m.entries().is_empty());
    }

    #[test]
    fn removing_an_absent_key_changes_nothing() {
        let m = PMap::new().insert(int(1), int(1));
        assert_eq!(m.remove(&int(2)).len(), 1);
    }

    #[test]
    fn removing_every_key_empties_the_map_at_every_step() {
        let mut m = (0..200).fold(PMap::new(), |m, i| m.insert(int(i), int(i)));
        for i in 0..200 {
            m = m.remove(&int(i));
            assert_eq!(m.len(), (199 - i) as usize);
        }
        assert!(m.is_empty());
    }

    #[test]
    fn keys_follow_the_total_order_so_nan_is_a_usable_key() {
        let m = PMap::new()
            .insert(Value::Real(f64::NAN), int(1))
            .insert(Value::Real(1.0), int(2));
        assert!(matches!(m.get(&Value::Real(f64::NAN)), Some(Value::Int(1))));
        let first = m.entries().remove(0).0;
        assert!(matches!(first, Value::Real(r) if r == 1.0));
    }

    #[test]
    fn replacing_a_key_does_not_grow_the_map() {
        let m = PMap::new().insert(int(1), int(1)).insert(int(1), int(2));
        assert_eq!(m.len(), 1);
        assert!(matches!(m.get(&int(1)), Some(Value::Int(2))));
    }
}
