#![cfg(target_family = "windows")]

use std::ffi::{OsStr, OsString};
use std::ops::{Add, Sub};
use std::os::windows::ffi::OsStringExt;
use std::time::Duration;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::timezoneapi::{
    GetDynamicTimeZoneInformation, DYNAMIC_TIME_ZONE_INFORMATION, PDYNAMIC_TIME_ZONE_INFORMATION,
    TIME_ZONE_ID_INVALID,
};

pub fn get_convert_utc_to_local() -> impl Fn(Duration) -> Duration {
    let mut total_bias;
    //Get Bias
    {
        let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
        let r =
            unsafe { GetDynamicTimeZoneInformation(&mut info as PDYNAMIC_TIME_ZONE_INFORMATION) };
        if r == TIME_ZONE_ID_INVALID {
            eprintln!("Error Occurred whilst getting information on UTC to timezone conversion information.\n\
			System error code is:{}. For meaning of the code go here: https://docs.microsoft.com/en-us/windows/win32/debug/system-error-codes",unsafe{GetLastError()});
            eprintln!("Will continue, but the time will now be in UTC instead of the local time.");
            info = DYNAMIC_TIME_ZONE_INFORMATION::default();
        }

        //We should have a bias here in every case, because even if the syscall fails, we reinitialise info.
        total_bias = info.Bias;
        match r {
            1 => total_bias += info.StandardBias, //No DST
            2 => total_bias += info.DaylightBias, //DST
            _ => {}
        }
    }
    //Make a fn, to apply bias
    let op = if total_bias < 0 {
        total_bias *= -1;
        Duration::add
    } else {
        Duration::sub
    };
    let total_bias = total_bias as u64;
    //we now know, that total_bias>=0. So we can create a Duration.
    let correction = Duration::from_secs(total_bias * 60);
    move |x| op(x, correction)
}
