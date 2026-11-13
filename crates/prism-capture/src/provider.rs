use crate::cancel::CancelToken;
use crate::system::{SystemProvider, NO_SYSTEM_PROVIDER_NOTE, SYSTEM_KINDS};
use crate::types::*;
use prism_model::Fidelity;

/// A runtime-specific way of capturing profiles.
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;

    /// `Ok(None)` if the target is not this provider's runtime. An
    /// `Err(NotDetected(hint))` is collected as a hint and the next provider
    /// is tried.
    fn detect(&self, target: &Target) -> Result<Option<RuntimeInfo>, CaptureError>;

    fn capabilities(&self, rt: &RuntimeInfo) -> Vec<Capability>;

    fn capture(
        &self,
        rt: &RuntimeInfo,
        target: &Target,
        plan: &CapturePlan,
        cancel: &CancelToken,
        progress: &dyn Fn(&str),
    ) -> Result<CaptureOutcome, CaptureError>;
}

#[derive(Default)]
pub struct ProviderRegistry {
    providers: Vec<Box<dyn Provider>>,
    system: Vec<Box<dyn SystemProvider>>,
}

impl ProviderRegistry {
    /// Go and .NET providers (added as they are implemented).
    pub fn with_defaults() -> Self {
        let mut r = ProviderRegistry::default();
        r.register(Box::new(crate::go::GoProvider::new()));
        r.register(Box::new(crate::dotnet::DotnetProvider::new()));
        r
    }

    pub fn register(&mut self, p: Box<dyn Provider>) {
        self.providers.push(p);
    }

    pub fn register_system(&mut self, p: Box<dyn SystemProvider>) {
        self.system.push(p);
    }

    /// Capabilities for [`SYSTEM_KINDS`]: the first provider that has a kind available
    /// wins, else the first provider's answer, else an "unavailable" note.
    pub fn system_capabilities(&self, rt: &RuntimeInfo) -> Vec<Capability> {
        let all: Vec<Vec<Capability>> = self.system.iter().map(|p| p.capabilities(rt)).collect();
        SYSTEM_KINDS
            .iter()
            .map(|k| {
                let mut found = all.iter().flatten().filter(|c| c.kind == *k);
                let first = found.next().cloned();
                match first {
                    Some(c) if c.available => c,
                    Some(c) => found.find(|c| c.available).cloned().unwrap_or(c),
                    None => Capability {
                        kind: *k,
                        fidelity: Fidelity::Unknown,
                        available: false,
                        note: Some(NO_SYSTEM_PROVIDER_NOTE.to_string()),
                    },
                }
            })
            .collect()
    }

    /// The first system provider that has every one of `kinds` available.
    pub fn system_provider_for(
        &self,
        rt: &RuntimeInfo,
        kinds: &[prism_model::ProfileKind],
    ) -> Option<&dyn SystemProvider> {
        self.system
            .iter()
            .find(|p| {
                let caps = p.capabilities(rt);
                kinds
                    .iter()
                    .all(|k| caps.iter().any(|c| c.kind == *k && c.available))
            })
            .map(|p| p.as_ref())
    }

    /// First provider (in registration order) that recognises the target.
    pub fn detect(&self, target: &Target) -> Result<(&dyn Provider, RuntimeInfo), CaptureError> {
        let mut hints = Vec::new();
        for p in &self.providers {
            match p.detect(target) {
                Ok(Some(rt)) => return Ok((p.as_ref(), rt)),
                Ok(None) => {}
                Err(CaptureError::NotDetected(h)) => hints.push(h),
                Err(e) => return Err(e),
            }
        }
        let tried: Vec<&str> = self.providers.iter().map(|p| p.id()).collect();
        let mut msg = format!(
            "no supported runtime detected for the target (tried: {})",
            if tried.is_empty() {
                "none".to_string()
            } else {
                tried.join(", ")
            }
        );
        for h in hints {
            msg.push_str(&format!("\nhint: {h}"));
        }
        Err(CaptureError::NotDetected(msg))
    }
}
