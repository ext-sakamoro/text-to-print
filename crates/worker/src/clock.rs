//! The current instant: the Workers runtime's `Date.now()` (milliseconds) on
//! wasm32, the system clock in host tests

use alice_ephemeris::time::Timestamp;

#[cfg(target_arch = "wasm32")]
pub(crate) fn now() -> Timestamp {
    let ms = i64::try_from(worker::Date::now().as_millis()).unwrap_or(i64::MAX);
    Timestamp::from_unix_millis(ms).unwrap_or(Timestamp::MAX)
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn now() -> Timestamp {
    Timestamp::from_system_time(std::time::SystemTime::now())
}
