//! Closed, numeric measurement wire for the opt-in example, never app IPC.
use serde::{de::{self, SeqAccess, Visitor}, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

pub const SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const LIMIT: u64 = 10_000_000;
pub const BIN_COUNT: usize = 257;

#[derive(Clone, Debug, PartialEq)]
pub struct Bins(pub [u64; BIN_COUNT]);
impl Serialize for Bins {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.as_slice().serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Bins {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Fixed;
        impl<'de> Visitor<'de> for Fixed {
            type Value = Bins;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str("exactly 257 bounded integer bins") }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Bins, A::Error> {
                let mut bins = [0; BIN_COUNT];
                for slot in &mut bins {
                    *slot = seq.next_element()?.ok_or_else(|| de::Error::custom("short histogram"))?;
                    if *slot > LIMIT { return Err(de::Error::custom("histogram limit")); }
                }
                if seq.next_element::<de::IgnoredAny>()?.is_some() { return Err(de::Error::custom("long histogram")); }
                Ok(Bins(bins))
            }
        }
        deserializer.deserialize_seq(Fixed)
    }
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Counters {
    pub created: u64, pub disposed: u64, pub raf: u64,
    pub submitted: u64, pub context_lost: u64, pub failures: u64,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Histograms { pub raf: Bins, pub submission: Bins, pub cpu: Bins }
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Maximums { pub raf: f64, pub submission: f64, pub cpu: f64 }
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Packet {
    pub version: u8, pub epoch: u64, pub sequence: u64,
    pub disposed: bool, pub telemetry_invalid: bool, pub dom_out: bool,
    pub counters: Counters, pub histograms: Histograms, pub max: Maximums,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Command { Continue, DomOut, DomIn, Dispose }
#[derive(Debug, Serialize)]
pub struct Reply { pub accepted: bool, pub epoch: u64, pub command: Command }

/// A timed DOM phase begins only after command delivery and a later report.
/// Restored in-view rendering additionally needs a submission after delivery.
pub struct PhaseGate {
    epoch: u64,
    command: Command,
    delivered: Option<(u64, u64)>,
    confirmed_in_view: Option<(u64, u64)>,
    last_sequence: u64,
    acknowledged: bool,
}
impl PhaseGate {
    pub fn new(epoch: u64, command: Command) -> Result<Self, &'static str> {
        if epoch == 0 || epoch > SAFE_INTEGER || !matches!(command, Command::DomIn | Command::DomOut) { return Err("invalid_phase_gate"); }
        Ok(Self { epoch, command, delivered: None, confirmed_in_view: None, last_sequence: 0, acknowledged: false })
    }
    pub fn acknowledged(&self) -> bool { self.acknowledged }
    /// Called only after the ordinary closed Protocol accepts this packet.
    pub fn observe(&mut self, packet: &Packet, reply: &Reply) -> Result<(), &'static str> {
        if packet.epoch != self.epoch || reply.epoch != self.epoch || !reply.accepted || reply.command != self.command
            || packet.sequence <= self.last_sequence || packet.disposed {
            return Err("stale_or_foreign_phase_report");
        }
        self.last_sequence = packet.sequence;
        let Some((issued_sequence, _issued_submitted)) = self.delivered else {
            self.delivered = Some((packet.sequence, packet.counters.submitted));
            return Ok(());
        };
        if packet.sequence <= issued_sequence { return Err("stale_or_foreign_phase_report"); }
        if self.command == Command::DomOut && packet.dom_out {
            self.acknowledged = true;
        } else if self.command == Command::DomIn {
            if packet.dom_out { self.confirmed_in_view = None; return Ok(()); }
            // Counts accrued while the command reply was in transit do not
            // establish a draw after CSS restored the in-view host.
            if let Some((confirmed_sequence, submitted)) = self.confirmed_in_view {
                if packet.sequence > confirmed_sequence && packet.counters.submitted > submitted { self.acknowledged = true; }
            } else { self.confirmed_in_view = Some((packet.sequence, packet.counters.submitted)); }
        }
        Ok(())
    }
}

pub struct Protocol {
    pub epoch: u64,
    pub command: Command,
    pub previous: Option<Packet>,
    expected_dom_out: bool,
    dispose_sent: bool,
}
impl Protocol {
    pub fn new(epoch: u64) -> Result<Self, &'static str> {
        if epoch == 0 || epoch > SAFE_INTEGER { return Err("invalid_epoch"); }
        Ok(Self { epoch, command: Command::Continue, previous: None, expected_dom_out: false, dispose_sent: false })
    }
    pub fn accept(&mut self, packet: Packet) -> Result<Reply, &'static str> {
        let next = self.previous.as_ref().map_or(Some(1), |p| p.sequence.checked_add(1)).ok_or("sequence_limit")?;
        let c = &packet.counters;
        if packet.version != 1 || packet.epoch != self.epoch || packet.sequence != next || packet.sequence > LIMIT {
            return Err("stale_or_invalid_batch");
        }
        if packet.telemetry_invalid || c.created != 1 || c.disposed > 1 || c.context_lost != 0 || c.failures != 0
            || c.raf > LIMIT || c.submitted > LIMIT || packet.disposed != (c.disposed == 1)
            || (packet.disposed && !self.dispose_sent) || packet.dom_out != self.expected_dom_out {
            return Err("invalid_stage_telemetry");
        }
        for max in [packet.max.raf, packet.max.submission, packet.max.cpu] {
            if !max.is_finite() || !(0.0..=3_600_000.0).contains(&max) { return Err("invalid_clock_telemetry"); }
        }
        let total = |bins: &Bins| bins.0.iter().try_fold(0u64, |sum, value| {
            if *value > LIMIT { None } else { sum.checked_add(*value).filter(|v| *v <= LIMIT) }
        });
        if total(&packet.histograms.raf) != Some(c.raf.saturating_sub(1))
            || total(&packet.histograms.submission) != Some(c.submitted.saturating_sub(1))
            || total(&packet.histograms.cpu) != Some(c.submitted) {
            return Err("inconsistent_histograms");
        }
        if let Some(old) = &self.previous {
            let prior = &old.counters;
            if c.raf < prior.raf || c.submitted < prior.submitted || c.disposed < prior.disposed
                || packet.max.raf < old.max.raf || packet.max.submission < old.max.submission || packet.max.cpu < old.max.cpu {
                return Err("telemetry_regressed");
            }
            for (now, before) in [(&packet.histograms.raf, &old.histograms.raf), (&packet.histograms.submission, &old.histograms.submission), (&packet.histograms.cpu, &old.histograms.cpu)] {
                if now.0.iter().zip(before.0.iter()).any(|(a, b)| a < b) { return Err("telemetry_regressed"); }
            }
            if old.disposed { return Err("retired_stage_batch"); }
        }
        let final_ack = packet.disposed;
        self.previous = Some(packet);
        match self.command {
            Command::DomOut => self.expected_dom_out = true,
            Command::DomIn => self.expected_dom_out = false,
            Command::Dispose => self.dispose_sent = true,
            Command::Continue => (),
        }
        Ok(Reply { accepted: true, epoch: self.epoch, command: if final_ack { Command::Continue } else { self.command } })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet() -> Packet {
        let mut cpu = Bins([0; BIN_COUNT]); cpu.0[2] = 1;
        Packet { version: 1, epoch: 1, sequence: 1, disposed: false, telemetry_invalid: false, dom_out: false,
            counters: Counters { created: 1, disposed: 0, raf: 1, submitted: 1, context_lost: 0, failures: 0 },
            histograms: Histograms { raf: Bins([0; BIN_COUNT]), submission: Bins([0; BIN_COUNT]), cpu },
            max: Maximums { raf: 0.0, submission: 0.0, cpu: 2.0 } }
    }
    #[test]
    fn caller_epoch_sequence_and_closed_schema_are_required() {
        let p = packet(); let mut protocol = Protocol::new(1).unwrap();
        let mut wrong = p.clone(); wrong.epoch = 2; assert!(protocol.accept(wrong).is_err());
        assert!(protocol.previous.is_none()); protocol.accept(p.clone()).unwrap();
        assert!(protocol.accept(p.clone()).is_err());
        let mut value = serde_json::to_value(&p).unwrap(); value["path"] = serde_json::json!("not accepted");
        assert!(serde_json::from_value::<Packet>(value).is_err());
        assert!(Protocol::new(0).is_err()); assert!(Protocol::new(SAFE_INTEGER + 1).is_err());
    }
    #[test]
    fn histograms_are_fixed_bounded_and_consistent() {
        for size in [0, 256, 258, 4096] { assert!(serde_json::from_value::<Bins>(serde_json::json!(vec![0; size])).is_err()); }
        let mut p = packet(); p.histograms.raf.0[1] = 1;
        assert!(Protocol::new(1).unwrap().accept(p).is_err());
        let mut bad = serde_json::to_value(packet()).unwrap(); bad["histograms"]["cpu"][2] = serde_json::json!(LIMIT + 1);
        assert!(serde_json::from_value::<Packet>(bad).is_err());
    }
    #[test]
    fn disposal_requires_native_request_and_cannot_replay() {
        let mut p = packet(); p.disposed = true; p.counters.disposed = 1;
        assert!(Protocol::new(1).unwrap().accept(p.clone()).is_err());
        let mut protocol = Protocol::new(1).unwrap(); protocol.command = Command::Dispose;
        assert_eq!(protocol.accept(packet()).unwrap().command, Command::Dispose);
        p.sequence = 2; assert_eq!(protocol.accept(p.clone()).unwrap().command, Command::Continue); p.sequence = 3;
        assert!(protocol.accept(p).is_err());
    }
    #[test]
    fn native_dom_command_requires_a_later_matching_report() {
        let mut protocol = Protocol::new(1).unwrap(); protocol.command = Command::DomOut;
        protocol.accept(packet()).unwrap(); let mut p = packet(); p.sequence = 2;
        assert!(protocol.accept(p.clone()).is_err()); p.dom_out = true; protocol.accept(p.clone()).unwrap();
        protocol.command = Command::DomIn; p.sequence = 3; protocol.accept(p.clone()).unwrap();
        p.sequence = 4; p.dom_out = false; protocol.accept(p).unwrap();
    }
    #[test]
    fn failures_and_counter_or_bin_regression_invalidate_the_run() {
        for field in 0..3 {
            let mut p = packet(); match field { 0 => p.telemetry_invalid = true, 1 => p.counters.context_lost = 1, _ => p.counters.failures = 1 }
            assert!(Protocol::new(1).unwrap().accept(p).is_err());
        }
        let mut protocol = Protocol::new(1).unwrap(); protocol.accept(packet()).unwrap();
        let mut p = packet(); p.sequence = 2; p.counters.raf = 0;
        assert!(protocol.accept(p).is_err());
        let mut p = packet(); p.sequence = 2; p.max.cpu = f64::NAN;
        assert!(protocol.accept(p).is_err());
        assert_eq!(protocol.previous.as_ref().unwrap().sequence, 1);
    }
    #[test]
    fn a_phase_needs_delivery_then_a_later_matching_dom_report() {
        let mut gate = PhaseGate::new(1, Command::DomOut).unwrap();
        let reply = Reply { accepted: true, epoch: 1, command: Command::DomOut };
        let mut p = packet(); p.dom_out = true;
        gate.observe(&p, &reply).unwrap(); assert!(!gate.acknowledged());
        assert!(gate.observe(&p, &reply).is_err());
        p.sequence = 2; p.dom_out = false; gate.observe(&p, &reply).unwrap(); assert!(!gate.acknowledged());
        p.sequence = 3; p.dom_out = true; gate.observe(&p, &reply).unwrap(); assert!(gate.acknowledged());
    }
    #[test]
    fn in_view_phase_needs_a_fresh_submission_after_command_delivery() {
        let mut gate = PhaseGate::new(1, Command::DomIn).unwrap();
        let reply = Reply { accepted: true, epoch: 1, command: Command::DomIn };
        let mut p = packet(); gate.observe(&p, &reply).unwrap();
        p.sequence = 2; gate.observe(&p, &reply).unwrap(); assert!(!gate.acknowledged());
        p.sequence = 3; p.counters.submitted += 1; p.dom_out = true;
        gate.observe(&p, &reply).unwrap(); assert!(!gate.acknowledged());
        p.sequence = 4; p.dom_out = false; gate.observe(&p, &reply).unwrap(); assert!(!gate.acknowledged());
        p.sequence = 5; p.counters.submitted += 1; gate.observe(&p, &reply).unwrap(); assert!(gate.acknowledged());
    }
    #[test]
    fn frames_before_the_first_confirmed_in_view_report_cannot_qualify_restoration() {
        let mut gate = PhaseGate::new(1, Command::DomIn).unwrap();
        let reply = Reply { accepted: true, epoch: 1, command: Command::DomIn };
        let mut p = packet(); gate.observe(&p, &reply).unwrap();
        p.sequence = 2; p.counters.submitted += 10; gate.observe(&p, &reply).unwrap();
        assert!(!gate.acknowledged());
        p.sequence = 3; gate.observe(&p, &reply).unwrap(); assert!(!gate.acknowledged());
        p.sequence = 4; p.counters.submitted += 1; gate.observe(&p, &reply).unwrap(); assert!(gate.acknowledged());
    }
    #[test]
    fn phase_gate_rejects_foreign_epoch_commands_and_disposal() {
        for field in 0..4 {
            let mut gate = PhaseGate::new(1, Command::DomIn).unwrap();
            let mut p = packet(); let mut reply = Reply { accepted: true, epoch: 1, command: Command::DomIn };
            match field { 0 => p.epoch = 2, 1 => reply.epoch = 2, 2 => reply.command = Command::Dispose, _ => p.disposed = true }
            assert!(gate.observe(&p, &reply).is_err()); assert!(!gate.acknowledged());
        }
        assert!(PhaseGate::new(0, Command::DomIn).is_err());
        assert!(PhaseGate::new(1, Command::Continue).is_err());
    }
}
