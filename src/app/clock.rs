//! The local wall-clock time, for the log's stamps.
//!
//! Local, not UTC: a line is read against the clock on the wall. Asked of the
//! operating system -- `localtime_r` on Unix, `GetLocalTime` on Windows --
//! which knows the time zone and its daylight saving time; the standard
//! library does not, and no date crate is linked for it.

/// A moment on the local clock, to the millisecond.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LocalTime {
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millisecond: u32,
}

impl LocalTime {
    /// `17:42:10.123`: what each log line carries.
    pub fn time(&self) -> String {
        format!(
            "{:02}:{:02}:{:02}.{:03}",
            self.hour, self.minute, self.second, self.millisecond
        )
    }
}

#[cfg(unix)]
pub fn now() -> LocalTime {
    // One reading for both: the seconds go through the time zone, the
    // milliseconds are the same in any.
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    // SAFETY: `localtime_r` writes the `tm` it is given and nothing else.
    unsafe {
        let t = since.as_secs() as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&t, &mut tm).is_null() {
            return utc_from_unix(since);
        }
        LocalTime {
            hour: tm.tm_hour as u32,
            minute: tm.tm_min as u32,
            second: tm.tm_sec as u32,
            millisecond: since.subsec_millis(),
        }
    }
}

#[cfg(windows)]
pub fn now() -> LocalTime {
    // SAFETY: `GetLocalTime` fills the struct it is given.
    let st = unsafe {
        let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = std::mem::zeroed();
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut st);
        st
    };
    LocalTime {
        hour: st.wHour as u32,
        minute: st.wMinute as u32,
        second: st.wSecond as u32,
        millisecond: st.wMilliseconds as u32,
    }
}

/// UTC, where the local time cannot be had.
#[cfg(not(any(unix, windows)))]
pub fn now() -> LocalTime {
    utc_from_unix(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default(),
    )
}

/// The UTC time of day, from the time since 1970.
#[cfg(any(not(windows), test))]
fn utc_from_unix(since: std::time::Duration) -> LocalTime {
    let rem = since.as_secs() % 86_400;
    LocalTime {
        hour: (rem / 3600) as u32,
        minute: (rem % 3600 / 60) as u32,
        second: (rem % 60) as u32,
        millisecond: since.subsec_millis(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn stamps_are_zero_padded() {
        let t = LocalTime { hour: 7, minute: 4, second: 3, millisecond: 5 };
        assert_eq!(t.time(), "07:04:03.005");
    }

    #[test]
    fn unix_time_becomes_the_utc_time_of_day() {
        assert_eq!(utc_from_unix(Duration::ZERO).time(), "00:00:00.000");
        // The v0.5.9 tag, 2026-09-24 16:35:01 UTC, and a quarter second.
        assert_eq!(
            utc_from_unix(Duration::from_millis(1_790_267_701_250)).time(),
            "16:35:01.250"
        );
    }

    #[test]
    fn now_is_a_time_of_day() {
        let t = now();
        assert!(
            t.hour < 24 && t.minute < 60 && t.second < 61 && t.millisecond < 1000,
            "{t:?}"
        );
    }
}
