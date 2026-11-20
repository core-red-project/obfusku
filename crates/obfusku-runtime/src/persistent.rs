//! The runtime's persistent (immutable, structure-sharing) data structures.
//!
//! **Why persistent.** Obfusku values are immutable, and evaluating a
//! variable *clones* its value. If a clone of a 100 000-element array copied
//! the elements, every read would be O(n). These structures make a clone a
//! reference-count bump and an "update" return a new version that shares
//! everything it did not touch, so the old version stays valid for free
//! (`ADR-025`, `ADR-024`).
//!
//! | Type | Structure | Costs |
//! |---|---|---|
//! | [`PVec`] | 32-way trie with a tail buffer | `get`, `set`, `push` O(log₃₂ n) |
//! | [`PMap`] | AVL tree with path copying | `get`, `insert`, `remove` O(log n) |
//! | [`Text`] | rope, flattened lazily | `✚` O(1); one O(total) flatten on first read |
//!
//! They are `Rc`-based and so single-threaded, like the evaluator.

mod map;
mod text;
mod vector;

pub use map::PMap;
pub use text::Text;
pub use vector::PVec;
