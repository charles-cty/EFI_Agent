use super::*;
use crate::model::ApiFormat;
use alloc::{collections::VecDeque, string::ToString};
use serde_json::{Value, json};

struct Model {
    api: ApiFormat,
    limits: Limits,
    replies: VecDeque<Result<ChatMessage, String>>,
    summary: Result<String, String>,
    requests: Vec<(Purpose, Value)>,
    effects: usize,
    summaries: usize,
    cancel_summary: bool,
    cancelled: bool,
    reserve: usize,
    file: String,
}

impl Model {
    fn new(api: ApiFormat) -> Self {
        Self {
            api,
            limits: Limits::default(),
            replies: VecDeque::new(),
            summary: Ok("Checked earlier exchanges; continue the task.".into()),
            requests: Vec::new(),
            effects: 0,
            summaries: 0,
            cancel_summary: false,
            cancelled: false,
            reserve: 0,
            file: String::new(),
        }
    }
}

impl Environment for Model {
    fn limits(&self) -> Limits {
        self.limits.clone()
    }
    fn request_bytes(&self, messages: &[ChatMessage], purpose: Purpose) -> Result<usize, String> {
        request::bytes(self.api, "m", messages, "medium", purpose)
    }
    fn result_reserve(&self, operation: &Operation) -> usize {
        if self.reserve != 0 {
            return self.reserve;
        }
        match operation {
            Operation::Read { .. } => 6 * MAX_FILE_BYTES + 32768,
            _ => 32768,
        }
    }
    fn cancelled(&self) -> bool {
        self.cancelled
    }
    fn complete(
        &mut self,
        messages: &[ChatMessage],
        purpose: Purpose,
        _: &mut dyn FnMut(&str),
    ) -> Result<ChatMessage, String> {
        let body = request::build(self.api, "m", messages, "medium", purpose)?;
        assert!(serde_json::to_vec(&body).unwrap().len() <= self.limits.request_bytes);
        self.requests.push((purpose, body));
        if purpose == Purpose::Summary {
            self.summaries += 1;
            self.cancelled = self.cancel_summary;
            return self
                .summary
                .clone()
                .map(|text| ChatMessage::text("assistant", text));
        }
        self.replies.pop_front().expect("Unexpected model request")
    }
    fn execute(&mut self, operation: Operation) -> Result<String, String> {
        self.effects += 1;
        match operation {
            Operation::Write { content, .. } => {
                self.file = content;
                Ok("File saved".into())
            }
            Operation::Read { .. } => Ok(self.file.clone()),
            _ => Ok("done".into()),
        }
    }
}

fn calls(api: ApiFormat, names: &[(&str, Value)]) -> ChatMessage {
    let calls:Vec<Value>=names.iter().enumerate().map(|(i,(name,args))|json!({"id":format!("call-{i}"),"type":"function","function":{"name":name,"arguments":args.to_string()},"signature":{"opaque":"SIGNED_419","null":null}})).collect();
    let mut message:ChatMessage=serde_json::from_value(json!({"role":"assistant","content":null,"tool_calls":calls,"reasoning_details":[{"type":"reasoning.encrypted","data":"OPAQUE_419","signature":null}]})).unwrap();
    if api == ApiFormat::Responses {
        message.response_items.push(
            json!({"type":"reasoning","id":"r","encrypted_content":"OPAQUE_419","signature":null}),
        );
        for call in &message.tool_calls {
            message.response_items.push(json!({"type":"function_call","call_id":call.id,"name":call.function.name,"arguments":call.function.arguments,"signature":"SIGNED_419"}));
        }
    }
    message
}

fn completed_history(agent: &mut Agent) {
    for index in 0..4 {
        agent
            .messages
            .push(ChatMessage::text("user", format!("old {index}")));
        agent
            .messages
            .push(ChatMessage::text("assistant", "record 中 ".repeat(2000)));
    }
}

#[test]
fn compaction_shrinks_completed_groups_within_one_long_user_turn() {
    for api in [ApiFormat::ChatCompletions, ApiFormat::Responses] {
        let mut agent = Agent::default();
        agent
            .messages
            .push(ChatMessage::text("user", "one long current task".into()));
        for n in 0..5 {
            let mut call = calls(api, &[("shell", json!({"command":format!("echo {n}")}))]);
            call.retain_canonical();
            agent.messages.push(call);
            let mut result = ChatMessage::text("tool", format!("result {} ", "x".repeat(90000)));
            result.tool_call_id = Some("call-0".into());
            agent.messages.push(result);
        }
        agent.messages.push(ChatMessage::text(
            "assistant",
            "latest group completed".into(),
        ));
        let user = agent.messages[1].clone();
        let mut model = Model::new(api);
        let before = model.request_bytes(&agent.messages, Purpose::Task).unwrap();
        agent.compact(&mut model, |_| {}).unwrap();
        let after = model.request_bytes(&agent.messages, Purpose::Task).unwrap();
        assert!(after < before);
        assert!(
            agent
                .messages
                .iter()
                .any(|message| message.content == user.content && !message.checkpoint)
        );
        assert!(
            agent
                .messages
                .iter()
                .any(|message| message.content.as_deref() == Some("latest group completed"))
        );
        assert!(agent.messages.iter().any(|message| message.checkpoint));
    }
}

