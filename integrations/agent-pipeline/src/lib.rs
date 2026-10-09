//! Demonstrates an end-to-end, deterministic agent task lifecycle.
//! Dependencies are pinned to feature branches until their PRs are merged.
use cybagents::{run, Agent, Budget, Task};
use cybbrain::Memory;
use cybmemory::Journal;
use cybswarm::{independent_review, Evidence, Review};
use std::path::Path;
use std::time::Duration;

struct EchoAgent;
impl Agent for EchoAgent {
    fn execute(&self, input: &str) -> String {
        input.to_owned()
    }
}

pub fn execute_verified_task(input: &str) -> Result<(String, Memory, Journal), String> {
    let task = Task {
        id: "demo-1".into(),
        capability: "research".into(),
        input: input.into(),
        budget: Budget {
            max_runtime: Duration::from_secs(2),
            max_output_bytes: 4096,
        },
    };
    let output = run(&EchoAgent, &task, &["research"])
        .map_err(|err| format!("execution failed: {err:?}"))?;
    let proposal = Evidence {
        author: "researcher".into(),
        claim: output.clone(),
        source: "task:demo-1".into(),
    };
    // The evaluator here is a deterministic test fixture, NOT a real
    // independent verification of the claim's factual correctness.
    let verification = Evidence {
        author: "evaluator-fixture".into(),
        claim: output.clone(),
        source: "fixture:demo-1".into(),
    };
    if independent_review(&proposal, &verification) != Review::Accepted {
        return Err("review rejected".into());
    }
    let mut memory = Memory::default();
    memory
        .append(&proposal.author, &proposal.source, &proposal.claim)
        .map_err(|err| format!("memory failed: {err:?}"))?;
    let mut journal = Journal::default();
    journal.append("task", &task.id).map_err(str::to_owned)?;
    journal
        .append("review", "accepted")
        .map_err(str::to_owned)?;
    Ok((output, memory, journal))
}

/// Executes the demonstration pipeline and writes its event journal to disk.
/// A successful return means the journal was written and could be reopened.
/// This does not guarantee power-loss durability or factual verification.
pub fn execute_and_persist(input: &str, path: &Path) -> Result<String, String> {
    let (output, _memory, journal) = execute_verified_task(input)?;
    cybmemory::save(&journal, path).map_err(|err| format!("save failed: {err}"))?;
    let restored = cybmemory::load(path).map_err(|err| format!("load failed: {err}"))?;
    if restored.entries() != journal.entries() {
        return Err("journal roundtrip mismatch".into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn end_to_end_pipeline_records_result() {
        let (result, memory, journal) = execute_verified_task("bees forage").unwrap();
        assert_eq!(result, "bees forage");
        assert_eq!(memory.search("bees").len(), 1);
        assert_eq!(journal.find_kind("review").len(), 1);
    }

    #[test]
    fn pipeline_persists_and_recovers_journal() {
        let path = std::env::temp_dir().join(format!(
            "cybcore-pipeline-{}-{}.bin",
            std::process::id(),
            "roundtrip"
        ));
        let output = execute_and_persist("test claim", &path).unwrap();
        assert_eq!(output, "test claim");
        let journal = cybmemory::load(&path).unwrap();
        assert_eq!(journal.find_kind("task").len(), 1);
        assert_eq!(journal.find_kind("review").len(), 1);
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_output_over_budget() {
        let input = "x".repeat(4097);
        assert!(execute_verified_task(&input).is_err());
    }
}
