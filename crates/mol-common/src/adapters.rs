//! Typed adapter interfaces and deterministic recording stubs.
//!
//! Each Python `Protocol` maps to an async trait with `Send + Sync` bounds.
//! The `Recording*` stubs capture calls in `Arc<Mutex<Vec<...>>>` for
//! thread-safe test assertions.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Response returned by [`WebFetchAdapter::fetch`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FetchResponse {
    pub url: String,
    pub status_code: u16,
    pub text: String,
}

/// Page snapshot returned by [`BrowserAdapter::open`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BrowserPage {
    pub url: String,
    pub title: String,
}

// ---------------------------------------------------------------------------
// Adapter traits (async, Send + Sync)
// ---------------------------------------------------------------------------

#[async_trait]
pub trait CronAdapter: Send + Sync {
    async fn schedule_resume(
        &self,
        run_id: &str,
        stage_id: i64,
        reason: &str,
    ) -> Result<String>;
}

#[async_trait]
pub trait MessageAdapter: Send + Sync {
    async fn notify(
        &self,
        channel: &str,
        subject: &str,
        body: &str,
    ) -> Result<String>;
}

#[async_trait]
pub trait MemoryAdapter: Send + Sync {
    async fn append(&self, namespace: &str, content: &str) -> Result<String>;
}

#[async_trait]
pub trait SessionsAdapter: Send + Sync {
    async fn spawn(&self, name: &str, command: &[String]) -> Result<String>;
}

#[async_trait]
pub trait WebFetchAdapter: Send + Sync {
    async fn fetch(&self, url: &str) -> Result<FetchResponse>;
}

#[async_trait]
pub trait BrowserAdapter: Send + Sync {
    async fn open(&self, url: &str) -> Result<BrowserPage>;
}

// ---------------------------------------------------------------------------
// Recording stubs
// ---------------------------------------------------------------------------

/// Recording stub for [`CronAdapter`].
#[derive(Debug, Default, Clone)]
pub struct RecordingCronAdapter {
    pub calls: Arc<Mutex<Vec<(String, i64, String)>>>,
}

#[async_trait]
impl CronAdapter for RecordingCronAdapter {
    async fn schedule_resume(
        &self,
        run_id: &str,
        stage_id: i64,
        reason: &str,
    ) -> Result<String> {
        let mut calls = self.calls.lock().unwrap();
        calls.push((run_id.to_owned(), stage_id, reason.to_owned()));
        Ok(format!("cron-{}", calls.len()))
    }
}

/// Recording stub for [`MessageAdapter`].
#[derive(Debug, Default, Clone)]
pub struct RecordingMessageAdapter {
    pub calls: Arc<Mutex<Vec<(String, String, String)>>>,
}

#[async_trait]
impl MessageAdapter for RecordingMessageAdapter {
    async fn notify(
        &self,
        channel: &str,
        subject: &str,
        body: &str,
    ) -> Result<String> {
        let mut calls = self.calls.lock().unwrap();
        calls.push((
            channel.to_owned(),
            subject.to_owned(),
            body.to_owned(),
        ));
        Ok(format!("message-{}", calls.len()))
    }
}

/// Recording stub for [`MemoryAdapter`].
#[derive(Debug, Default, Clone)]
pub struct RecordingMemoryAdapter {
    pub entries: Arc<Mutex<Vec<(String, String)>>>,
}

#[async_trait]
impl MemoryAdapter for RecordingMemoryAdapter {
    async fn append(&self, namespace: &str, content: &str) -> Result<String> {
        let mut entries = self.entries.lock().unwrap();
        entries.push((namespace.to_owned(), content.to_owned()));
        Ok(format!("memory-{}", entries.len()))
    }
}

/// Recording stub for [`SessionsAdapter`].
#[derive(Debug, Default, Clone)]
pub struct RecordingSessionsAdapter {
    pub calls: Arc<Mutex<Vec<(String, Vec<String>)>>>,
}

#[async_trait]
impl SessionsAdapter for RecordingSessionsAdapter {
    async fn spawn(&self, name: &str, command: &[String]) -> Result<String> {
        let mut calls = self.calls.lock().unwrap();
        calls.push((name.to_owned(), command.to_vec()));
        Ok(format!("session-{}", calls.len()))
    }
}

