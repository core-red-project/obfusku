//! [`PVec`]: the persistent vector behind `Value::Array` (`ADR-025`).

use crate::value::Value;
use std::rc::Rc;

/// Index bits consumed per trie level.
const BITS: usize = 5;
/// Children per node: `2^BITS`.
const WIDTH: usize = 1 << BITS;
/// Selects one level's index out of a position.
const MASK: usize = WIDTH - 1;

/// A trie node. Interior nodes hold up to 32 children; leaves hold up to 32
/// elements. Both are immutable once built and shared through `Rc`.
#[derive(Debug)]
enum Node {
    Branch(Vec<Rc<Node>>),
    Leaf(Vec<Value>),
}

/// A persistent vector of [`Value`]s.
///
/// A 32-way trie plus a *tail*: the last, partially filled leaf lives outside
/// the trie, so the common operation (append) touches one small buffer and
/// only every 32nd push promotes it into the trie. 32 is wide enough that the
/// tree is a handful of levels for any realistic length (log₃₂ of a billion
/// is 6) and still fits a cache line's worth of pointers per node.
///
/// Every update returns a new vector sharing structure with the old one; the
/// old one is unaffected. `len` is O(1).
#[derive(Debug, Clone)]
pub struct PVec {
    len: usize,
    shift: usize,
    root: Rc<Node>,
    tail: Rc<Vec<Value>>,
}

impl Default for PVec {
    fn default() -> Self {
        PVec::new()
    }
}

/// A chain of single-child branches from `level` down to `leaf`: the path a
/// new subtree hangs from when the trie gains a level.
fn new_path(level: usize, leaf: Rc<Node>) -> Rc<Node> {
    if level == 0 {
        leaf
    } else {
        Rc::new(Node::Branch(vec![new_path(level - BITS, leaf)]))
    }
}