#[test]
fn adding_user_prompt_changes_complete_request_by_exact_message_bytes() {
    for api in [ApiFormat::ChatCompletions, ApiFormat::Responses] {
        let mut agent = Agent::default();
        let model = Model::new(api);
        let before = model.request_bytes(&agent.messages, Purpose::Task).unwrap();
        let prompt = "quoted \u{0001} 中😀\n".repeat(10000);
        let message_bytes = budget::text_message_bytes("user", &prompt);
        agent.messages.push(ChatMessage::text("user", prompt));
        assert_eq!(
            model.request_bytes(&agent.messages, Purpose::Task).unwrap(),
            before + 1 + message_bytes
        );
    }
}

#[test]
fn full_512k_write_and_read_continue_in_both_apis_and_execute_once() {
    for api in [ApiFormat::ChatCompletions, ApiFormat::Responses] {
        let mut agent = Agent::default();
        let mut model = Model::new(api);
        let payload = "\u{0001}".repeat(MAX_FILE_BYTES);
        model.replies.push_back(Ok(calls(
            api,
            &[("write", json!({"path":"nested/file","content":payload}))],
        )));
        model
            .replies
            .push_back(Ok(calls(api, &[("read", json!({"path":"nested/file"}))])));
        model
            .replies
            .push_back(Ok(ChatMessage::text("assistant", "verified".into())));
        agent
            .turn("write and read".into(), &mut model, |_| {})
            .unwrap();
        assert_eq!(model.effects, 2);
        assert_eq!(model.file, payload);
        assert!(model.summaries > 0);
        assert!(agent.compactions > 0);
        assert!(
            model
                .requests
                .iter()
                .filter(|(p, _)| *p == Purpose::Task)
                .any(|(_, body)| serde_json::to_vec(body).unwrap().len() > 640 * 1024)
        );
        let final_body = &model.requests.last().unwrap().1;
        let encoded = final_body.to_string();
        assert!(encoded.contains("OPAQUE_419") && encoded.contains("SIGNED_419"));
        assert!(
            agent
                .messages
                .iter()
                .any(|m| m.role == "tool" && m.content.as_deref() == Some(payload.as_str()))
        );
    }
}

#[test]
fn oversized_call_groups_are_rejected_before_any_effect_and_all_ids_are_paired() {
    for api in [ApiFormat::ChatCompletions, ApiFormat::Responses] {
        let mut model = Model::new(api);
        model.reserve = 3 * 1024 * 1024;
        model.replies.push_back(Ok(calls(
            api,
            &[
                ("write", json!({"path":"a","content":"one"})),
                ("write", json!({"path":"b","content":"two"})),
            ],
        )));
        model.replies.push_back(Ok(ChatMessage::text(
            "assistant",
            "use smaller groups".into(),
        )));
        let mut agent = Agent::default();
        agent
            .turn("two effects".into(), &mut model, |_| {})
            .unwrap();
        assert_eq!(model.effects, 0);
        let results: Vec<_> = agent.messages.iter().filter(|m| m.role == "tool").collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].tool_call_id.as_deref(), Some("call-0"));
        assert_eq!(results[1].tool_call_id.as_deref(), Some("call-1"));
        assert!(
            results
                .iter()
                .all(|m| m.content.as_ref().unwrap().contains("not executed"))
        );
    }
}

#[test]
fn failed_or_cancelled_admission_compaction_leaves_completed_calls_paired() {
    for cancel in [false, true] {
        let mut model = Model::new(ApiFormat::ChatCompletions);
        model.reserve = 4 * 1024 * 1024;
        if cancel {
            model.cancel_summary = true;
        } else {
            model.summary = Err("provider unavailable".into());
        }
        model.replies.push_back(Ok(calls(
            model.api,
            &[
                ("write", json!({"path":"a","content":"one"})),
                ("write", json!({"path":"b","content":"two"})),
            ],
        )));
        let mut agent = Agent::default();
        completed_history(&mut agent);
        let original = serde_json::to_value(&agent.messages).unwrap();
        assert!(agent.turn("do work".into(), &mut model, |_| {}).is_err());
        assert_eq!(model.effects, 0);
        assert_eq!(
            serde_json::to_value(&agent.messages[..original.as_array().unwrap().len()]).unwrap(),
            original
        );
        assert_eq!(
            agent
                .messages
                .iter()
                .rev()
                .take(2)
                .filter(|m| m.role == "tool")
                .count(),
            2
        );
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
    }
}

