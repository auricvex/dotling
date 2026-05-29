pub mod error;
pub mod fs;
pub mod path;
pub mod platform;
pub mod store;

/// Shared mutex for tests that mutate global state (HOME, env vars).
/// All HOME-sensitive tests must acquire this lock to avoid parallel interference.
#[cfg(test)]
#[allow(clippy::disallowed_types)]
pub static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
