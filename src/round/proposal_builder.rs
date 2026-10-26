use super::message::NewRoundVtxoTreeProposalPayload;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step3ProposalBuildInput {
    pub round_label: String,
    pub participants: Vec<String>,
    pub onboard_outpoints: Vec<String>,
    pub closing_block: u64,
    pub recycled_block: u64,
    pub leaf_timeout: u64,
}

fn sorted(values: &[String]) -> Vec<String> {
    let mut out = values.to_vec();
    out.sort();
    out
}

pub fn build_step3_proposal_payload(
    input: &Step3ProposalBuildInput,
) -> NewRoundVtxoTreeProposalPayload {
    let participants = sorted(&input.participants);
    let onboard_outpoints = sorted(&input.onboard_outpoints);

    println!(
        "[step3-builder] begin round={} participants={} onboard_outpoints={} closing_block={} recycled_block={} leaf_timeout={}",
        input.round_label,
        participants.len(),
        onboard_outpoints.len(),
        input.closing_block,
        input.recycled_block,
        input.leaf_timeout
    );
    println!("[step3-builder] participants_ordered={:?}", participants);
    println!(
        "[step3-builder] onboard_outpoints_ordered={:?}",
        onboard_outpoints
    );

    let root_psbt = format!(
        "BASE64_ROOT_PSBT::round={}::participants={}",
        input.round_label,
        participants.join("+")
    );
    let vtxo_tree_psbts: Vec<String> = participants
        .iter()
        .enumerate()
        .map(|(idx, participant)| {
            format!(
                "BASE64_VTXO_TREE_PSBT::{idx}::participant={participant}::round={}",
                input.round_label
            )
        })
        .collect();
    let forfeit_tree_psbts: Vec<String> = participants
        .iter()
        .enumerate()
        .map(|(idx, participant)| {
            format!(
                "BASE64_FORFEIT_TREE_PSBT::{idx}::participant={participant}::round={}",
                input.round_label
            )
        })
        .collect();
    let recycle_psbt = format!(
        "BASE64_RECYCLE_PSBT::round={}::recycled_block={}",
        input.round_label, input.recycled_block
    );

    println!(
        "[step3-builder] artifacts root_psbt_len={} vtxo_tree_psbts={} forfeit_tree_psbts={} recycle_psbt_len={}",
        root_psbt.len(),
        vtxo_tree_psbts.len(),
        forfeit_tree_psbts.len(),
        recycle_psbt.len()
    );

    NewRoundVtxoTreeProposalPayload {
        root_psbt,
        vtxo_tree_psbts,
        forfeit_tree_psbts,
        recycle_psbt,
    }
}

#[cfg(test)]
mod tests {
    use super::{Step3ProposalBuildInput, build_step3_proposal_payload};
    use crate::round::message::parse_new_round_vtxo_tree_proposal_payload;

    fn sample_input() -> Step3ProposalBuildInput {
        Step3ProposalBuildInput {
            round_label: "r-2026-02-25-001".to_string(),
            participants: vec!["Bob".to_string(), "Alice".to_string(), "Eve".to_string()],
            onboard_outpoints: vec![
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb:1".to_string(),
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa:0".to_string(),
            ],
            closing_block: 910_000,
            recycled_block: 910_144,
            leaf_timeout: 10,
        }
    }

    #[test]
    fn step3_builder_is_deterministic() {
        let input = sample_input();
        let first = build_step3_proposal_payload(&input);
        let second = build_step3_proposal_payload(&input);
        assert_eq!(first, second, "same input must produce identical payload");
    }

    #[test]
    fn step3_builder_has_ordered_outputs_and_required_artifacts() {
        let payload = build_step3_proposal_payload(&sample_input());
        assert!(payload.root_psbt.contains("Alice+Bob+Eve"));
        assert_eq!(payload.vtxo_tree_psbts.len(), 3);
        assert_eq!(payload.forfeit_tree_psbts.len(), 3);
        assert!(
            payload
                .vtxo_tree_psbts
                .first()
                .expect("at least one vtxo tree psbt")
                .contains("participant=Alice")
        );
        assert!(payload.recycle_psbt.contains("recycled_block=910144"));
    }

    #[test]
    fn step3_builder_output_parses_with_message_parser() {
        let payload = build_step3_proposal_payload(&sample_input());
        let json = crate::round::message::new_round_vtxo_tree_proposal_payload_json(&payload);
        let parsed = parse_new_round_vtxo_tree_proposal_payload(&json)
            .expect("builder output should parse as proposal payload");
        assert_eq!(parsed, payload);
    }
}
