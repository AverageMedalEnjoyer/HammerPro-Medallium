#[cfg(windows)]
pub fn on_battery() -> bool {
    use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut status = SYSTEM_POWER_STATUS {
        ACLineStatus: 0,
        BatteryFlag: 0,
        BatteryLifePercent: 0,
        SystemStatusFlag: 0,
        BatteryLifeTime: 0,
        BatteryFullLifeTime: 0,
    };
    let ok = unsafe { GetSystemPowerStatus(&raw mut status) };
    ok != 0 && status.ACLineStatus == 0
}

#[cfg(not(windows))]
pub fn on_battery() -> bool {
    false
}

#[cfg(test)]
mod tests {
    #[test]
    fn power_state_is_readable() {
        let _ = super::on_battery();
    }
}
