//! `Str` runtime representation: an immutable rope. `✚` on two strings is
//! O(1); the flat `str` is built once, on first read, and cached. A loop
//! that appends `n` pieces then reads the result costs O(total), not
//! O(n²) (`ADR-024`).

use std::cell::OnceCell;
use std::rc::Rc;

#[derive(Clone)]
pub struct Text(Rc<Node>);

struct Node {
    len: usize,
    flat: OnceCell<Rc<str>>,
    children: Option<(Text, Text)>,
}

impl Text {
    #[must_use]
    pub fn concat(a: &Text, b: &Text) -> Text {
        if a.0.len == 0 {
            return b.clone();
        }
        if b.0.len == 0 {
            return a.clone();
        }
        Text(Rc::new(Node {
            len: a.0.len + b.0.len,
            flat: OnceCell::new(),
            children: Some((a.clone(), b.clone())),
        }))
    }

    /// Byte length, without flattening.
    #[must_use]
    pub fn byte_len(&self) -> usize {
        self.0.len
    }

    fn flatten(&self) -> Rc<str> {
        let mut out = String::with_capacity(self.0.len);
        // Right child pushed first so the left is visited first; iterative,
        // as a left-leaning rope of an append loop is as deep as it is long.
        let mut stack: Vec<&Text> = vec![self];
        while let Some(t) = stack.pop() {
            match (t.0.flat.get(), &t.0.children) {
                (Some(flat), _) => out.push_str(flat),
                (None, Some((l, r))) => {
                    stack.push(r);
                    stack.push(l);
                }
                (None, None) => {}
            }
        }
        Rc::from(out)
    }
}

impl std::ops::Deref for Text {
    type Target = str;
    fn deref(&self) -> &str {
        self.0.flat.get_or_init(|| self.flatten())
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Text {
        let flat = OnceCell::new();
        let _ = flat.set(Rc::from(s));
        Text(Rc::new(Node {
            len: s.len(),
            flat,
            children: None,
        }))
    }
}

impl From<String> for Text {
    fn from(s: String) -> Text {
        Text::from(s.as_str())
    }
}

impl From<&String> for Text {
    fn from(s: &String) -> Text {
        Text::from(s.as_str())
    }
}

impl PartialEq for Text {
    fn eq(&self, other: &Text) -> bool {
        self.0.len == other.0.len && **self == **other
    }
}

impl PartialEq<str> for Text {
    fn eq(&self, other: &str) -> bool {
        **self == *other
    }
}

impl PartialEq<&str> for Text {
    fn eq(&self, other: &&str) -> bool {
        **self == **other
    }
}

impl std::fmt::Display for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self)
    }
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, f)
    }
}

impl Drop for Node {
    // A rope built by an append loop is a deep left-leaning chain; the
    // default recursive drop would overflow the stack on it.
    fn drop(&mut self) {
        let mut pending: Vec<Text> = Vec::new();
        if let Some((l, r)) = self.children.take() {
            pending.push(l);
            pending.push(r);
        }
        while let Some(t) = pending.pop() {
            if let Ok(mut node) = Rc::try_unwrap(t.0)
                && let Some((l, r)) = node.children.take()
            {
                pending.push(l);
                pending.push(r);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concat_reads_back_in_order() {
        let t = Text::concat(
            &Text::from("ab"),
            &Text::concat(&Text::from("c"), &Text::from("d")),
        );
        assert_eq!(&*t, "abcd");
        assert_eq!(t.byte_len(), 4);
    }

    #[test]
    fn long_append_chain_flattens_and_drops_without_overflowing() {
        let mut acc = Text::from("");
        let piece = Text::from("x");
        for _ in 0..500_000 {
            acc = Text::concat(&acc, &piece);
        }
        assert_eq!(acc.len(), 500_000);
        // Dropped on a default 2 MiB test-thread stack.
    }

    #[test]
    fn empty_operand_is_the_identity() {
        let a = Text::from("a");
        assert_eq!(Text::concat(&a, &Text::from("")), a);
        assert_eq!(Text::concat(&Text::from(""), &a), a);
    }
}
