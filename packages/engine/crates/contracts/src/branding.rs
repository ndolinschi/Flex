//! The single place in the entire codebase where the product name may appear.
//!
//! Everything that needs the brand imports it from here: config directory
//! names, environment variable prefixes, user agents, server identifiers.
//! A product rename touches this file, the runner's `[[bin]]` name, the
//! workspace repository URL, and README — nothing else. CI greps for leaks.

/// Human-facing product name.
pub const PRODUCT_NAME: &str = "Flex";

/// Machine-facing slug: config directories (`~/.flex/`, project
/// `.flex/`), file names, identifiers.
pub const PRODUCT_SLUG: &str = "flex";

/// Prefix for environment variables (`FLEX_LOG`, `FLEX_AGENT`, ...).
pub const ENV_PREFIX: &str = "FLEX";

/// User-Agent header value for outbound HTTP requests.
pub const USER_AGENT: &str = concat!("flex/", env!("CARGO_PKG_VERSION"));

/// Name under which the engine registers when exposing an MCP server.
pub const MCP_SERVER_NAME: &str = "flex";

/// Engine version (workspace-wide).
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
