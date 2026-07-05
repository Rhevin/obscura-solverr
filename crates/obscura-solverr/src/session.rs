use std::collections::HashMap;
use std::sync::Arc;

use obscura_browser::{BrowserContext, Page};
use uuid::Uuid;

pub struct BrowserSession {
    pub context: Arc<BrowserContext>,
    pub page: Page,
    pub requests: u32,
}

pub struct SessionStore {
    sessions: HashMap<String, BrowserSession>,
    max_requests_per_session: u32,
}

impl SessionStore {
    pub fn new(max_requests_per_session: u32) -> Self {
        Self {
            sessions: HashMap::new(),
            max_requests_per_session,
        }
    }

    pub fn create(
        &mut self,
        proxy: Option<String>,
        stealth: bool,
        user_agent: Option<String>,
    ) -> String {
        let id = Uuid::new_v4().to_string();
        let context = Arc::new(BrowserContext::with_storage_full(
            format!("solverr-{id}"),
            proxy,
            stealth,
            user_agent,
            None,
        ));
        let page = Page::new(format!("solverr-page-{id}"), context.clone());
        self.sessions.insert(
            id.clone(),
            BrowserSession {
                context,
                page,
                requests: 0,
            },
        );
        id
    }

    pub fn destroy(&mut self, session_id: &str) -> bool {
        self.sessions.remove(session_id).is_some()
    }

    pub fn list(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    pub fn get_mut(&mut self, session_id: &str) -> Option<&mut BrowserSession> {
        self.sessions.get_mut(session_id)
    }

    pub fn touch(&mut self, session_id: &str) -> Result<(), String> {
        let session = self
            .sessions
            .get_mut(session_id)
            .ok_or_else(|| format!("Session not found: {session_id}"))?;
        if session.requests >= self.max_requests_per_session {
            return Err(format!(
                "Session {session_id} exceeded max requests ({})",
                self.max_requests_per_session
            ));
        }
        session.requests += 1;
        Ok(())
    }
}
