use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the current timestamp in microseconds since the Unix epoch.
///
/// # Panics
///
/// Panics if the system time is before the Unix epoch.
#[inline]
#[must_use]
pub fn timestamp_us() -> u64 {
	SystemTime::now().duration_since(UNIX_EPOCH).expect("Time went backwards").as_micros().try_into().expect("Timestamp overflow")
}

