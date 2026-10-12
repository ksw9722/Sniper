//! Default-Off, session-bound settings for passively observed socket metadata.
//! This module never opens a connection, resolves a host, or starts a worker.
use crate::{runtime::RuntimeSettingsSnapshot, session::SessionContext};
use rsa::sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use uuid::Uuid;

pub(crate) type DurabilityGates = Mutex<std::collections::HashMap<Uuid, Arc<AtomicBool>>>;

#[derive(Default)]
pub(crate) struct SessionPeerSettingsState {
    // AppState binds all contexts for one session to the same gate. Unrelated
    // saves and context replacement must not silently confirm this setting.
    gate: Mutex<Arc<AtomicBool>>,
}

impl SessionPeerSettingsState {
    pub(crate) fn bind(&self, shared: Arc<AtomicBool>) {
        let mut current = self.gate.lock().unwrap();
        if current.load(Ordering::Acquire) {
            shared.store(true, Ordering::Release);
        }
        *current = shared;
    }

    pub(crate) fn uncertain(&self) -> bool {
        self.gate.lock().unwrap().load(Ordering::Acquire)
    }

    pub(crate) fn mark_uncertain(&self, uncertain: bool) {
        self.gate
            .lock()
            .unwrap()
            .store(uncertain, Ordering::Release);
    }
}

#[derive(Debug)]
pub(crate) enum SettingsWriteError {
    NotCommitted,
    CommittedButUncertain,
    OutcomeUnknown,
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) enum SettingsWriteFault {
    BeforeReplacement,
    AfterReplacement,
    OutcomeUnknown,
}

/// Opaque CAS value, separated from all other settings features. In particular,
/// invalid None revisions never recreate a prior zero or exhausted token.
pub fn config_token(session_id: Uuid, enabled: bool, revision: Option<u64>) -> String {
    let bytes = serde_json::to_vec(&(session_id, enabled, revision))
        .expect("upstream peer settings are serializable");
    let mut hash = Sha256::new();
    hash.update(b"sniper-upstream-peer-settings-v1\0");
    hash.update(bytes);
    format!("{:x}", hash.finalize())
}

pub(crate) fn valid_config_token(token: &str) -> bool {
    token.len() == 64
        && token
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpstreamPeerBlockedReason {
    PersistenceUncertain,
    InvalidConfiguration,
    HttpCapturePaused,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpstreamPeerSettingsStatus {
    pub effective_enabled: bool,
    pub blocked_reason: Option<UpstreamPeerBlockedReason>,
}

pub(crate) fn status(
    runtime: &RuntimeSettingsSnapshot,
    uncertain: bool,
) -> UpstreamPeerSettingsStatus {
    let blocked_reason = if uncertain {
        Some(UpstreamPeerBlockedReason::PersistenceUncertain)
    } else if runtime
        .upstream_peer_capture_revision
        .is_none_or(|revision| revision == u64::MAX)
    {
        // An exhausted On cannot accept a later Off revision. Fail closed
        // without resetting the revision or changing its existing CAS token.
        Some(UpstreamPeerBlockedReason::InvalidConfiguration)
    } else if runtime.upstream_peer_capture_enabled && !runtime.http_capture_enabled {
        Some(UpstreamPeerBlockedReason::HttpCapturePaused)
    } else {
        None
    };
    UpstreamPeerSettingsStatus {
        effective_enabled: runtime.upstream_peer_capture_enabled && blocked_reason.is_none(),
        blocked_reason,
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UpstreamPeerSettingsSnapshot {
    pub session_id: Uuid,
    pub enabled: bool,
    pub config_revision: String,
    pub config_token: String,
    pub status: UpstreamPeerSettingsStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed: Option<bool>,
}

pub async fn snapshot(
    session: &SessionContext,
    changed: Option<bool>,
) -> UpstreamPeerSettingsSnapshot {
    let runtime = session.runtime.snapshot().await;
    UpstreamPeerSettingsSnapshot {
        session_id: session.id(),
        enabled: runtime.upstream_peer_capture_enabled,
        config_revision: runtime
            .upstream_peer_capture_revision
            .unwrap_or(u64::MAX)
            .to_string(),
        config_token: config_token(
            session.id(),
            runtime.upstream_peer_capture_enabled,
            runtime.upstream_peer_capture_revision,
        ),
        status: status(&runtime, session.upstream_peer_settings.uncertain()),
        changed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_settings_tokens_bind_session_feature_value_and_revision() {
        let id = Uuid::new_v4();
        let token = config_token(id, false, Some(0));
        assert!(valid_config_token(&token));
        for other in [
            config_token(Uuid::new_v4(), false, Some(0)),
            config_token(id, true, Some(0)),
            config_token(id, false, Some(1)),
            config_token(id, false, None),
            config_token(id, false, Some(u64::MAX)),
            crate::history_retention::history_retention_config_token(id, &Default::default(), 0),
        ] {
            assert_ne!(token, other);
        }
        assert_ne!(
            config_token(id, false, None),
            config_token(id, false, Some(u64::MAX))
        );
    }

    #[test]
    fn peer_settings_status_fails_closed_and_gate_survives_context_replacement() {
        let mut runtime = RuntimeSettingsSnapshot::default();
        assert!(!status(&runtime, false).effective_enabled);
        runtime.upstream_peer_capture_enabled = true;
        assert!(status(&runtime, false).effective_enabled);
        runtime.http_capture_enabled = false;
        assert_eq!(
            status(&runtime, false).blocked_reason,
            Some(UpstreamPeerBlockedReason::HttpCapturePaused)
        );
        for revision in [Some(u64::MAX), None] {
            runtime.upstream_peer_capture_revision = revision;
            assert!(!status(&runtime, false).effective_enabled);
            assert_eq!(
                status(&runtime, false).blocked_reason,
                Some(UpstreamPeerBlockedReason::InvalidConfiguration)
            );
        }
        assert_eq!(
            status(&runtime, true).blocked_reason,
            Some(UpstreamPeerBlockedReason::PersistenceUncertain)
        );
        let gate = Arc::new(AtomicBool::new(false));
        let old = SessionPeerSettingsState::default();
        old.bind(gate.clone());
        old.mark_uncertain(true);
        let replacement = SessionPeerSettingsState::default();
        replacement.bind(gate);
        assert!(replacement.uncertain());
        replacement.mark_uncertain(false);
        assert!(!old.uncertain());
    }
}
