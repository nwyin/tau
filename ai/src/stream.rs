use std::pin::Pin;
use std::task::{Context, Poll};

use futures::Stream;
use tokio::sync::{mpsc, oneshot};

use crate::types::{AssistantMessage, AssistantMessageEvent, SimpleStreamOptions};

// ---------------------------------------------------------------------------
// Generic EventStream
// ---------------------------------------------------------------------------

/// Producer handle — push events into the stream and resolve the final result.
pub struct EventStreamSender<T: Clone> {
    tx: mpsc::UnboundedSender<T>,
    result_tx: Option<oneshot::Sender<T>>,
    is_complete: fn(&T) -> bool,
}

impl<T: Clone> EventStreamSender<T> {
    /// Push an event. If `is_complete` returns true, also resolves `result()`.
    pub fn push(&mut self, event: T) {
        if (self.is_complete)(&event) {
            if let Some(tx) = self.result_tx.take() {
                let _ = tx.send(event.clone());
            }
        }
        let _ = self.tx.send(event);
    }

    /// Explicitly close the stream without resolving result (drops sender).
    pub fn close(self) {
        drop(self.tx);
    }
}

/// Consumer handle — iterate events and await the final result.
pub struct EventStream<T> {
    rx: mpsc::UnboundedReceiver<T>,
    result_rx: oneshot::Receiver<T>,
}

impl<T: Clone + Unpin> Stream for EventStream<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

impl<T: Clone> EventStream<T> {
    /// Resolves with the terminal event once it has been pushed.
    pub async fn result(self) -> T {
        self.result_rx
            .await
            .expect("EventStream sender dropped without resolving result")
    }
}

/// Create a paired (sender, stream).
pub fn event_stream<T: Clone>(
    is_complete: fn(&T) -> bool,
) -> (EventStreamSender<T>, EventStream<T>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let (result_tx, result_rx) = oneshot::channel();
    let sender = EventStreamSender {
        tx,
        result_tx: Some(result_tx),
        is_complete,
    };
    let stream = EventStream { rx, result_rx };
    (sender, stream)
}

// ---------------------------------------------------------------------------
// AssistantMessageEventStream — specialisation for LLM streaming responses
// ---------------------------------------------------------------------------

pub type AssistantMessageEventSender = EventStreamSender<AssistantMessageEvent>;

pub struct AssistantMessageEventStream {
    inner: EventStream<AssistantMessageEvent>,
}

impl AssistantMessageEventStream {
    /// Drain all events and return the final AssistantMessage.
    pub async fn result(self) -> AssistantMessage {
        self.inner.result().await.into_message()
    }
}

impl Stream for AssistantMessageEventStream {
    type Item = AssistantMessageEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(&mut self.inner).poll_next(cx)
    }
}

pub fn assistant_message_event_stream() -> (AssistantMessageEventSender, AssistantMessageEventStream)
{
    let (sender, stream) = event_stream(|e: &AssistantMessageEvent| e.is_terminal());
    (sender, AssistantMessageEventStream { inner: stream })
}

/// Create a stream that immediately emits a single Error event.
///
/// Used by provider implementations when setup fails before streaming starts
/// (e.g. missing API key, request build error).
pub fn error_stream(
    model: &crate::types::Model,
    error: impl std::fmt::Display,
) -> AssistantMessageEventStream {
    let (mut tx, stream) = assistant_message_event_stream();
    let err_msg = AssistantMessage {
        role: "assistant".into(),
        content: Vec::new(),
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        usage: crate::types::Usage::default(),
        stop_reason: crate::types::StopReason::Error,
        error_message: Some(error.to_string()),
        timestamp: chrono::Utc::now().timestamp_millis(),
    };
    tx.push(AssistantMessageEvent::Error {
        reason: crate::types::StopReason::Error,
        error: err_msg,
    });
    stream
}

// ---------------------------------------------------------------------------
// StreamFn type alias — what provider implementations return
// ---------------------------------------------------------------------------

use crate::types::{Context as LlmContext, Model};

pub type StreamFn = Box<
    dyn Fn(Model, LlmContext, Option<SimpleStreamOptions>) -> AssistantMessageEventStream
        + Send
        + Sync,
>;
