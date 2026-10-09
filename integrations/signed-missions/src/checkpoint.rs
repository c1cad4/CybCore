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

/// Serialize the witness watermark for storage independent of the journal.
/// A valid signature must still be checked against the current registry.
pub fn encode_witness(witness: &Witness) -> Vec<u8> {
    let mut bytes = b"CYBWIT1".to_vec();
    match witness.latest() {
        Some((count, digest)) => {
            bytes.push(1);
            bytes.extend_from_slice(&count.to_be_bytes());
            bytes.extend_from_slice(&digest);
        }
        None => bytes.push(0),
    }
    bytes
}

pub fn decode_witness(bytes: &[u8]) -> Result<Witness, &'static str> {
    match bytes {
        b"CYBWIT1\x00" => Ok(Witness::default()),
        [b'C', b'Y', b'B', b'W', b'I', b'T', b'1', 1, rest @ ..] if rest.len() == 40 => {
            let mut count_bytes = [0u8; 8];
            count_bytes.copy_from_slice(&rest[..8]);
            let mut digest = [0u8; 32];
            digest.copy_from_slice(&rest[8..]);
            Ok(Witness {
                latest: Some((u64::from_be_bytes(count_bytes), digest)),
            })
        }
        _ => Err("invalid witness payload"),
    }
}

/// Persist a witness watermark with a temporary file and rename.
/// Callers must serialize concurrent updates and protect the witness path.
pub fn save_witness(witness: &Witness, path: &std::path::Path) -> std::io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension("cybwtmp");
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)?;
    file.write_all(&encode_witness(witness))?;
    file.sync_all()?;
    std::fs::rename(temporary, path)
}

pub fn load_witness(path: &std::path::Path) -> std::io::Result<Witness> {
    let bytes = std::fs::read(path)?;
    decode_witness(&bytes)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

/// Hold an exclusive OS advisory lock across the complete witness read,
/// verified monotonic update and atomic replacement.
pub fn observe_witness_locked(
    witness_path: &std::path::Path,
    journal: &Journal,
    registry: &Registry,
    checkpoint: &Checkpoint,
) -> std::io::Result<()> {
    use fs2::FileExt;
    let lock_path = witness_path.with_extension("cybwlock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.lock_exclusive()?;
    let result = (|| {
        let mut witness = match load_witness(witness_path) {
            Ok(witness) => witness,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Witness::default(),
            Err(error) => return Err(error),
        };
        witness
            .observe(journal, registry, checkpoint)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        save_witness(&witness, witness_path)
    })();
    FileExt::unlock(&lock)?;
    result
}

/// Load a journal and require its externally witnessed watermark to match.
/// A missing witness is an error: never silently trust a fresh empty watermark.
pub fn recover_witnessed_journal(
    journal_path: &std::path::Path,
    witness_path: &std::path::Path,
    registry: &Registry,
    checkpoint: &Checkpoint,
) -> std::io::Result<Journal> {
    let journal = cybmemory::load_checked_locked(journal_path)?;
    verify_checkpoint(&journal, registry, checkpoint)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let witness = load_witness(witness_path)?;
    match witness.latest() {
        Some((count, digest)) if checkpoint.count == count && checkpoint.digest == digest => {
            Ok(journal)
        }
        _ => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "journal differs from witnessed state",
        )),
    }
}

/// Coordinate cooperative writers across journal and witness files.
/// The lock covers the complete critical section, including recovery checks.
pub fn with_audit_transaction_lock<T>(
    lock_path: &std::path::Path,
    action: impl FnOnce() -> std::io::Result<T>,
) -> std::io::Result<T> {
    use fs2::FileExt;
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    file.lock_exclusive()?;
    let result = action();
    FileExt::unlock(&file)?;
    result
}

/// Commit a prepared journal and matching checkpoint while coordinating
/// cooperative writers. An interrupted commit must be reconciled explicitly.
pub fn commit_witnessed_journal(
    lock_path: &std::path::Path,
    journal_path: &std::path::Path,
    witness_path: &std::path::Path,
    journal: &Journal,
    registry: &Registry,
    checkpoint: &Checkpoint,
) -> std::io::Result<()> {
    with_audit_transaction_lock(lock_path, || {
        verify_checkpoint(journal, registry, checkpoint)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        if witness_path.exists() {
            let previous = load_witness(witness_path)?;
            if let Some((count, digest)) = previous.latest() {
                if checkpoint.count < count
                    || (checkpoint.count == count && checkpoint.digest != digest)
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "witness rollback or fork",
                    ));
                }
            }
        }
        cybmemory::save_checked_locked(journal, journal_path)?;
        observe_witness_locked(witness_path, journal, registry, checkpoint)
    })
}

