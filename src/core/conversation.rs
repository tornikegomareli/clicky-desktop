/// Conversation history manager — maintains the last N exchanges for
/// Claude's context window.
/// Ported from CompanionManager.swift:686-694.
///
/// Each exchange is a (user_transcript, assistant_response) pair.
/// The history is capped at 10 exchanges to prevent unbounded context growth.

use std::collections::VecDeque;

const MAX_CONVERSATION_HISTORY_LENGTH: usize = 10;

/// A single exchange in the conversation.
#[derive(Debug, Clone)]
pub struct ConversationExchange {
    /// What the user said (transcribed from voice)
    pub user_transcript: String,

    /// Claude's spoken response (with POINT tags stripped)
    pub assistant_response: String,
}

/// Maintains a bounded history of user ↔ assistant exchanges.
/// Oldest exchanges are dropped when the limit is exceeded.
pub struct ConversationHistory {
    exchanges: VecDeque<ConversationExchange>,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            exchanges: VecDeque::with_capacity(MAX_CONVERSATION_HISTORY_LENGTH),
        }
    }

    /// Records a completed exchange. If the history exceeds the max length,
    /// the oldest exchange is removed.
    pub fn add_exchange(&mut self, user_transcript: String, assistant_response: String) {
        if self.exchanges.len() >= MAX_CONVERSATION_HISTORY_LENGTH {
            self.exchanges.pop_front();
        }
        self.exchanges.push_back(ConversationExchange {
            user_transcript,
            assistant_response,
        });
    }

    /// Returns all exchanges in chronological order (oldest first).
    pub fn exchanges(&self) -> impl Iterator<Item = &ConversationExchange> {
        self.exchanges.iter()
    }

    /// Returns the number of exchanges in history.
    pub fn len(&self) -> usize {
        self.exchanges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.exchanges.is_empty()
    }

    /// Clears all conversation history.
    pub fn clear(&mut self) {
        self.exchanges.clear();
    }

    /// Builds the messages array for the Claude API request, interleaving
    /// user and assistant messages from history followed by the current
    /// user message.
    pub fn build_claude_messages_payload(
        &self,
        current_user_message_content: serde_json::Value,
    ) -> Vec<serde_json::Value> {
        let mut messages = Vec::new();

        for exchange in &self.exchanges {
            messages.push(serde_json::json!({
                "role": "user",
                "content": exchange.user_transcript,
            }));
            messages.push(serde_json::json!({
                "role": "assistant",
                "content": exchange.assistant_response,
            }));
        }

        messages.push(serde_json::json!({
            "role": "user",
            "content": current_user_message_content,
        }));

        messages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_history_is_empty() {
        let history = ConversationHistory::new();
        assert!(history.is_empty());
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn adds_and_retrieves_exchanges() {
        let mut history = ConversationHistory::new();
        history.add_exchange("hello".into(), "hi there".into());
        history.add_exchange("how are you".into(), "doing great".into());

        assert_eq!(history.len(), 2);
        let exchanges: Vec<_> = history.exchanges().collect();
        assert_eq!(exchanges[0].user_transcript, "hello");
        assert_eq!(exchanges[1].user_transcript, "how are you");
    }

    #[test]
    fn drops_oldest_when_exceeding_max() {
        let mut history = ConversationHistory::new();
        for i in 0..12 {
            history.add_exchange(format!("msg {}", i), format!("reply {}", i));
        }

        assert_eq!(history.len(), MAX_CONVERSATION_HISTORY_LENGTH);
        let first = history.exchanges().next().unwrap();
        assert_eq!(first.user_transcript, "msg 2"); // 0 and 1 were dropped
    }

    #[test]
    fn builds_claude_messages_with_history() {
        let mut history = ConversationHistory::new();
        history.add_exchange("first question".into(), "first answer".into());

        let messages =
            history.build_claude_messages_payload(serde_json::json!("current question"));

        assert_eq!(messages.len(), 3); // 1 history pair + 1 current
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "first question");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[2]["content"], "current question");
    }
}
