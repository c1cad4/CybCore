use cybidentity::{verify, Identity};
use cybmemory::Journal;
use cybregistry::Registry;
use sha2::{Digest, Sha256};

const DOMAIN: &[u8] = b"cybcore.audit.checkpoint.v1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checkpoint {
    pub reviewer_id: String,
    pub count: u64,
    pub digest: [u8; 32],
    pub signature: [u8; 64],
}

fn digest(journal: &Journal) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"cybcore.audit.entries.v1");
    for entry in journal.entries() {
        for value in [entry.kind.as_bytes(), entry.payload.as_bytes()] {
            hash.update((value.len() as u64).to_be_bytes());
            hash.update(value);
        }
    }
    hash.finalize().into()
}

fn message(reviewer_id: &str, count: u64, digest: &[u8; 32]) -> Vec<u8> {
    let mut result = DOMAIN.to_vec();
    result.extend_from_slice(&(reviewer_id.len() as u64).to_be_bytes());
    result.extend_from_slice(reviewer_id.as_bytes());
    result.extend_from_slice(&count.to_be_bytes());
    result.extend_from_slice(digest);
    result
}

/// Create a checkpoint that can be stored outside the journal.
pub fn sign_checkpoint(journal: &Journal, reviewer_id: &str, signer: &Identity) -> Checkpoint {
    let count = journal.entries().len() as u64;
    let digest = digest(journal);
    let signature = signer.sign(&message(reviewer_id, count, &digest));
    Checkpoint {
        reviewer_id: reviewer_id.to_owned(),
        count,
        digest,
        signature,
    }
}

/// Verify both the signed checkpoint and the complete ordered journal.
/// Trust depends on securely provisioning the reviewer registry.
pub fn verify_checkpoint(
    journal: &Journal,
    registry: &Registry,
    checkpoint: &Checkpoint,
) -> Result<(), &'static str> {
    let reviewer = registry
        .get(&checkpoint.reviewer_id)
        .ok_or("unknown checkpoint signer")?;
    if !reviewer.capabilities.contains("approve_mission") {
        return Err("checkpoint signer not authorized");
    }
    if checkpoint.count != journal.entries().len() as u64 || checkpoint.digest != digest(journal) {
        return Err("journal history differs from checkpoint");
    }
    if !verify(
        &reviewer.public_key,
        &message(&checkpoint.reviewer_id, checkpoint.count, &checkpoint.digest),
        &checkpoint.signature,
    ) {
        return Err("checkpoint signature invalid");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cybregistry::AgentRecord;
    use std::collections::BTreeSet;

    fn registry_for(identity: &Identity) -> Registry {
        let mut registry = Registry::default();
        registry
            .register(AgentRecord {
                id: "auditor".into(),
                public_key: identity.public_key(),
                capabilities: BTreeSet::from(["approve_mission".into()]),
            })
            .unwrap();
        registry
    }

    #[test]
    fn checkpoint_roundtrip_and_missing_event() {
        let identity = Identity::generate();
        let registry = registry_for(&identity);
        let mut journal = Journal::default();
        journal.append("mission", "m1").unwrap();
        journal.append("review", "r1").unwrap();
        let checkpoint = sign_checkpoint(&journal, "auditor", &identity);
        verify_checkpoint(&journal, &registry, &checkpoint).unwrap();
        let mut truncated = Journal::default();
        truncated.append("mission", "m1").unwrap();
        assert!(verify_checkpoint(&truncated, &registry, &checkpoint).is_err());
    }

    #[test]
    fn rejects_wrong_signer_and_reordered_history() {
        let identity = Identity::generate();
        let impostor = Identity::generate();
        let registry = registry_for(&identity);
        let mut journal = Journal::default();
        journal.append("mission", "m1").unwrap();
        journal.append("review", "r1").unwrap();
        let forged = sign_checkpoint(&journal, "auditor", &impostor);
        assert!(verify_checkpoint(&journal, &registry, &forged).is_err());
        let valid = sign_checkpoint(&journal, "auditor", &identity);
        let mut reordered = Journal::default();
        reordered.append("review", "r1").unwrap();
        reordered.append("mission", "m1").unwrap();
        assert!(verify_checkpoint(&reordered, &registry, &valid).is_err());
    }

    #[test]
    fn rejects_unauthorized_signer() {
        let identity = Identity::generate();
        let registry = Registry::default();
        let journal = Journal::default();
        let checkpoint = sign_checkpoint(&journal, "auditor", &identity);
        assert!(verify_checkpoint(&journal, &registry, &checkpoint).is_err());
    }
}
