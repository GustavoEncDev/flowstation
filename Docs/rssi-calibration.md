# Receiver RSSI in dBm

The station can convert its uplink burst level locally into calibrated RF input power,
show it in the registered-terminals table, and send the same value to the Core over Brew.
No extra service or periodic SDR access is required on the burst path.

## What is measured

The demodulator calculates the mean squared magnitude over the received burst window:

```text
L_dBFS = 10 log10(mean(|x[n]|²))
```

The existing samples include the receiver gain and the digital receive/filter chain.
They describe a digital level relative to full scale, rather than an absolute RF power.
A measured reference supplies the missing scale. Within a linear segment, the calculation is:

```text
P_dBm = P0_dBm + (L_dBFS - L0_dBFS) × (P1_dBm - P0_dBm) / (L1_dBFS - L0_dBFS)
```

For a linear receiver with one dB of digital change per dB of RF-input change, this reduces
to `P_dBm = L_dBFS + offset`. There is no universal offset for every SDR, gain setting,
filter path, antenna port or external preamplifier. RSSI includes noise and other energy
inside the measurement window; it is not an independent SNR or BER measurement.

A handheld radio's signal meter measures the downlink at that handheld. It cannot calibrate
an uplink measurement at the station. Similarly, transmitter power and distance alone do
not establish received power at the station connector.

## Measure and configure

1. Use a known RF input at the receiver connector, with the same antenna port, gains,
   uplink frequency, RX center frequency, PPM correction, sample rate and normal receive
   filtering as the operating station. Include external cable/attenuator losses in the
   reference power at that connector. Use a suitable TETRA test signal through the normal
   burst measurement path; a different signal bandwidth can give a different result.
2. Record at least two pairs of station dBFS and known input dBm spanning at least 10 dB.
   Keep well above the noise floor and below compression/clipping. A sequence of levels
   should produce approximately one dB of digital change per dB of RF-input change.
3. In **Config → RF → RSSI calibration**, select **Read receiver settings**. This reads
   cached startup metadata, not the SDR itself. Enter a unique receiver name and the
   measured `points` in the JSON profile. Reading settings clears previous reference
   points so they cannot silently be reused after a gain change.
4. Save and apply the configuration. At SDR startup the profile is checked against the
   actual gain and antenna readbacks. Invalid or mismatched calibration falls back to
   explicitly labelled dBFS. A secondary carrier does not inherit the primary-frequency
   calibration. Additional cells have their own receiver profiles and thread-local state.
5. Enable the existing **Brew → RSSI export** option to send the values to the Core.

An optional TOML profile can also be added under `[phy_io.soapysdr.rssi_calibration]`
(or `[cells.soapysdr.rssi_calibration]` for an additional cell). The following values are
**synthetic examples for tests**, not a factory calibration for SXceiver:

```toml
[phy_io.soapysdr.rssi_calibration]
receiver = "Lab RX 1"
device = "driver=sx"
rx_antenna = "RX"
rx_channel = 0
rx_frequency_hz = 430575000.0
rx_center_frequency_hz = 430575000.0
ppm_err = 0.0
sample_rate = 600000.0
rx_gains = { LNA = 42.0, PGA = 16.0 }
points = [{ dbfs = -70.0, dbm = -110.0 }, { dbfs = -30.0, dbm = -70.0 }]
```

Up to 64 ordered points are accepted. Conversion only interpolates inside the measured
range. Non-finite values, clipped references, reversed points and inconsistent slopes are
rejected. Without a matching profile, or outside its measured range, no dBm is invented.
Recalibrate after changing receiver hardware, the RF input chain or acquisition settings.
Accuracy is determined by the reference instrument, measurement method and receiver drift;
the software does not claim a particular absolute accuracy.

The **Read receiver settings** endpoint is authenticated and returns only cached RF
acquisition metadata. It never returns credentials or opens/reconfigures the SDR.

## Brew compatibility

The existing service header `0xf4 0x10` and NUL termination are preserved:

```json
{"issi":2683283,"rssi_dbfs":-42.3,"rssi_dbm":-92.3}
```

`rssi_dbm` is optional. Uncalibrated stations keep the original payload shape and original
dBFS field. The receiving Core must support the additional JSON field. Calibrated values
are sent with the existing per-subscriber five-second rate limit and export opt-in.
No new mandatory protocol message is introduced.

## Hardware notes

The [SXceiver specifications](https://sxceiver.com/doc/specs) describe its SX1255 I/Q ADC
and decimation chain. The [SoapySX driver](https://github.com/tejeez/sxxcvr) exposes
programmable LNA/PGA gains but does not currently expose an absolute RSSI sensor.
This implementation therefore uses measured receiver references rather than assuming
that a gain-control number is a factory-calibrated RF power scale.

Implementation by Gustavo Encarnação — CS7ATL.
