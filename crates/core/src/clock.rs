//! The current instant, read from the system clock (the one place the core reads it)

use alice_ephemeris::time::Timestamp;

/// Now, to the resolution of the platform's `SystemTime`
#[must_use]
pub fn now() -> Timestamp {
    Timestamp::from_system_time(std::time::SystemTime::now())
}
