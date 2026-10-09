use cybgrowth::{Contribution, GrowthLedger};
use cybidentity::{verify, Identity};
use cybmemory::Journal;
use cybmissions::{MissionBoard, Status};
use cybregistry::{AgentRecord, Registry};
use cybtrust::{ReviewEvent, TrustLedger};
use std::collections::BTreeSet;
use std::path::Path;
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

/// Signed, domain-separated review record with an authenticated reviewer.
/// Replay protection still requires a unique event id in the journal.
pub fn signed_review_message(
    event_id: &str,
    subject: &str,
    evidence_id: &str,
    accepted: bool,
) -> Vec<u8> {
    let mut bytes = b"cybcore.review.v1".to_vec();
    for value in [event_id, subject, evidence_id] {
        bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
        bytes.extend_from_slice(value.as_bytes());
    }
    bytes.push(u8::from(accepted));
    bytes
}

pub fn append_authorized_review(
    journal: &mut Journal,
    registry: &Registry,
    reviewer_id: &str,
    event_id: &str,
    subject: &str,
    evidence_id: &str,
    accepted: bool,
    signature: &[u8; 64],
) -> Result<(), String> {
    let message = signed_review_message(event_id, subject, evidence_id, accepted);
    if !authorized_review(registry, reviewer_id, &message, signature) {
        return Err("unauthorized review signature".into());
    }
    append_review_once(journal, event_id, subject, evidence_id, accepted)
}

/// Encode a signed review in a journal entry. The registered reviewer key
/// must be available when the journal is replayed.
pub fn append_signed_review(
    journal: &mut Journal,
    registry: &Registry,
    reviewer_id: &str,
    event_id: &str,
    subject: &str,
    evidence_id: &str,
    accepted: bool,
    signature: &[u8; 64],
) -> Result<(), String> {
    let message = signed_review_message(event_id, subject, evidence_id, accepted);
    if !authorized_review(registry, reviewer_id, &message, signature) {
        return Err("unauthorized review signature".into());
    }
    let decision = if accepted { "accepted" } else { "rejected" };
    let encoded = format!(
        "{event_id}|{reviewer_id}|{subject}|{evidence_id}|{decision}|{}",
        hex_signature(signature)
    );
    if [event_id, reviewer_id, subject, evidence_id]
        .iter()
        .any(|value| value.is_empty() || value.contains('|'))
    {
        return Err("invalid signed review field".into());
    }
    if journal.entries().iter().any(|entry| {
        entry.kind == "signed_review" && entry.payload.split('|').next() == Some(event_id)
    }) {
        return Err("duplicate signed review".into());
    }
    journal
        .append("signed_review", &encoded)
        .map_err(str::to_owned)?;
    Ok(())
}

fn hex_signature(signature: &[u8; 64]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(128);
    for byte in signature {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 15) as usize] as char);
    }
    out
}

/// Validate all signed review entries against the current trusted registry.
/// Rejects duplicates, malformed entries and invalid signatures.
pub fn verify_signed_review_history(journal: &Journal, registry: &Registry) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    for entry in journal
        .entries()
        .iter()
        .filter(|entry| entry.kind == "signed_review")
    {
        let fields: Vec<&str> = entry.payload.split('|').collect();
        if fields.len() != 6 || !seen.insert(fields[0]) {
            return Err("malformed or duplicate signed review".into());
        }
        let accepted = match fields[4] {
            "accepted" => true,
            "rejected" => false,
            _ => return Err("invalid signed review decision".into()),
        };
        let hex = fields[5].as_bytes();
        if hex.len() != 128 {
            return Err("invalid signature length".into());
        }
        let mut signature = [0u8; 64];
        for (index, pair) in hex.chunks_exact(2).enumerate() {
            let digit = |byte: u8| -> Option<u8> {
                match byte {
                    b'0'..=b'9' => Some(byte - b'0'),
                    b'a'..=b'f' => Some(byte - b'a' + 10),
                    _ => None,
                }
            };
            signature[index] = (digit(pair[0]).ok_or("invalid hex")? << 4)
                | digit(pair[1]).ok_or("invalid hex")?;
        }
        let message = signed_review_message(fields[0], fields[2], fields[3], accepted);
        if !authorized_review(registry, fields[1], &message, &signature) {
            return Err("signed review verification failed".into());
        }
    }
    Ok(())
}

/// Compute a deterministic SHA-256 commitment to the ordered journal entries.
/// Keep the returned digest in a separate trusted store to detect deletion or reordering.
pub fn journal_commitment(journal: &Journal) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    hash.update(b"cybcore.journal.v1");
    for entry in journal.entries() {
        for value in [entry.kind.as_bytes(), entry.payload.as_bytes()] {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value);
        }
    }
    hash.finalize().into()
}

