//! GC report for a recorded session. Shared by the HTTP API and `perfhawk gc`.

use perfhawk_analysis::gc::{report, GcConfig, GcReport};
use perfhawk_store::{SessionRecord, Store, StoreError};

/// `None` when the session has no GC data.
pub fn report_for_session(
    store: &Store,
    session: &SessionRecord,
) -> Result<Option<GcReport>, StoreError> {
    Ok(store
        .load_gc(&session.id)?
        .map(|d| report(&d, &GcConfig::default())))
}
