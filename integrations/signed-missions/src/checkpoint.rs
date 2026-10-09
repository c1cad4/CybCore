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
        &message(
            &checkpoint.reviewer_id,
            checkpoint.count,
            &checkpoint.digest,
        ),
        &checkpoint.signature,
    ) {
        return Err("checkpoint signature invalid");
    }
    Ok(())
}


/// A trusted witness can reject an older checkpoint even when its signature
/// remains valid. The witness counter must be stored independently.
pub fn verify_checkpoint_freshness(
    checkpoint: &Checkpoint,
    minimum_count: u64,
) -> Result<(), &'static str> {
    if checkpoint.count < minimum_count {
        return Err("checkpoint rollback detected");
    }
    Ok(())
}

/// Validate a checkpoint and enforce the latest externally witnessed count.
pub fn verify_witnessed_checkpoint(
    journal: &Journal,
    registry: &Registry,
    checkpoint: &Checkpoint,
    minimum_count: u64,
) -> Result<(), &'static str> {
    verify_checkpoint(journal, registry, checkpoint)?;
    verify_checkpoint_freshness(checkpoint, minimum_count)
}


/// A witness stores the latest count and digest outside the audited journal.
/// Updates require a verified, authorized checkpoint and never decrease count.
#[derive(Debug, Clone, Default)]
pub struct Witness {
    latest: Option<(u64, [u8; 32])>,
}

impl Witness {
    pub fn observe(
        &mut self,
        journal: &Journal,
        registry: &Registry,
        checkpoint: &Checkpoint,
    ) -> Result<(), &'static str> {
        verify_checkpoint(journal, registry, checkpoint)?;
        if let Some((count, digest)) = self.latest {
            if checkpoint.count < count {
                return Err("witness rollback detected");
            }
            if checkpoint.count == count && checkpoint.digest != digest {
                return Err("witness equivocation detected");
            }
        }
        self.latest = Some((checkpoint.count, checkpoint.digest));
        Ok(())
    }

    pub fn latest(&self) -> Option<(u64, [u8; 32])> {
        self.latest
    }
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
    fn checked_disk_recovery_with_external_checkpoint() {
        let identity = Identity::generate();
        let registry = registry_for(&identity);
        let mut journal = Journal::default();
        journal.append("mission", "m1").unwrap();
        journal.append("signed_review", "r1").unwrap();
        let checkpoint = sign_checkpoint(&journal, "auditor", &identity);
        let path =
            std::env::temp_dir().join(format!("cybcore-checkpoint-{}.bin", std::process::id()));
        cybmemory::save_checked_locked(&journal, &path).unwrap();
        let recovered = cybmemory::load_checked_locked(&path).unwrap();
        verify_checkpoint(&recovered, &registry, &checkpoint).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(path.with_extension("cyblock")).unwrap();
    }

    #[test]
    fn witnessed_count_rejects_valid_old_checkpoint() {
        let identity = Identity::generate();
        let registry = registry_for(&identity);
        let mut old = Journal::default();
        old.append("mission", "m1").unwrap();
        let old_checkpoint = sign_checkpoint(&old, "auditor", &identity);
        verify_checkpoint(&old, &registry, &old_checkpoint).unwrap();
        assert!(verify_witnessed_checkpoint(&old, &registry, &old_checkpoint, 2).is_err());
    }

    #[test]
    fn witness_rejects_fork_and_rollback() {
        let identity = Identity::generate();
        let registry = registry_for(&identity);
        let mut witness = Witness::default();
        let mut journal = Journal::default();
        journal.append("mission", "m1").unwrap();
        let first = sign_checkpoint(&journal, "auditor", &identity);
        witness.observe(&journal, &registry, &first).unwrap();
        let mut fork = Journal::default();
        fork.append("mission", "different").unwrap();
        let fork_checkpoint = sign_checkpoint(&fork, "auditor", &identity);
        assert!(witness.observe(&fork, &registry, &fork_checkpoint).is_err());
        journal.append("review", "r1").unwrap();
        let second = sign_checkpoint(&journal, "auditor", &identity);
        witness.observe(&journal, &registry, &second).unwrap();
        let mut old = Journal::default();
        old.append("mission", "m1").unwrap();
        assert!(witness.observe(&old, &registry, &first).is_err());
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
