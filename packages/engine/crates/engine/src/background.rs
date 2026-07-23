use agentloop_contracts::SessionId;
use agentloop_core::BackgroundEntrySummary;

use crate::EngineResult;
use crate::service::EngineService;

impl EngineService {
    pub async fn shutdown(&self) {
        if let Some(registry) = &self.background_processes {
            registry.kill_all().await;
        }
    }

    pub fn background_list(&self, session: &SessionId) -> Vec<BackgroundEntrySummary> {
        match &self.background_processes {
            Some(registry) => registry.list(session),
            None => Vec::new(),
        }
    }

    pub async fn background_kill(&self, session: &SessionId, id: &str) -> EngineResult<bool> {
        match &self.background_processes {
            Some(registry) => Ok(registry.kill(session, id).await?),
            None => Ok(false),
        }
    }

    pub fn background_demote(&self, session: &SessionId, id: &str) -> bool {
        match &self.demote_processes {
            Some(registry) => registry.request_demote(session, id),
            None => false,
        }
    }
}
