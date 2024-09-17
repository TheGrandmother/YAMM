#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unreachable_code)]

use cortex_m::prelude::*;
use rp_pico::hal;

use hal::gpio::bank0::*;
use hal::gpio::{Pin, PinId, PinState};
use hal::pwm::{Channel, ChannelId, DynSliceId, FreeRunning, SliceId};
use hal::{gpio, pwm};
use heapless::Vec;
use rp_pico::hal::pwm::Slices;
use rp_pico::Pins;

use crate::types::{self, CvPair, GateA, PwmGate, SliceAB, SliceCD};

/**
- 1V = C1(1)= MIDI note 24 = 32.703 Hz
- 3V = C3 = MIDI note 48 = 130.81 Hz
 */

fn note_to_voltage(key: u8) -> f32 {
    return (key - 12) as f32 / 12.0;
}

pub struct FourVoiceChannel {
    offset: u8,
    pairs: (CvPair<SliceAB>, CvPair<SliceCD>),
    gates: [PwmGate; 4],
    notes: [Option<(u16, u8)>; 4],
    count: u16,
}

impl FourVoiceChannel {
    pub fn new(
        slice_ab: SliceAB,
        slice_cd: SliceCD,
        pin_a: types::PwmA,
        pin_b: types::PwmB,
        pin_c: types::PwmC,
        pin_d: types::PwmD,
        gate_a: types::GateA,
        gate_b: types::GateB,
        gate_c: types::GateC,
        gate_d: types::GateD,
    ) -> Self {
        return Self {
            offset: 0,
            count: 0,
            pairs: (
                CvPair::new(slice_ab, pin_a, pin_b),
                CvPair::new(slice_cd, pin_d, pin_c),
            ),
            notes: [None, None, None, None],
            gates: [
                PwmGate::GateA(gate_a),
                PwmGate::GateB(gate_b),
                PwmGate::GateC(gate_c),
                PwmGate::GateD(gate_d),
            ],
        };
    }

    fn set_channel(&mut self, channel: usize, note: u8) {
        let voltage = note_to_voltage(note - self.offset);
        match channel {
            0 => self.pairs.0.set_a(voltage),
            1 => self.pairs.0.set_b(voltage),
            2 => self.pairs.1.set_a(voltage),
            3 => self.pairs.1.set_b(voltage),
            _ => {}
        }
    }

    fn find_oldest_channel(&self) -> usize {
        let mut oldest_count = 0;
        let mut oldest_channel = 0;
        for channel in 0..4 {
            match self.notes[channel] {
                None => {
                    oldest_channel = channel;
                    break;
                }
                Some((count, _)) => {
                    if count <= oldest_count {
                        oldest_channel = channel;
                        oldest_count = count
                    }
                }
            }
        }
        return oldest_channel;
    }

    fn find_by_note(&self, off_key: u8) -> Option<usize> {
        for channel in 0..4 {
            match self.notes[channel] {
                None => {}
                Some((_, key)) => {
                    if key == off_key {
                        return Some(channel);
                    }
                }
            }
        }
        return None;
    }

    pub fn note_on(&mut self, key: u8) {
        self.count += 1;
        let channel = self.find_oldest_channel();
        self.notes[channel] = Some((self.count, key));
        self.set_channel(channel, key);
        self.gates[channel].set_state(true);
    }

    pub fn note_off(&mut self, key: u8) {
        match self.find_by_note(key) {
            Some(channel) => self.gates[channel].set_state(false).unwrap(),
            None => {}
        }
    }
}

// Gate pitch value channel
pub struct GpvChannel {
    channel: u8,
    played_key: Option<u8>,
    gate: PwmGate,
    slice: SliceAB,
    max_voltage: f32,
    max_duty: u16,
    offset: f32,
}

impl GpvChannel {
    pub fn new(
        channel: u8,
        mut gate: PwmGate,
        mut slice: SliceAB,
        pwm_pins: (
            Pin<Gpio14, gpio::FunctionPwm, gpio::PullDown>,
            Pin<Gpio15, gpio::FunctionPwm, gpio::PullDown>,
        ),
    ) -> Self {
        slice.set_div_int(1u8);
        slice.set_div_frac(0u8);
        slice.set_top(0xA00);
        slice.enable();
        slice = slice.into_mode::<hal::pwm::FreeRunning>();
        let (pin_a, pin_b) = pwm_pins;
        slice.channel_a.output_to(pin_a);
        slice.channel_b.output_to(pin_b);
        slice.channel_b.set_inverted();
        slice.channel_a.set_inverted();
        slice.channel_a.set_duty(0x0);
        slice.channel_b.set_duty(0x0);
        gate.set_state(false);
        let max_duty = slice.channel_a.get_max_duty();

        return Self {
            channel,
            played_key: None,
            gate,
            slice,
            max_voltage: 5.00,
            max_duty,
            offset: 12.0,
        };
    }

    fn note_to_duty(&self, key: u8) -> u16 {
        let duty_per_voltage = self.max_duty as f32 / self.max_voltage;
        let volt = (key as f32 - self.offset) / 12.0;
        let duty = volt * duty_per_voltage;
        return duty as u16;
    }

    fn set_velocity(&mut self, vel: u8) {
        let duty_per_voltage = self.max_duty as f32 / self.max_voltage;
        self.slice
            .channel_b
            .set_duty((((vel as f32 * 5.0) / 127.0) * duty_per_voltage) as u16);
    }

    fn set_gate(&mut self, on: bool) {
        self.gate.set_state(on).unwrap()
    }

    pub fn note_off(&mut self, key: u8) {
        match self.played_key {
            Some(k) if k == key => {
                self.played_key = None;
                self.set_gate(false)
            }
            _ => (),
        }
    }

    pub fn aftertouch(&mut self, key: u8, vel: u8) {
        match self.played_key {
            Some(k) if k == key => self.set_velocity(vel),
            _ => (),
        }
    }

    pub fn note_on(&mut self, key: u8, vel: u8) {
        self.set_velocity(vel);
        self.slice.channel_a.set_duty(self.note_to_duty(key));
        self.set_gate(true);
        self.played_key = Some(key);
    }
}