impl PVec {
    /// The empty vector.
    #[must_use]
    pub fn new() -> PVec {
        PVec {
            len: 0,
            shift: BITS,
            root: Rc::new(Node::Branch(Vec::new())),
            tail: Rc::new(Vec::new()),
        }
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the vector has no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Index of the first element stored in the tail rather than the trie.
    fn tail_offset(&self) -> usize {
        if self.len < WIDTH {
            0
        } else {
            ((self.len - 1) >> BITS) << BITS
        }
    }

    /// The element at `i`, or `None` past the end.
    #[must_use]
    pub fn get(&self, i: usize) -> Option<&Value> {
        if i >= self.len {
            return None;
        }
        if i >= self.tail_offset() {
            return self.tail.get(i & MASK);
        }
        let mut node = &self.root;
        let mut level = self.shift;
        while level > 0 {
            match &**node {
                Node::Branch(children) => node = children.get((i >> level) & MASK)?,
                Node::Leaf(_) => return None,
            }
            level -= BITS;
        }
        match &**node {
            Node::Leaf(items) => items.get(i & MASK),
            Node::Branch(_) => None,
        }
    }

    /// Appends `v`.
    ///
    /// Takes `self` by value so a uniquely owned tail is extended in place
    /// (`Rc::make_mut` copies it only when another vector shares it): a loop
    /// that pushes onto its own accumulator never copies.
    #[must_use]
    pub fn push(mut self, v: Value) -> PVec {
        if self.tail.len() < WIDTH {
            Rc::make_mut(&mut self.tail).push(v);
            self.len += 1;
            return self;
        }
        let leaf = Rc::new(Node::Leaf((*self.tail).clone()));
        let (root, shift) = if (self.len >> BITS) > (1 << self.shift) {
            let grown = Rc::new(Node::Branch(vec![
                Rc::clone(&self.root),
                new_path(self.shift, leaf),
            ]));
            (grown, self.shift + BITS)
        } else {
            (
                Rc::new(self.push_tail(self.shift, &self.root, leaf)),
                self.shift,
            )
        };
        PVec {
            len: self.len + 1,
            shift,
            root,
            tail: Rc::new(vec![v]),
        }
    }

    /// Promotes the full tail `leaf` into the trie below `parent`.
    fn push_tail(&self, level: usize, parent: &Rc<Node>, leaf: Rc<Node>) -> Node {
        let Node::Branch(children) = &**parent else {
            unreachable!("push_tail walks branches only");
        };
        let sub = ((self.len - 1) >> level) & MASK;
        let mut out = children.clone();
        let inserted = if level == BITS {
            leaf
        } else if let Some(child) = children.get(sub) {
            Rc::new(self.push_tail(level - BITS, child, leaf))
        } else {
            new_path(level - BITS, leaf)
        };
        if sub < out.len() {
            out[sub] = inserted;
        } else {
            out.push(inserted);
        }
        Node::Branch(out)
    }

    /// A copy with element `i` replaced by `v`, or `None` when `i` is out of
    /// range. Copies only the path from the root to that element.
    #[must_use]
    pub fn set(&self, i: usize, v: Value) -> Option<PVec> {
        if i >= self.len {
            return None;
        }
        if i >= self.tail_offset() {
            let mut tail = (*self.tail).clone();
            tail[i & MASK] = v;
            return Some(PVec {
                len: self.len,
                shift: self.shift,
                root: Rc::clone(&self.root),
                tail: Rc::new(tail),
            });
        }
        Some(PVec {
            len: self.len,
            shift: self.shift,
            root: do_set(self.shift, &self.root, i, v),
            tail: Rc::clone(&self.tail),
        })
    }

    /// The elements in order, cloned (cheap: values are `Rc`-shared).
    #[must_use]
    pub fn to_vec(&self) -> Vec<Value> {
        let mut out = Vec::with_capacity(self.len);
        collect(&self.root, &mut out);
        out.extend(self.tail.iter().cloned());
        out
    }

    /// Builds the trie bottom-up in O(n): full 32-element leaves, grouped
    /// into branches level by level, the last (possibly short) chunk as
    /// the tail.
    #[must_use]
    pub fn from_vec(mut items: Vec<Value>) -> PVec {
        let len = items.len();
        if len <= WIDTH {
            return PVec {
                len,
                shift: BITS,
                root: Rc::new(Node::Branch(Vec::new())),
                tail: Rc::new(items),
            };
        }
        let tail_offset = ((len - 1) >> BITS) << BITS;
        let tail = items.split_off(tail_offset);
        let mut nodes: Vec<Rc<Node>> = items
            .chunks(WIDTH)
            .map(|c| Rc::new(Node::Leaf(c.to_vec())))
            .collect();
        let mut shift = BITS;
        while nodes.len() > WIDTH {
            nodes = nodes
                .chunks(WIDTH)
                .map(|c| Rc::new(Node::Branch(c.to_vec())))
                .collect();
            shift += BITS;
        }
        PVec {
            len,
            shift,
            root: Rc::new(Node::Branch(nodes)),
            tail: Rc::new(tail),
        }
    }

    /// `self` followed by `other`: O(m log n) in the length of `other`.
    #[must_use]
    pub fn concat(&self, other: &PVec) -> PVec {
        other.to_vec().into_iter().fold(self.clone(), PVec::push)
    }

    /// Elements `start..start+len`, clamped to the vector: O(len · log n).
    #[must_use]
    pub fn slice(&self, start: usize, len: usize) -> PVec {
        let end = start.saturating_add(len).min(self.len);
        let start = start.min(end);
        PVec::from_vec((start..end).filter_map(|i| self.get(i).cloned()).collect())
    }
}

/// Path-copying update of element `i` below `node`.
fn do_set(level: usize, node: &Rc<Node>, i: usize, v: Value) -> Rc<Node> {
    match &**node {
        Node::Branch(children) => {
            let sub = (i >> level) & MASK;
            let mut out = children.clone();
            out[sub] = do_set(level - BITS, &children[sub], i, v);
            Rc::new(Node::Branch(out))
        }
        Node::Leaf(items) => {
            let mut out = items.clone();
            out[i & MASK] = v;
            Rc::new(Node::Leaf(out))
        }
    }
}

/// Appends every element below `node` to `out`, left to right.
fn collect(node: &Node, out: &mut Vec<Value>) {
    match node {
        Node::Branch(children) => children.iter().for_each(|c| collect(c, out)),
        Node::Leaf(items) => out.extend(items.iter().cloned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(v: &Value) -> i64 {
        match v {
            Value::Int(i) => *i,
            other => panic!("expected Int, got {other:?}"),
        }
    }

    #[test]
    fn push_get_set_across_trie_levels() {
        let n = 40_000;
        let mut v = PVec::new();
        for i in 0..n {
            v = v.push(Value::Int(i as i64));
        }
        assert_eq!(v.len(), n);
        for i in (0..n).step_by(997).chain([0, 31, 32, 1023, 1024, n - 1]) {
            assert_eq!(int(v.get(i).unwrap()), i as i64);
        }
        let w = v.set(33, Value::Int(-1)).unwrap();
        let u = v.set(n - 1, Value::Int(-2)).unwrap();
        assert_eq!(int(w.get(33).unwrap()), -1);
        assert_eq!(int(v.get(33).unwrap()), 33);
        assert_eq!(int(u.get(n - 1).unwrap()), -2);
        assert!(v.get(n).is_none());
        assert!(v.set(n, Value::Unit).is_none());
    }

    #[test]
    fn to_vec_preserves_order_and_slice_clamps() {
        let v = PVec::from_vec((0..100).map(Value::Int).collect());
        let all: Vec<i64> = v.to_vec().iter().map(int).collect();
        assert_eq!(all, (0..100).collect::<Vec<_>>());
        let s = v.slice(98, 10);
        assert_eq!(s.len(), 2);
        assert_eq!(int(s.get(0).unwrap()), 98);
        assert_eq!(v.slice(500, 3).len(), 0);
    }

    #[test]
    fn bulk_build_matches_incremental_push_at_every_boundary() {
        for n in [
            0usize, 1, 31, 32, 33, 64, 65, 1023, 1024, 1025, 1056, 1057, 32768, 32800, 40_000,
        ] {
            let bulk = PVec::from_vec((0..n).map(|i| Value::Int(i as i64)).collect());
            let mut inc = PVec::new();
            for i in 0..n {
                inc = inc.push(Value::Int(i as i64));
            }
            assert_eq!(bulk.len(), inc.len(), "n={n}");
            for i in (0..n).step_by(37).chain(n.checked_sub(1)) {
                assert_eq!(
                    int(bulk.get(i).unwrap()),
                    int(inc.get(i).unwrap()),
                    "n={n} i={i}"
                );
            }
            // The bulk-built vector must keep growing correctly.
            let grown = bulk.push(Value::Int(-1));
            assert_eq!(int(grown.get(n).unwrap()), -1, "n={n}");
            assert_eq!(grown.len(), n + 1);
        }
    }

    #[test]
    fn the_empty_vector_has_no_elements() {
        let v = PVec::new();
        assert!(v.is_empty());
        assert_eq!(v.len(), 0);
        assert!(v.get(0).is_none());
        assert!(v.to_vec().is_empty());
    }

    #[test]
    fn concat_appends_in_order_and_keeps_both_operands() {
        let a = PVec::from_vec((0..40).map(Value::Int).collect());
        let b = PVec::from_vec((100..110).map(Value::Int).collect());
        let both = a.concat(&b);
        let got: Vec<i64> = both.to_vec().iter().map(int).collect();
        let want: Vec<i64> = (0..40).chain(100..110).collect();
        assert_eq!(got, want);
        assert_eq!((a.len(), b.len()), (40, 10));
    }

    #[test]
    fn set_in_the_trie_leaves_the_original_untouched() {
        let v = PVec::from_vec((0..100).map(Value::Int).collect());
        let w = v.set(5, Value::Int(-5)).unwrap();
        assert_eq!(int(v.get(5).unwrap()), 5);
        assert_eq!(int(w.get(5).unwrap()), -5);
    }

    #[test]
    fn pushing_onto_a_shared_vector_does_not_disturb_the_original() {
        let a = PVec::from_vec((0..5).map(Value::Int).collect());
        let b = a.clone().push(Value::Int(9));
        assert_eq!(a.len(), 5);
        assert_eq!(b.len(), 6);
        assert_eq!(int(b.get(5).unwrap()), 9);
    }

    #[test]
    fn old_versions_are_unaffected_by_pushes() {
        let a = PVec::from_vec((0..33).map(Value::Int).collect());
        let b = a.clone().push(Value::Int(99));
        assert_eq!(a.len(), 33);
        assert_eq!(b.len(), 34);
        assert_eq!(int(b.get(33).unwrap()), 99);
    }
}
