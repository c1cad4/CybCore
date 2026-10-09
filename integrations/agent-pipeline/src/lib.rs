//! Demonstrates an end-to-end, deterministic agent task lifecycle.
//! Dependencies are pinned to feature branches until their PRs are merged.
use cybagents::{run, Agent, Budget, Task};
use cybbrain::Memory;
use cybmemory::Journal;
use cybswarm::{independent_review, Evidence, Review};
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
    journal
        .append("task", &task.id)
        .map_err(str::to_owned)?;
    journal
        .append("review", "accepted")
        .map_err(str::to_owned)?;
    Ok((output, memory, journal))
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
    fn rejects_output_over_budget() {
        let input = "x".repeat(4097);
        assert!(execute_verified_task(&input).is_err());
    }
}
