use crate::cancel::CancelToken;
use crate::types::*;

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
}

impl ProviderRegistry {
    /// Go and .NET providers (added as they are implemented).
    pub fn with_defaults() -> Self {
        let mut r = ProviderRegistry::default();
        r.register(Box::new(crate::go::GoProvider::new()));
        r
    }

    pub fn register(&mut self, p: Box<dyn Provider>) {
        self.providers.push(p);
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
