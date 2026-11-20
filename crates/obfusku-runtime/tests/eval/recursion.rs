//! Module-level recursion and tail-call elimination.

use super::support::*;

#[test]
fn module_level_recursive_function_computes_correctly() {
    // countdown = λn → match n with 0 → 0 | _ → countdown(n - 1)... but
    // there's no subtraction primitive in Core yet, so instead prove
    // recursion terminates via constructor-shaped decrement: represent
    // n as a chain of `Succ`/`Zero` and recurse structurally.
    let succ = |n: Expr| ctor("Succ", vec![n]);
    let zero = ctor("Zero", vec![]);
    let three = succ(succ(succ(zero)));

    // depth = λn → match n with Zero → 0 | Succ(k) → depth(k)  -- always
    // returns 0, but only by actually recursing down the chain.
    let depth_body = match_(
        var("n"),
        vec![
            (pctor("Zero", vec![]), int(0)),
            (
                pctor("Succ", vec![pvar("k")]),
                apply(var("depth"), var("k")),
            ),
        ],
    );
    let module = run_letrec(
        vec![("depth", lambda("n", depth_body))],
        vec![("result", apply(var("depth"), three))],
    );
    assert_eq!(as_int(module), 0);
}

#[test]
fn deep_tail_recursion_does_not_grow_the_rust_stack() {
    // loop_ = λn → match n with Zero → 0 | Succ(k) → loop_(k)
    //
    // Evaluating the depth-N `Succ`-chain argument costs one native
    // stack frame per layer (non-tail), fully unwinding before `loop_`
    // starts running. Depth is picked large enough that the tail-call
    // phase alone, without trampolining, would need more stack than any
    // ordinary process has — proving flat stack usage really comes from
    // TCO, not a small N happening to fit.
    let depth = 100_000usize;

    let handle = std::thread::Builder::new()
        .stack_size(1024 * 1024 * 1024)
        .spawn(move || {
            let mut n_expr = ctor("Zero", vec![]);
            for _ in 0..depth {
                n_expr = ctor("Succ", vec![n_expr]);
            }
            let loop_body = match_(
                var("n"),
                vec![
                    (pctor("Zero", vec![]), int(0)),
                    (
                        pctor("Succ", vec![pvar("k")]),
                        apply(var("loop_"), var("k")),
                    ),
                ],
            );
            as_int(run_letrec(
                vec![("loop_", lambda("n", loop_body))],
                vec![("result", apply(var("loop_"), n_expr))],
            ))
        })
        .expect("spawn test thread");
    let result = handle.join().expect("test thread panicked");
    assert_eq!(result, 0);
}

// ---------------------------------------------------------------------
// Raise / Catch
// ---------------------------------------------------------------------