/// A recoverable intent record: on restart, either finalize the prepared
/// signed checkpoint or refuse an ambiguous state.
pub fn encode_pending_checkpoint(checkpoint: &Checkpoint) -> Vec<u8> {
    let mut bytes = b"CYBPEND1".to_vec();
    let reviewer = checkpoint.reviewer_id.as_bytes();
    bytes.extend_from_slice(&(reviewer.len() as u64).to_be_bytes());
    bytes.extend_from_slice(reviewer);
    bytes.extend_from_slice(&checkpoint.count.to_be_bytes());
    bytes.extend_from_slice(&checkpoint.digest);
    bytes.extend_from_slice(&checkpoint.signature);
    bytes
}

pub fn decode_pending_checkpoint(bytes: &[u8]) -> Result<Checkpoint, &'static str> {
    if !bytes.starts_with(b"CYBPEND1") || bytes.len() < 8 + 8 + 8 + 32 + 64 {
        return Err("invalid pending checkpoint");
    }
    let length = u64::from_be_bytes(
        bytes[8..16]
            .try_into()
            .map_err(|_| "invalid reviewer length")?,
    );
    let length = usize::try_from(length).map_err(|_| "reviewer too long")?;
    if length > 1024 || bytes.len() != 120 + length {
        return Err("invalid pending length");
    }
    let reviewer_id = String::from_utf8(bytes[16..16 + length].to_vec())
        .map_err(|_| "invalid reviewer encoding")?;
    let offset = 16 + length;
    let count = u64::from_be_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .map_err(|_| "invalid count")?,
    );
    let mut digest = [0u8; 32];
    digest.copy_from_slice(&bytes[offset + 8..offset + 40]);
    let mut signature = [0u8; 64];
    signature.copy_from_slice(&bytes[offset + 40..offset + 104]);
    Ok(Checkpoint {
        reviewer_id,
        count,
        digest,
        signature,
    })
}

/// Read a durable pending intent and verify it against the journal.
/// Never accept a pending intent merely because its bytes parse.
pub fn recover_pending_checkpoint(
    pending_path: &std::path::Path,
    journal_path: &std::path::Path,
    registry: &Registry,
) -> std::io::Result<Checkpoint> {
    let checkpoint = decode_pending_checkpoint(&std::fs::read(pending_path)?)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    let journal = cybmemory::load_checked_locked(journal_path)?;
    verify_checkpoint(&journal, registry, &checkpoint)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    Ok(checkpoint)
}

