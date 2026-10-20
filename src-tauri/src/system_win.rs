// Copyright © 2026 Wayne Davies. Free software under the GNU General Public License, version 3 or later.
// See LICENSE for the full text.
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the PC is doing that decides whether a backup should wait, as system.rs does on macOS:
//! the battery, and whether the network is one Windows calls metered (a phone's hotspot, a
//! connection set as metered in Settings).

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

/// On battery power, its charge in percent; None on mains power or a PC without one.
pub fn battery_percent() -> Option<u32> {
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s) }.ok()?;
    // ACLineStatus 0 is on battery; BatteryLifePercent 255 is unknown; BatteryFlag 128 is no battery.
    (s.ACLineStatus == 0 && s.BatteryFlag != 128 && s.BatteryLifePercent <= 100).then_some(s.BatteryLifePercent as u32)
}

static EXPENSIVE: AtomicBool = AtomicBool::new(false);

/// Whether the internet connection is metered: Windows' cost of it is fixed or variable, or it's
/// roaming or over its data limit.
fn metered() -> bool {
    use windows::Networking::Connectivity::{NetworkCostType, NetworkInformation};
    let Ok(profile) = NetworkInformation::GetInternetConnectionProfile() else { return false };
    let Ok(cost) = profile.GetConnectionCost() else { return false };
    let kind = cost.NetworkCostType().unwrap_or(NetworkCostType::Unrestricted);
    matches!(kind, NetworkCostType::Fixed | NetworkCostType::Variable)
        || cost.Roaming().unwrap_or(false)
        || cost.OverDataLimit().unwrap_or(false)
}

/// Starts watching the network: asked again every half minute, in the background.
pub fn watch_network() {
    std::thread::spawn(|| loop {
        EXPENSIVE.store(metered(), Relaxed);
        std::thread::sleep(std::time::Duration::from_secs(30));
    });
}

pub fn on_expensive_network() -> bool {
    EXPENSIVE.load(Relaxed)
}

/// macOS stops apps started at login reading files that are only in the cloud until they ask;
/// Windows downloads such a file whenever it's read.
pub fn allow_cloud_downloads() {}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_battery_and_network() {
        let pct = super::battery_percent();
        assert!(pct.is_none_or(|p| p <= 100));
        println!("battery: {pct:?}; metered: {}", super::metered());
    }
}
