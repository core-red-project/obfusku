//! [`Text`]: the runtime representation of `Str` (`ADR-024`).

use std::cell::OnceCell;
use std::rc::Rc;

/// An immutable string stored as a rope.
///
/// **Why a rope.** Programs build strings by appending in a loop. With flat
/// strings, `✚` copies both sides, so `n` appends cost O(n²). Here `✚` is
/// O(1): it records the two halves. The flat `str` is built once, on the
/// first read, and cached, so appending `n` pieces and then reading costs
/// O(total) (`ADR-024`).
///
/// Cloning is a reference-count bump. Equality and ordering compare the
/// flattened text. The type derefs to `str`, so any `str` method works (and
/// triggers the flatten).
#[derive(Clone)]
pub struct Text(Rc<Node>);

/// A rope node: a leaf holds its flat text from construction; an interior
/// node holds its halves and caches the flat text on first read.
struct Node {
    len: usize,
    flat: OnceCell<Rc<str>>,
    children: Option<(Text, Text)>,
}

impl Text {
    /// The concatenation of `a` and `b`, in O(1). An empty operand is
    /// absorbed, so appending to an empty accumulator allocates nothing.
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

    /// Builds the flat text from the leaves.
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
    fn equality_compares_text_not_structure() {
        let split = Text::concat(&Text::from("ab"), &Text::from("c"));
        assert_eq!(split, Text::from("abc"));
        assert_ne!(split, Text::from("abd"));
        assert_eq!(split, "abc");
    }

    #[test]
    fn byte_len_counts_bytes_not_chars() {
        assert_eq!(Text::from("≔").byte_len(), 3);
    }

    #[test]
    fn display_and_debug_show_the_flat_text() {
        let t = Text::concat(&Text::from("a"), &Text::from("\"b"));
        assert_eq!(t.to_string(), "a\"b");
        assert_eq!(format!("{t:?}"), "\"a\\\"b\"");
    }

    #[test]
    fn empty_operand_is_the_identity() {
        let a = Text::from("a");
        assert_eq!(Text::concat(&a, &Text::from("")), a);
        assert_eq!(Text::concat(&Text::from(""), &a), a);
    }
}