/// Commit with a durable pending checkpoint marker. A crash after writing
/// the marker can be inspected with recover_pending_checkpoint.
pub fn commit_witnessed_journal_with_intent(
    transaction_lock: &std::path::Path,
    journal_path: &std::path::Path,
    witness_path: &std::path::Path,
    pending_path: &std::path::Path,
    journal: &Journal,
    registry: &Registry,
    checkpoint: &Checkpoint,
) -> std::io::Result<()> {
    use std::io::Write;
    with_audit_transaction_lock(transaction_lock, || {
        verify_checkpoint(journal, registry, checkpoint)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
        if pending_path.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "unresolved pending audit transaction",
            ));
        }
        if witness_path.exists() {
            let witness = load_witness(witness_path)?;
            if let Some((count, digest)) = witness.latest() {
                if checkpoint.count < count
                    || (checkpoint.count == count && checkpoint.digest != digest)
                {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "checkpoint rollback or fork",
                    ));
                }
            }
        }
        let mut marker = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(pending_path)?;
        marker.write_all(&encode_pending_checkpoint(checkpoint))?;
        marker.sync_all()?;
        cybmemory::save_checked_locked(journal, journal_path)?;
        observe_witness_locked(witness_path, journal, registry, checkpoint)?;
        std::fs::remove_file(pending_path)
    })
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
    fn witness_encoding_roundtrip_and_corruption() {
        let identity = Identity::generate();
        let registry = registry_for(&identity);
        let mut journal = Journal::default();
        journal.append("mission", "m1").unwrap();
        let checkpoint = sign_checkpoint(&journal, "auditor", &identity);
        let mut witness = Witness::default();
        witness.observe(&journal, &registry, &checkpoint).unwrap();
        let encoded = encode_witness(&witness);
        assert_eq!(decode_witness(&encoded).unwrap().latest(), witness.latest());
        assert!(decode_witness(&encoded[..encoded.len() - 1]).is_err());
    }

    #[test]
    fn locked_witness_rejects_stale_update_after_restart() {
        let identity = Identity::generate();
        let registry = registry_for(&identity);
        let path = std::env::temp_dir().join(format!("cybcore-witness-{}.bin", std::process::id()));
        let mut journal = Journal::default();
        journal.append("mission", "m1").unwrap();
        let old = sign_checkpoint(&journal, "auditor", &identity);
        observe_witness_locked(&path, &journal, &registry, &old).unwrap();
        journal.append("review", "r1").unwrap();
        let newest = sign_checkpoint(&journal, "auditor", &identity);
        observe_witness_locked(&path, &journal, &registry, &newest).unwrap();
        let mut previous = Journal::default();
        previous.append("mission", "m1").unwrap();
        assert!(observe_witness_locked(&path, &previous, &registry, &old).is_err());
        assert_eq!(load_witness(&path).unwrap().latest().unwrap().0, 2);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_file(path.with_extension("cybwlock")).unwrap();
    }

    #[test]
    fn recovery_rejects_stale_checkpoint_and_missing_witness() {
        let identity = Identity::generate();
        let registry = registry_for(&identity);
        let journal_path =
            std::env::temp_dir().join(format!("cybcore-recover-{}.bin", std::process::id()));
        let witness_path = std::env::temp_dir().join(format!(
            "cybcore-recover-witness-{}.bin",
            std::process::id()
        ));
        let mut journal = Journal::default();
        journal.append("mission", "m1").unwrap();
        let old = sign_checkpoint(&journal, "auditor", &identity);
        cybmemory::save_checked_locked(&journal, &journal_path).unwrap();
        assert!(recover_witnessed_journal(&journal_path, &witness_path, &registry, &old).is_err());
        observe_witness_locked(&witness_path, &journal, &registry, &old).unwrap();
        recover_witnessed_journal(&journal_path, &witness_path, &registry, &old).unwrap();
        journal.append("review", "r2").unwrap();
        let latest = sign_checkpoint(&journal, "auditor", &identity);
        cybmemory::save_checked_locked(&journal, &journal_path).unwrap();
        observe_witness_locked(&witness_path, &journal, &registry, &latest).unwrap();
        assert!(recover_witnessed_journal(&journal_path, &witness_path, &registry, &old).is_err());
        recover_witnessed_journal(&journal_path, &witness_path, &registry, &latest).unwrap();
        std::fs::remove_file(&journal_path).unwrap();
        std::fs::remove_file(journal_path.with_extension("cyblock")).unwrap();
        std::fs::remove_file(&witness_path).unwrap();
        std::fs::remove_file(witness_path.with_extension("cybwlock")).unwrap();
    }

    #[test]
    fn transaction_commit_and_recovery_roundtrip() {
        let identity = Identity::generate();
        let registry = registry_for(&identity);
        let base = std::env::temp_dir().join(format!("cybcore-txn-{}", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        let journal_path = base.join("journal.bin");
        let witness_path = base.join("witness.bin");
        let transaction_lock = base.join("transaction.lock");
        let mut journal = Journal::default();
        journal.append("mission", "m1").unwrap();
        let first = sign_checkpoint(&journal, "auditor", &identity);
        commit_witnessed_journal(
            &transaction_lock,
            &journal_path,
            &witness_path,
            &journal,
            &registry,
            &first,
        )
        .unwrap();
        recover_witnessed_journal(&journal_path, &witness_path, &registry, &first).unwrap();
        journal.append("review", "r2").unwrap();
        let second = sign_checkpoint(&journal, "auditor", &identity);
        commit_witnessed_journal(
            &transaction_lock,
            &journal_path,
            &witness_path,
            &journal,
            &registry,
            &second,
        )
        .unwrap();
        assert!(
            recover_witnessed_journal(&journal_path, &witness_path, &registry, &first).is_err()
        );
        recover_witnessed_journal(&journal_path, &witness_path, &registry, &second).unwrap();
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn pending_checkpoint_codec_roundtrip_and_corruption() {
        let identity = Identity::generate();
        let journal = Journal::default();
        let checkpoint = sign_checkpoint(&journal, "auditor", &identity);
        let bytes = encode_pending_checkpoint(&checkpoint);
        assert_eq!(decode_pending_checkpoint(&bytes).unwrap(), checkpoint);
        assert!(decode_pending_checkpoint(&bytes[..bytes.len() - 1]).is_err());
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