pub fn verify_journal_commitment(journal: &Journal, expected: &[u8; 32]) -> bool {
    &journal_commitment(journal) == expected
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

/// Write an audit journal and reopen it to verify the encoded records.
/// This is not atomic, authenticated, or crash-safe persistence.
pub fn persist_signed_mission_audit(path: &Path) -> Result<(), String> {
    let (score, missions) = signed_mission_demo()?;
    let mut journal = Journal::default();
    journal
        .append("mission", "m1:completed:proof1")
        .map_err(str::to_owned)?;
    append_review_once(&mut journal, "r1", "worker", "proof1", true)?;
    journal
        .append("trust", &format!("worker:{score}"))
        .map_err(str::to_owned)?;
    journal
        .append("growth", &format!("worker:{missions}"))
        .map_err(str::to_owned)?;
    cybmemory::save_checked(&journal, path).map_err(|e| e.to_string())?;
    let loaded = cybmemory::load_checked(path).map_err(|e| e.to_string())?;
    if loaded.entries() != journal.entries() {
        return Err("audit journal mismatch".into());
    }
    if replay_review_score(&loaded, "worker")? != score {
        return Err("replayed score mismatch".into());
    }
    Ok(())
}

/// Rebuild a score from journaled review events.
/// Each event id is counted once; malformed entries fail closed.
/// The journal is assumed to have passed integrity checks before replay.
pub fn replay_review_score(journal: &Journal, subject: &str) -> Result<i64, String> {
    use std::collections::HashSet;
    let mut seen = HashSet::new();
    let mut score = 0i64;
    for entry in journal
        .entries()
        .iter()
        .filter(|entry| entry.kind == "review_event")
    {
        let fields: Vec<&str> = entry.payload.split('|').collect();
        if fields.len() != 4 || fields.iter().any(|value| value.is_empty()) {
            return Err("invalid review event".into());
        }
        if !seen.insert(fields[0]) {
            return Err("duplicate review event".into());
        }
        if fields[1] == subject {
            score += match fields[3] {
                "accepted" => 1,
                "rejected" => -1,
                _ => return Err("invalid review decision".into()),
            };
        }
    }
    Ok(score)
}

/// Persist a new review only if the event identifier has never been recorded.
pub fn append_review_once(
    journal: &mut Journal,
    event_id: &str,
    subject: &str,
    evidence_id: &str,
    accepted: bool,
) -> Result<(), String> {
    if [event_id, subject, evidence_id]
        .iter()
        .any(|field| field.is_empty() || field.contains('|'))
    {
        return Err("invalid review field".into());
    }
    for entry in journal
        .entries()
        .iter()
        .filter(|entry| entry.kind == "review_event")
    {
        if entry.payload.split('|').next() == Some(event_id) {
            return Err("duplicate review event".into());
        }
    }
    let decision = if accepted { "accepted" } else { "rejected" };
    journal
        .append(
            "review_event",
            &format!("{event_id}|{subject}|{evidence_id}|{decision}"),
        )
        .map_err(str::to_owned)?;
    Ok(())
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
    fn audit_journal_roundtrip() {
        let path = std::env::temp_dir().join(format!(
            "cybcore-signed-missions-{}.bin",
            std::process::id()
        ));
        persist_signed_mission_audit(&path).unwrap();
        let journal = cybmemory::load_checked(&path).unwrap();
        assert_eq!(journal.find_kind("mission").len(), 1);
        assert_eq!(journal.find_kind("review_event").len(), 1);
        assert_eq!(journal.find_kind("trust").len(), 1);
        assert_eq!(journal.find_kind("growth").len(), 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn replay_rejects_duplicate_reviews() {
        let mut journal = Journal::default();
        append_review_once(&mut journal, "r1", "worker", "proof1", true).unwrap();
        assert!(append_review_once(&mut journal, "r1", "worker", "proof1", true).is_err());
        append_review_once(&mut journal, "r2", "worker", "proof2", false).unwrap();
        assert_eq!(replay_review_score(&journal, "worker").unwrap(), 0);
    }

    #[test]
    fn signed_review_rejects_tampered_decision() {
        let reviewer = Identity::generate();
        let mut registry = Registry::default();
        registry
            .register(AgentRecord {
                id: "reviewer".into(),
                public_key: reviewer.public_key(),
                capabilities: BTreeSet::from(["approve_mission".into()]),
            })
            .unwrap();
        let message = signed_review_message("r1", "worker", "proof1", true);
        let signature = reviewer.sign(&message);
        let mut journal = Journal::default();
        assert!(append_authorized_review(
            &mut journal,
            &registry,
            "reviewer",
            "r1",
            "worker",
            "proof1",
            false,
            &signature,
        )
        .is_err());
        assert!(journal.entries().is_empty());
    }

    #[test]
    fn signed_history_replay_verifies_and_rejects_duplicates() {
        let reviewer = Identity::generate();
        let mut registry = Registry::default();
        registry
            .register(AgentRecord {
                id: "reviewer".into(),
                public_key: reviewer.public_key(),
                capabilities: BTreeSet::from(["approve_mission".into()]),
            })
            .unwrap();
        let message = signed_review_message("r1", "worker", "proof1", true);
        let signature = reviewer.sign(&message);
        let mut journal = Journal::default();
        append_signed_review(
            &mut journal,
            &registry,
            "reviewer",
            "r1",
            "worker",
            "proof1",
            true,
            &signature,
        )
        .unwrap();
        verify_signed_review_history(&journal, &registry).unwrap();
        assert!(append_signed_review(
            &mut journal,
            &registry,
            "reviewer",
            "r1",
            "worker",
            "proof1",
            true,
            &signature,
        )
        .is_err());
    }

    #[test]
    fn journal_commitment_detects_reordering() {
        let mut first = Journal::default();
        first.append("a", "1").unwrap();
        first.append("b", "2").unwrap();
        let commitment = journal_commitment(&first);
        assert!(verify_journal_commitment(&first, &commitment));
        let mut reordered = Journal::default();
        reordered.append("b", "2").unwrap();
        reordered.append("a", "1").unwrap();
        assert!(!verify_journal_commitment(&reordered, &commitment));
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
