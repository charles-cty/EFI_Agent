use super::*;
use alloc::string::ToString;
use serde_json::json;

struct Model {
    limits: Limits,
    rounds: usize,
    effects: usize,
    requests: usize,
    fail_once: bool,
    partial: bool,
    pauses: usize,
}

impl Model {
    fn new(rounds: usize) -> Self {
        Self {
            limits: Limits::default(),
            rounds,
            effects: 0,
            requests: 0,
            fail_once: false,
            partial: false,
            pauses: 0,
        }
    }
}

impl Environment for Model {
    fn limits(&self) -> Limits {
        self.limits.clone()
    }
    fn retryable_error(&self, error: &str) -> bool {
        error == "temporary provider failure"
    }
    fn pause_retry(&mut self, _: u64) -> Result<(), String> {
        self.pauses += 1;
        Ok(())
    }
    fn complete(
        &mut self,
        _: &[ChatMessage],
        _: Purpose,
        progress: &mut dyn FnMut(&str),
    ) -> Result<ChatMessage, String> {
        self.requests += 1;
        if self.fail_once && self.effects == 1 {
            self.fail_once = false;
            if self.partial {
                progress("partial response");
            }
            return Err("temporary provider failure".into());
        }
        if self.effects == self.rounds {
            return Ok(ChatMessage::text("assistant", "done".into()));
        }
        serde_json::from_value(json!({"role":"assistant","content":null,"tool_calls":[{
            "id": format!("effect-{}", self.effects), "type":"function",
            "function":{"name":"write","arguments":"{\"path\":\"effect\",\"content\":\"one\"}"}
        }]}))
        .map_err(|error| error.to_string())
    }
    fn execute(&mut self, _: Operation) -> Result<String, String> {
        self.effects += 1;
        Ok("saved".into())
    }
}

#[test]
fn default_runs_beyond_twelve_rounds_and_transient_retry_does_not_replay_effects() {
    let mut model = Model::new(23);
    model.fail_once = true;
    Agent::default()
        .turn("finish the task".into(), &mut model, |_| {})
        .unwrap();
    assert_eq!((model.effects, model.requests, model.pauses), (23, 25, 1));
}

#[test]
fn partial_stream_failure_is_not_automatically_replayed() {
    let mut model = Model::new(2);
    model.fail_once = true;
    model.partial = true;
    assert!(
        Agent::default()
            .turn("work".into(), &mut model, |_| {})
            .is_err()
    );
    assert_eq!((model.effects, model.requests, model.pauses), (1, 2, 0));
}

#[test]
fn explicit_round_budget_pairs_rejected_calls_and_next_request_can_continue() {
    let mut model = Model::new(5);
    model.limits.max_tool_rounds = Some(3);
    let mut agent = Agent::default();
    assert!(agent.turn("work".into(), &mut model, |_| {}).is_err());
    assert_eq!(model.effects, 3);
    assert!(
        agent
            .messages
            .last()
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .contains("not executed")
    );
    agent.turn("continue".into(), &mut model, |_| {}).unwrap();
    assert_eq!(model.effects, 5);
}

#[test]
fn token_window_admission_rejects_protected_data_before_provider_or_tool_calls() {
    let mut model = Model::new(0);
    model.limits.context_window_tokens = Some(30_000);
    let mut agent = Agent::default();
    let error = agent
        .turn("x".repeat(100_000), &mut model, |_| {})
        .unwrap_err();
    assert!(error.contains("ContextTokenBudgetExceeded"), "{error}");
    assert_eq!((model.requests, model.effects), (0, 0));
    agent
        .turn("small request".into(), &mut model, |_| {})
        .unwrap();
    assert_eq!(model.requests, 1);
}
