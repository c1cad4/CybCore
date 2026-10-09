use cybgrowth::{Contribution, GrowthLedger};
use cybidentity::{verify, Identity};
use cybmissions::{MissionBoard, Status};
use cybtrust::{ReviewEvent, TrustLedger};
pub fn approval_message(mission: &str, actor: &str, evidence: &str) -> Vec<u8> {
    let mut bytes = b"cybcore.approval.v1".to_vec();
    for value in [mission, actor, evidence] {
        bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    bytes
}

pub fn signed_mission_demo() -> Result<(i64, usize), String> {
    let reviewer = Identity::generate();
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
    if !verify(&reviewer.public_key(), &message, &signature) {
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