#[test]
fn compaction_is_atomic_and_preserves_exact_current_user_and_latest_opaque_group() {
    for api in [ApiFormat::ChatCompletions, ApiFormat::Responses] {
        let mut agent = Agent::default();
        completed_history(&mut agent);
        let current = ChatMessage::text(
            "user",
            "[Conversation checkpoint]\nActual current user instruction".into(),
        );
        agent.messages.push(current.clone());
        let mut reply = calls(api, &[("read", json!({"path":"a"}))]);
        reply.retain_canonical();
        agent.messages.push(reply);
        let mut result = ChatMessage::text("tool", "read result".into());
        result.tool_call_id = Some("call-0".into());
        agent.messages.push(result);
        let protected = serde_json::to_value(&agent.messages[agent.messages.len() - 3..]).unwrap();
        let mut model = Model::new(api);
        agent.compact(&mut model, |_| {}).unwrap();
        assert_eq!(
            serde_json::to_value(&agent.messages[agent.messages.len() - 3..]).unwrap(),
            protected
        );
        assert!(
            agent
                .messages
                .iter()
                .any(|m| m.content == current.content && !m.checkpoint)
        );
        let mut failed_agent = Agent::default();
        completed_history(&mut failed_agent);
        failed_agent
            .messages
            .push(ChatMessage::text("user", "current".into()));
        let before = serde_json::to_value(&failed_agent.messages).unwrap();
        model.summary = Err("failure".into());
        assert!(failed_agent.compact(&mut model, |_| {}).is_err());
        assert_eq!(
            serde_json::to_value(&failed_agent.messages).unwrap(),
            before
        );
    }
}

#[test]
fn provider_context_retries_once_without_repeating_completed_effects() {
    let api = ApiFormat::Responses;
    let mut agent = Agent::default();
    completed_history(&mut agent);
    let mut model = Model::new(api);
    model.replies.push_back(Ok(calls(
        api,
        &[("write", json!({"path":"a","content":"once"}))],
    )));
    model
        .replies
        .push_back(Err("ModelContextExceeded: smaller context required".into()));
    model
        .replies
        .push_back(Ok(ChatMessage::text("assistant", "done".into())));
    agent.turn("one effect".into(), &mut model, |_| {}).unwrap();
    assert_eq!(model.effects, 1);
    assert_eq!(agent.compactions, 1);
    assert_eq!(
        model
            .requests
            .iter()
            .filter(|(p, _)| *p == Purpose::Task)
            .count(),
        3
    );
}

#[test]
fn oversized_new_user_request_does_not_grow_retained_history_or_execute() {
    let mut agent = Agent::default();
    let before = serde_json::to_value(&agent.messages).unwrap();
    let mut model = Model::new(ApiFormat::ChatCompletions);
    assert!(
        agent
            .turn(
                "a".repeat(model.limits.history_bytes + 1),
                &mut model,
                |_| {}
            )
            .unwrap_err()
            .contains("BudgetExceeded")
    );
    assert_eq!(serde_json::to_value(&agent.messages).unwrap(), before);
    assert!(model.requests.is_empty());
    assert_eq!(model.effects, 0);
}

#[test]
fn invalid_summary_variants_preserve_history_and_no_eligible_prefix_makes_no_request() {
    for text in [String::new(), "x".repeat(65537), "x".repeat(65536)] {
        let mut agent = Agent::default();
        agent
            .messages
            .push(ChatMessage::text("user", "first".into()));
        agent
            .messages
            .push(ChatMessage::text("assistant", "small record".into()));
        agent
            .messages
            .push(ChatMessage::text("user", "current".into()));
        let before = serde_json::to_value(&agent.messages).unwrap();
        let mut model = Model::new(ApiFormat::ChatCompletions);
        model.summary = Ok(text);
        assert!(agent.compact(&mut model, |_| {}).is_err());
        assert_eq!(serde_json::to_value(&agent.messages).unwrap(), before);
        assert_eq!(model.effects, 0);
    }
    let mut agent = Agent::default();
    agent
        .messages
        .push(ChatMessage::text("user", "current".into()));
    let mut model = Model::new(ApiFormat::Responses);
    assert!(
        agent
            .compact(&mut model, |_| {})
            .unwrap()
            .starts_with("No earlier")
    );
    assert!(model.requests.is_empty());
}

#[test]
fn explicit_context_failure_is_retried_only_once_and_partial_history_cannot_be_summarized() {
    let mut agent = Agent::default();
    completed_history(&mut agent);
    let mut model = Model::new(ApiFormat::ChatCompletions);
    model.replies.extend([
        Err("ModelContextExceeded: first".into()),
        Err("ModelContextExceeded: second".into()),
    ]);
    assert!(
        agent
            .turn("retry once".into(), &mut model, |_| {})
            .unwrap_err()
            .contains("second")
    );
    assert_eq!(
        model
            .requests
            .iter()
            .filter(|(p, _)| *p == Purpose::Task)
            .count(),
        2
    );
    assert_eq!(agent.compactions, 1);
    let mut agent = Agent::default();
    agent.messages.push(ChatMessage::text("user", "old".into()));
    agent.messages.push(calls(
        ApiFormat::ChatCompletions,
        &[("read", json!({"path":"a"}))],
    ));
    agent
        .messages
        .push(ChatMessage::text("user", "current".into()));
    let mut model = Model::new(ApiFormat::ChatCompletions);
    assert!(
        agent
            .compact(&mut model, |_| {})
            .unwrap_err()
            .contains("incomplete tool group")
    );
    assert!(model.requests.is_empty());
}
