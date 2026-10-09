use cybgrowth::{Contribution, GrowthLedger};
use cybidentity::{verify, Identity};
use cybmissions::{MissionBoard, Status};
use cybregistry::{AgentRecord, Registry};
use std::collections::BTreeSet;
use cybtrust::{ReviewEvent, TrustLedger};
pub fn approval_message(mission: &str, actor: &str, evidence: &str) -> Vec<u8> {
    let mut bytes = b"cybcore.approval.v1".to_vec();
    for value in [mission, actor, evidence] {
        bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    bytes
}

/// Verify the signature against a registered reviewer with approval capability.
/// The registry itself is assumed trusted and must be provisioned securely.
pub fn authorized_review(
    registry: &Registry,
    reviewer_id: &str,
    message: &[u8],
    signature: &[u8; 64],
) -> bool {
    registry.get(reviewer_id).is_some_and(|agent| {
        agent.capabilities.contains("approve_mission")
            && verify(&agent.public_key, message, signature)
    })
}

pub fn signed_mission_demo() -> Result<(i64, usize), String> {
    let reviewer = Identity::generate();
    let mut registry = Registry::default();
    registry
        .register(AgentRecord {
            id: "reviewer".into(),
            public_key: reviewer.public_key(),
            capabilities: BTreeSet::from(["approve_mission".into()]),
        })
        .map_err(|e| format!("{e:?}"))?;
    let mut board = MissionBoard::default();
    board
        .create("m1", "Document an observation")
        .map_err(|e| format!("{e:?}"))?;
    board.claim("m1", "worker").map_err(|e| format!("{e:?}"))?;
    board
        .submit("m1", "worker", "proof1")
        .map_err(|e| format!("{e:?}"))?;
    let message = approval_message("m1", "worker", "proof1");
    let signature = reviewer.sign(&message);
    if !authorized_review(&registry, "reviewer", &message, &signature) {
        return Err("signature rejected".into());
    }
    board.approve("m1").map_err(|e| format!("{e:?}"))?;
    if !matches!(
        board.get("m1").map(|m| &m.status),
        Some(Status::Completed { .. })
    ) {
        return Err("mission incomplete".into());
    }
    let mut trust = TrustLedger::default();
    trust
        .record(ReviewEvent {
        event_id: "r1".into(),
        subject: "worker".into(),
        reviewer: "reviewer".into(),
        evidence_id: "proof1".into(),
        accepted: true,
        })
        .map_err(|e| format!("{e:?}"))?;
    let mut growth = GrowthLedger::default();
    growth
        .submit(Contribution {
        id: "c1".into(),
        contributor: "worker".into(),
        mission_id: "m1".into(),
        evidence_id: "proof1".into(),
        approved: true,
        })
        .map_err(|e| format!("{e:?}"))?;
    Ok((trust.score("worker"), growth.completed_missions("worker")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_mission_updates_ledgers() {
        assert_eq!(signed_mission_demo().unwrap(), (1, 1));
    }

    #[test]
    fn unknown_reviewer_is_rejected() {
        let registry = Registry::default();
        let signer = Identity::generate();
        let message = approval_message("m1", "worker", "proof1");
        assert!(!authorized_review(
            &registry,
            "unknown",
            &message,
            &signer.sign(&message)
        ));
    }

    #[test]
    fn tampered_evidence_fails() {
        let reviewer = Identity::generate();
        let signature = reviewer.sign(&approval_message("m1", "worker", "proof1"));
        assert!(!verify(
            &reviewer.public_key(),
            &approval_message("m1", "worker", "proof2"),
            &signature
        ));
    }
}
