//! Local receiver calibration. Each SDR stack owns its own thread-local profile.
//! No device reads, network calls or cross-cell locks occur on the burst path.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{OnceLock, RwLock};
use tetra_config::bluestation::rssi_calibration::RssiCalibration;

thread_local! {
    static ACTIVE: RefCell<Option<RssiCalibration>> = const { RefCell::new(None) };
}

pub fn configure(profile: Option<RssiCalibration>) {
    ACTIVE.with(|p| *p.borrow_mut() = profile.filter(|p| p.validate().is_ok()));
}

pub fn calibrated_dbm(dbfs: f32) -> Option<f32> {
    ACTIVE.with(|p| p.borrow().as_ref()?.convert(dbfs))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_calibration_means_no_invented_dbm() {
        configure(None);
        assert_eq!(calibrated_dbm(-34.0), None);
    }
}

// Written only during SDR setup; queried by authenticated dashboard requests.
static RECEIVERS: OnceLock<RwLock<BTreeMap<u8, serde_json::Value>>> = OnceLock::new();

pub fn report_receiver(settings: serde_json::Value) {
    let cell = crate::cell_context::current().0;
    if let Ok(mut receivers) = RECEIVERS.get_or_init(Default::default).write() {
        receivers.insert(cell, settings);
    }
}

pub fn receiver_snapshot() -> serde_json::Value {
    let receivers = RECEIVERS.get_or_init(Default::default).read().unwrap();
    serde_json::json!({"receivers": receivers.iter().map(|(cell, settings)|
        serde_json::json!({"cell": cell, "settings": settings})).collect::<Vec<_>>()})
}

pub fn clear_receiver() {
    configure(None);
    if let Ok(mut receivers) = RECEIVERS.get_or_init(Default::default).write() {
        receivers.remove(&crate::cell_context::current().0);
    }
}

#[cfg(test)]
mod isolation_tests {
    use super::*;

    #[test]
    fn receiver_calibration_is_local_to_the_sdr_thread_and_can_be_cleared() {
        let profile = serde_json::from_value(serde_json::json!({
            "receiver":"Lab RX", "device":"driver=sx", "rx_antenna":"RX", "rx_channel":0,
            "rx_frequency_hz":430575000.0, "rx_center_frequency_hz":430575000.0,
            "ppm_err":0.0, "sample_rate":600000.0, "rx_gains":{"LNA":42.0,"PGA":16.0},
            "points":[{"dbfs":-70.0,"dbm":-110.0},{"dbfs":-30.0,"dbm":-70.0}]
        }))
        .unwrap();
        configure(Some(profile));
        assert_eq!(calibrated_dbm(-50.0), Some(-90.0));
        std::thread::spawn(|| assert_eq!(calibrated_dbm(-50.0), None)).join().unwrap();
        configure(None);
        assert_eq!(calibrated_dbm(-50.0), None);
    }
}
