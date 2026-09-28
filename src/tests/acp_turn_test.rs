//! Stop-reason mapping tests for the ACP turn bridge (#1540).

use crate::acp::turn::{CallIdPairing, round_text_is_duplicate, stop_reason};
use crate::brain::provider::StopReason;

#[test]
fn stop_reasons_map_to_acp() {
    assert_eq!(stop_reason(Some(StopReason::EndTurn)), "end_turn");
    assert_eq!(stop_reason(Some(StopReason::MaxTokens)), "max_tokens");
    // A stop sequence ends the turn the same way an end-turn does;
    // "stop_sequence" is not an ACP stopReason value.
    assert_eq!(stop_reason(Some(StopReason::StopSequence)), "end_turn");
    assert_eq!(stop_reason(Some(StopReason::ToolUse)), "end_turn");
    assert_eq!(stop_reason(None), "end_turn");
}

#[test]
fn round_aggregate_repeating_streamed_text_is_a_duplicate() {
    // The wire-proven doubling: the loop streams the answer and then fires
    // the round aggregate with the same text (smoke phase B: two identical
    // `agent_message_chunk` frames, "ACP-OK" twice).
    assert!(round_text_is_duplicate("ACP-OK", "ACP-OK"));
}

#[test]
fn whitespace_disagreement_is_still_a_duplicate() {
    assert!(round_text_is_duplicate("ACP-OK", "ACP-OK\n"));
    assert!(round_text_is_duplicate(" ACP-OK ", "ACP-OK"));
}

#[test]
fn unstreamed_or_differing_text_is_not_a_duplicate() {
    // CLI providers stream nothing — the aggregate is the only delivery.
    assert!(!round_text_is_duplicate("", "ACP-OK"));
    // Round 2 after a tool call: fresh stream, different aggregate.
    assert!(!round_text_is_duplicate("round one", "round two"));
    // An empty aggregate would only add noise; never treat it as a dup.
    assert!(!round_text_is_duplicate("ACP-OK", ""));
    assert!(!round_text_is_duplicate("", "\n"));
}

#[test]
fn permission_ask_and_its_tool_call_share_one_id() {
    // The round 7 dogfood bug: the ask minted a fresh id, so the gated tool
    // rendered as two transcript rows. The ask's id must be the one the
    // tool call adopts, and the completion must close that same id.
    let pairing = CallIdPairing::new();
    let ask = pairing.mint_for_ask("bash");
    let start = pairing.consume_for_start("bash");
    assert_eq!(ask, start);
    assert_eq!(pairing.complete("bash"), ask);
}

#[test]
fn denied_ask_retracts_its_pending_id() {
    let pairing = CallIdPairing::new();
    let denied = pairing.mint_for_ask("bash");
    pairing.retract_for_deny("bash");
    let started = pairing.consume_for_start("bash");
    assert_ne!(denied, started);
}

#[test]
fn a_denied_ask_does_not_leak_into_the_next_same_name_tool() {
    let pairing = CallIdPairing::new();
    let _denied = pairing.mint_for_ask("bash");
    pairing.retract_for_deny("bash");
    let fresh_ask = pairing.mint_for_ask("bash");
    assert_eq!(pairing.consume_for_start("bash"), fresh_ask);
}

#[test]
fn ungated_tools_mint_and_complete_their_own_ids() {
    // Tools that never ask permission keep the original pairing behavior.
    let pairing = CallIdPairing::new();
    let started = pairing.consume_for_start("ls");
    assert_eq!(pairing.complete("ls"), started);
}

#[test]
fn parallel_same_name_tools_pair_in_start_order() {
    let pairing = CallIdPairing::new();
    let first = pairing.consume_for_start("bash");
    let second = pairing.consume_for_start("bash");
    assert_eq!(pairing.complete("bash"), first);
    assert_eq!(pairing.complete("bash"), second);
}
