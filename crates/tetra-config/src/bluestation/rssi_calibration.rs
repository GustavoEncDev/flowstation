//! Measured RF-input references for one receiver and one fixed acquisition setup.
//!
//! A terminal's downlink signal meter is not an uplink reference. Use a known RF input
//! at the station receiver connector and measure through the normal burst/filter path.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RssiReference {
    pub dbfs: f32,
    pub dbm: f32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RssiCalibration {
    pub receiver: String,
    pub device: String,
    pub rx_antenna: String,
    pub rx_channel: usize,
    pub rx_frequency_hz: f64,
    pub sample_rate: f64,
    pub rx_center_frequency_hz: f64,
    pub ppm_err: f64,
    /// Complete gain-stage readback, not just optional overrides from the TOML.
    pub rx_gains: BTreeMap<String, f64>,
    /// Ascending measured points. No extrapolation outside the validated range.
    pub points: Vec<RssiReference>,
}

impl RssiCalibration {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.receiver.trim().is_empty()
            || self.receiver.len() > 128
            || self.device.trim().is_empty()
            || self.device.len() > 512
            || self.rx_antenna.len() > 64
            || self.rx_channel > 64
            || self.receiver.chars().any(char::is_control)
            || self.device.chars().any(char::is_control)
            || self.rx_antenna.chars().any(char::is_control)
        {
            return Err("invalid RSSI receiver identity");
        }
        if !self.rx_frequency_hz.is_finite()
            || !(1e6..=20e9).contains(&self.rx_frequency_hz)
            || !self.rx_center_frequency_hz.is_finite()
            || !(1e6..=20e9).contains(&self.rx_center_frequency_hz)
            || !self.ppm_err.is_finite()
            || self.ppm_err.abs() > 1000.0
            || !self.sample_rate.is_finite()
            || !(1e3..=100e6).contains(&self.sample_rate)
            || self.rx_gains.is_empty()
            || self.rx_gains.len() > 16
            || self.rx_gains.iter().any(|(k, v)| {
                k.is_empty() || k.len() > 64 || k.chars().any(char::is_control) || !v.is_finite() || !(-100.0..=150.0).contains(v)
            })
        {
            return Err("invalid RSSI acquisition settings");
        }
        if !(2..=64).contains(&self.points.len())
            || self.points.iter().any(|p| {
                !p.dbfs.is_finite() || !p.dbm.is_finite() || !(-180.0..=-3.0).contains(&p.dbfs) || !(-180.0..=10.0).contains(&p.dbm)
            })
        {
            return Err("provide 2 to 64 finite, unsaturated RSSI references");
        }
        if self.points.last().unwrap().dbfs - self.points[0].dbfs < 10.0 {
            return Err("RSSI references must cover at least 10 dB");
        }
        for pair in self.points.windows(2) {
            let digital_step = pair[1].dbfs - pair[0].dbfs;
            let input_step = pair[1].dbm - pair[0].dbm;
            if digital_step < 1.0 || !(0.75..=1.25).contains(&(input_step / digital_step)) {
                return Err("RSSI references are not ordered or outside the receiver linear range");
            }
        }
        Ok(())
    }

    /// Interpolate only within measured, linear, unclipped reference points.
    pub fn convert(&self, dbfs: f32) -> Option<f32> {
        if !dbfs.is_finite() || self.validate().is_err() || dbfs < self.points.first()?.dbfs || dbfs > self.points.last()?.dbfs {
            return None;
        }
        let pair = self.points.windows(2).find(|p| dbfs <= p[1].dbfs)?;
        let ratio = (dbfs - pair[0].dbfs) / (pair[1].dbfs - pair[0].dbfs);
        Some(pair[0].dbm + ratio * (pair[1].dbm - pair[0].dbm))
    }

    /// The gain correction is deliberately not guessed when the operator changes settings.
    /// Gain controls can be quantized or non-linear; a new measured profile is required.
    pub fn matches_acquisition(
        &self,
        device: &str,
        antenna: &str,
        channel: usize,
        frequency_hz: f64,
        sample_rate: f64,
        gains: &[(String, f64)],
    ) -> bool {
        self.validate().is_ok()
            && self.device == device
            && self.rx_antenna == antenna
            && self.rx_channel == channel
            && frequency_hz.is_finite()
            && sample_rate.is_finite()
            && (self.rx_frequency_hz - frequency_hz).abs() < 1.0
            && (self.sample_rate - sample_rate).abs() < 1.0
            && self.rx_gains.len() == gains.len()
            && gains.iter().all(|(name, gain)| {
                gain.is_finite()
                    && self
                        .rx_gains
                        .iter()
                        .find(|(key, _)| key.eq_ignore_ascii_case(name))
                        .is_some_and(|(_, reference)| (reference - gain).abs() < 0.05)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> RssiCalibration {
        RssiCalibration {
            receiver: "Lab RX 1".into(),
            device: "driver=sx".into(),
            rx_antenna: "RX".into(),
            rx_channel: 0,
            rx_frequency_hz: 430575000.0,
            sample_rate: 600000.0,
            rx_center_frequency_hz: 430575000.0,
            ppm_err: 0.0,
            rx_gains: BTreeMap::from([("LNA".into(), 42.0), ("PGA".into(), 16.0)]),
            points: vec![
                RssiReference { dbfs: -70.0, dbm: -110.0 },
                RssiReference { dbfs: -30.0, dbm: -70.0 },
            ],
        }
    }

    #[test]
    fn interpolates_and_never_extrapolates() {
        let p = profile();
        assert_eq!(p.convert(-50.0), Some(-90.0));
        assert_eq!(p.convert(-70.0), Some(-110.0));
        assert_eq!(p.convert(-30.0), Some(-70.0));
        for value in [-71.0, -29.0, f32::NAN, f32::INFINITY] {
            assert_eq!(p.convert(value), None);
        }
    }

    #[test]
    fn rejects_saturation_reversed_points_and_inconsistent_measurements() {
        let mut p = profile();
        p.points[1].dbm = -120.0;
        assert!(p.validate().is_err());
        p.points[1].dbm = -40.0;
        assert!(p.validate().is_err());
        p.points[1].dbfs = -1.0;
        assert!(p.validate().is_err());
    }

    #[test]
    fn binds_to_readback_gains_frequency_antenna_channel_and_sample_rate() {
        let p = profile();
        let mut gains = vec![("lna".into(), 42.0), ("pga".into(), 16.0)];
        assert!(p.matches_acquisition("driver=sx", "RX", 0, 430575000.0, 600000.0, &gains));
        gains[0].1 = 36.0;
        assert!(!p.matches_acquisition("driver=sx", "RX", 0, 430575000.0, 600000.0, &gains));
        gains[0].1 = 42.0;
        for (device, antenna, ch, freq, fs) in [
            ("driver=mucell", "RX", 0, 430575000.0, 600000.0),
            ("driver=sx", "RX2", 0, 430575000.0, 600000.0),
            ("driver=sx", "RX", 1, 430575000.0, 600000.0),
            ("driver=sx", "RX", 0, 430600000.0, 600000.0),
            ("driver=sx", "RX", 0, 430575000.0, 300000.0),
        ] {
            assert!(!p.matches_acquisition(device, antenna, ch, freq, fs, &gains));
        }
    }
}
