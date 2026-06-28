//! Slice 4 end-to-end trace harness — NOT a runtime binary.
//!
//! Reads one presence `action_result` envelope from stdin, runs it through the same
//! `parse_to_body` the live desktop body uses, and prints the parsed `ActionGrade` as
//! JSON on stdout. The vitest trace (`src/__tests__/governanceVerticalSlice.trace.test.ts`)
//! drives `handleActionRequest` to produce a real envelope, pipes it here, and asserts
//! the body reconstructs the same `backed_by` ids the soul emitted and the ledger
//! persisted — closing the wire↔ledger↔body loop deterministically.
//!
//! Shares `presence.rs` with the main binary via `#[path]` so the parser exercised here
//! is byte-identical to the one the body runs. Output contract:
//!   {"ok":true,"grade":{"graded":N,"trusted":M,"backed_by":["id",...]}}  — parsed, grade present
//!   {"ok":true,"grade":null}                                            — parsed, no grade on the cue
//!   {"ok":false}                                                        — malformed envelope, dropped

// The harness only needs `parse_to_body` + the `Cue`/`ActionGrade`/`ToBody` types. Sharing
// `presence.rs` pulls in the WebSocket client and request builders too, which are dead in
// this binary — suppress the expected dead-code warnings rather than carve up the module.
#![allow(dead_code)]

#[path = "../presence.rs"]
mod presence;

use std::io::Read;

use presence::{parse_to_body, Cue};

fn main() {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        print!("{{\"ok\":false}}");
        return;
    }
    let body = match parse_to_body(&input) {
        Some(b) => b,
        None => {
            print!("{{\"ok\":false}}");
            return;
        }
    };
    match body.cue {
        Cue::ActionResult { grade: Some(g), .. } => {
            // Hand-rolled JSON to keep the harness dependency-free and the output auditable.
            // `backed_by` ids are receipt ids (ASCII, no control chars); the escape below
            // covers the only two characters that can appear in a JSON string literal.
            let ids = g
                .backed_by
                .iter()
                .map(|id| format!("\"{}\"", id.replace('\\', "\\\\").replace('"', "\\\"")))
                .collect::<Vec<_>>()
                .join(",");
            print!(
                "{{\"ok\":true,\"grade\":{{\"graded\":{},\"trusted\":{},\"backed_by\":[{}]}}}}",
                g.graded, g.trusted, ids
            );
        }
        Cue::ActionResult { grade: None, .. } => {
            print!("{{\"ok\":true,\"grade\":null}}");
        }
        // The harness is `action_result`-only. A parsed-but-wrong-kind cue means the wrong
        // envelope was piped in — fail rather than silently report "no grade", so the trace
        // can't mistake a routing error for a back-compat action_result.
        _ => print!("{{\"ok\":false}}"),
    }
}
