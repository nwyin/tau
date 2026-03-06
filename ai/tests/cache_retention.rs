//! Mirrors: packages/ai/test/cache-retention.test.ts
//!
//! tau does not yet have provider-specific payload builders, so these tests
//! focus on the boundary we do have today: cache retention options must survive
//! through the provider registry and reach the selected provider unchanged.

mod common;
use common::{create_assistant_message, mock_model, registry_lock};

use ai::providers::{clear_api_providers, complete, complete_simple, register_api_provider, ApiProvider};
use ai::stream::{assistant_message_event_stream, AssistantMessageEventStream};
use ai::types::{CacheRetention, Context, Message, SimpleStreamOptions, StreamOptions, UserContent, UserMessage};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingProvider {
    raw_cache_retention: Mutex<Vec<Option<CacheRetention>>>,
    simple_cache_retention: Mutex<Vec<Option<CacheRetention>>>,
}

impl RecordingProvider {
    fn stream_with_text(&self, text: &str) -> AssistantMessageEventStream {
        let message = create_assistant_message(text);
        let (mut tx, stream) = assistant_message_event_stream();
        tokio::spawn(async move {
            tx.push(ai::types::AssistantMessageEvent::Start { partial: message.clone() });
            tx.push(ai::types::AssistantMessageEvent::Done {
                reason: message.stop_reason.clone(),
                message,
            });
        });
        stream
    }
}

impl ApiProvider for RecordingProvider {
    fn api(&self) -> &str {
        "test-cache-retention"
    }

    fn stream(
        &self,
        _model: &ai::types::Model,
        _context: &Context,
        options: Option<&StreamOptions>,
    ) -> AssistantMessageEventStream {
        self.raw_cache_retention
            .lock()
            .unwrap()
            .push(options.and_then(|opts| opts.cache_retention.clone()));
        self.stream_with_text("raw ok")
    }

    fn stream_simple(
        &self,
        _model: &ai::types::Model,
        _context: &Context,
        options: Option<&SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        self.simple_cache_retention
            .lock()
            .unwrap()
            .push(options.and_then(|opts| opts.base.cache_retention.clone()));
        self.stream_with_text("simple ok")
    }
}

fn sample_context() -> Context {
    Context {
        system_prompt: Some("You are a helpful assistant.".into()),
        messages: vec![Message::User(UserMessage {
            role: "user".into(),
            content: UserContent::Text("Hello".into()),
            timestamp: 0,
        })],
        tools: None,
    }
}

#[tokio::test]
async fn forwards_cache_retention_in_raw_stream_options() {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(RecordingProvider::default());
    register_api_provider(provider.clone());

    let mut opts = StreamOptions::default();
    opts.cache_retention = Some(CacheRetention::Long);

    let model = mock_model("test-cache-retention", "test");
    let response = complete(&model, &sample_context(), Some(&opts)).await.unwrap();

    assert_eq!(response.role, "assistant");
    assert_eq!(
        provider.raw_cache_retention.lock().unwrap().as_slice(),
        &[Some(CacheRetention::Long)]
    );

    clear_api_providers();
}

#[tokio::test]
async fn forwards_cache_retention_in_simple_stream_options() {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(RecordingProvider::default());
    register_api_provider(provider.clone());

    let mut opts = SimpleStreamOptions::default();
    opts.base.cache_retention = Some(CacheRetention::Short);

    let model = mock_model("test-cache-retention", "test");
    let response = complete_simple(&model, &sample_context(), Some(&opts)).await.unwrap();

    assert_eq!(response.role, "assistant");
    assert_eq!(
        provider.simple_cache_retention.lock().unwrap().as_slice(),
        &[Some(CacheRetention::Short)]
    );

    clear_api_providers();
}

#[tokio::test]
async fn preserves_none_cache_retention_when_unspecified() {
    let _guard = registry_lock();
    clear_api_providers();

    let provider = Arc::new(RecordingProvider::default());
    register_api_provider(provider.clone());

    let model = mock_model("test-cache-retention", "test");
    complete(&model, &sample_context(), Some(&StreamOptions::default())).await.unwrap();
    complete_simple(&model, &sample_context(), Some(&SimpleStreamOptions::default())).await.unwrap();

    assert_eq!(provider.raw_cache_retention.lock().unwrap().as_slice(), &[None]);
    assert_eq!(provider.simple_cache_retention.lock().unwrap().as_slice(), &[None]);

    clear_api_providers();
}
