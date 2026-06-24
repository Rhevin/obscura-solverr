pub mod page;
pub mod context;
pub mod lifecycle;
pub mod profiles;

pub use page::{NetworkEvent, Page, PageError};
pub use context::BrowserContext;
pub use lifecycle::{LifecycleState, WaitUntil};
pub use obscura_js::HTML_TO_MARKDOWN_JS;
// Re-exported so the embeddable `obscura` crate (which depends on obscura-browser,
// not obscura-js) can surface the interception channel types.
pub use obscura_js::ops::{InterceptResolution, InterceptedRequest};
