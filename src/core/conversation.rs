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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_and_retrieves_exchanges() {
        let mut history = ConversationHistory::new();
        history.add_exchange("hello".into(), "hi there".into());
        history.add_exchange("how are you".into(), "doing great".into());

        let exchanges: Vec<_> = history.exchanges().collect();
        assert_eq!(exchanges.len(), 2);
        assert_eq!(exchanges[0].user_transcript, "hello");
        assert_eq!(exchanges[1].user_transcript, "how are you");
    }

    #[test]
    fn drops_oldest_when_exceeding_max() {
        let mut history = ConversationHistory::new();
        for i in 0..12 {
            history.add_exchange(format!("msg {}", i), format!("reply {}", i));
        }

        let exchanges: Vec<_> = history.exchanges().collect();
        assert_eq!(exchanges.len(), MAX_CONVERSATION_HISTORY_LENGTH);
        let first = exchanges[0];
        assert_eq!(first.user_transcript, "msg 2"); // 0 and 1 were dropped
    }
}