/// Recording stub for [`WebFetchAdapter`].
#[derive(Debug, Default, Clone)]
pub struct RecordingWebFetchAdapter {
    pub calls: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl WebFetchAdapter for RecordingWebFetchAdapter {
    async fn fetch(&self, url: &str) -> Result<FetchResponse> {
        let mut calls = self.calls.lock().unwrap();
        calls.push(url.to_owned());
        Ok(FetchResponse {
            url: url.to_owned(),
            status_code: 200,
            text: format!("stub fetch for {url}"),
        })
    }
}

/// Recording stub for [`BrowserAdapter`].
#[derive(Debug, Default, Clone)]
pub struct RecordingBrowserAdapter {
    pub calls: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl BrowserAdapter for RecordingBrowserAdapter {
    async fn open(&self, url: &str) -> Result<BrowserPage> {
        let mut calls = self.calls.lock().unwrap();
        calls.push(url.to_owned());
        Ok(BrowserPage {
            url: url.to_owned(),
            title: format!("Stub browser page for {url}"),
        })
    }
}

// ---------------------------------------------------------------------------
// AdapterBundle
// ---------------------------------------------------------------------------

/// Holds one instance of each adapter behind a trait object.
///
/// `Default` wires up recording stubs, exactly like the Python
/// `AdapterBundle` dataclass.
pub struct AdapterBundle {
    pub cron: Box<dyn CronAdapter>,
    pub message: Box<dyn MessageAdapter>,
    pub memory: Box<dyn MemoryAdapter>,
    pub sessions: Box<dyn SessionsAdapter>,
    pub web_fetch: Box<dyn WebFetchAdapter>,
    pub browser: Box<dyn BrowserAdapter>,
}

impl Default for AdapterBundle {
    fn default() -> Self {
        Self {
            cron: Box::new(RecordingCronAdapter::default()),
            message: Box::new(RecordingMessageAdapter::default()),
            memory: Box::new(RecordingMemoryAdapter::default()),
            sessions: Box::new(RecordingSessionsAdapter::default()),
            web_fetch: Box::new(RecordingWebFetchAdapter::default()),
            browser: Box::new(RecordingBrowserAdapter::default()),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn recording_message_adapter_captures_calls() {
        let adapter = RecordingMessageAdapter::default();
        let result = adapter.notify("ch1", "subject", "body").await.unwrap();
        assert!(result.starts_with("message-"));
        assert_eq!(adapter.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn adapter_bundle_default_uses_recording_stubs() {
        let bundle = AdapterBundle::default();
        let r = bundle.message.notify("ch", "s", "b").await.unwrap();
        assert!(r.starts_with("message-"));
    }

    #[tokio::test]
    async fn recording_web_fetch_returns_stub_response() {
        let adapter = RecordingWebFetchAdapter::default();
        let resp = adapter.fetch("https://example.com").await.unwrap();
        assert_eq!(resp.status_code, 200);
        assert!(resp.text.contains("example.com"));
    }

    #[tokio::test]
    async fn recording_cron_adapter_captures_calls() {
        let adapter = RecordingCronAdapter::default();
        let r1 = adapter.schedule_resume("run-1", 0, "waiting").await.unwrap();
        let r2 = adapter.schedule_resume("run-1", 1, "retry").await.unwrap();
        assert_eq!(r1, "cron-1");
        assert_eq!(r2, "cron-2");
        assert_eq!(adapter.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn recording_memory_adapter_captures_entries() {
        let adapter = RecordingMemoryAdapter::default();
        let r = adapter.append("ns", "some content").await.unwrap();
        assert_eq!(r, "memory-1");
        let entries = adapter.entries.lock().unwrap();
        assert_eq!(entries[0], ("ns".to_owned(), "some content".to_owned()));
    }

    #[tokio::test]
    async fn recording_sessions_adapter_captures_calls() {
        let adapter = RecordingSessionsAdapter::default();
        let cmd = vec!["python".to_owned(), "run.py".to_owned()];
        let r = adapter.spawn("worker", &cmd).await.unwrap();
        assert_eq!(r, "session-1");
    }

    #[tokio::test]
    async fn recording_browser_adapter_returns_stub_page() {
        let adapter = RecordingBrowserAdapter::default();
        let page = adapter.open("https://example.com").await.unwrap();
        assert_eq!(page.url, "https://example.com");
        assert!(page.title.contains("example.com"));
    }
}
