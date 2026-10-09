# Witnessed audit storage protocol (v1)

The journal and its independent witness watermark are **not** one atomic file.
The checkpoint is signed by an authorized identity and binds the exact journal
entry count and ordered content digest. The witness watermark rejects rollback
and same-height forks only when its own storage is trusted.

## Normal writer sequence

1. Acquire a **single transaction lock** covering the journal and witness.
2. Load and verify the current journal, checkpoint and witness.
3. Prepare the new journal and a new signed checkpoint.
4. Save the journal with checked, locked I/O.
5. Update the witness under its exclusive advisory lock.
6. Publish the new signed checkpoint to a separate trusted location.
7. Release the transaction lock.

**Important:** steps 4–6 are not crash-atomic. A crash can leave a journal
ahead of its witness, or a witness ahead of the published checkpoint. On
recovery, refuse to accept a mismatched pair automatically; reconcile from
a previously verified checkpoint or trusted remote witness. Never silently
reset the watermark to zero.

## Threat model

- A SHA-256 digest detects accidental changes, not forgery by a writer.
- Ed25519 checkpoint signatures authenticate an authorized signing key;
  registry provisioning, key rotation and revocation remain external.
- File locks are advisory, require every participating writer to cooperate,
  and do not protect against malicious filesystem access.
- A witness on the same disk is not an independent trust anchor. Replicate
  witnessed checkpoints to another node or trusted append-only service.
- Signed checkpoints alone do not prevent replay of a previously valid
  checkpoint. A separately maintained latest-height/digest anchor is required.
- The journal's checked save currently uses fixed sibling temporary names.
  Never run independent legacy writers concurrently with locked writers.
- Do not treat the current prototype as durable distributed consensus.

## Recovery decision matrix

| Journal | Witness | Action |
|---|---|---|
| Valid and matches signed checkpoint | Same height and digest | Accept |
| Valid | Missing | Reject; provision trusted witness |
| Valid | Lower height | Stop; possible interrupted update |
| Valid | Higher height | Stop; possible rollback |
| Valid | Same height, different digest | Reject; fork or corruption |
| Invalid checksum | Any | Reject; restore from verified backup |

## Future hardening

Implement a transaction manifest with durable fsync on parent directories,
unique temporary files, multi-process fault-injection tests, signed checkpoint
serialization, secure external witness replication, and authenticated
identity/key lifecycle.
