//! Correctness contract for `obfusku_fmt::format`: reformatting must not change what a program means. Two layers, deliberately: re-parsing alone does not prove the tree is right, so most fixtures are also run before and after formatting and compared. `obfusku-fmt` has no dependency on the checker or runtime; those are dev-only here.

mod support;

mod constructs;
mod fidelity;
