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
use rp_pico::hal::pwm::{AnySlice, Slices};
use rp_pico::Pins;

use crate::types::{self, CvPair, GateA, PwmGate, SliceAB, SliceCD};

/**
- 1V = C1(1)= MIDI note 24 = 32.703 Hz
- 3V = C3 = MIDI note 48 = 130.81 Hz
 */

fn note_to_voltage(key: u8) -> f32 {
    return (key - 12) as f32 / 12.0;
}

fn find_oldest_channel<const S: usize>(notes: [Option<(u16, u8)>; S]) -> usize {
    let mut oldest_count = 0;
    let mut oldest_channel = 0;
    for channel in 0..S {
        match notes[channel] {
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

fn find_by_note<const S: usize>(notes: [Option<(u16, u8)>; S], off_key: u8) -> Option<usize> {
    for channel in 0..4 {
        match notes[channel] {
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
        (pin_a, pin_b): (types::PwmA, types::PwmB),
        (pin_c, pin_d): (types::PwmC, types::PwmD),
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

    fn set_channel_note(&mut self, channel: usize, note: u8) {
        let voltage = note_to_voltage(note - self.offset);
        match channel {
            0 => self.pairs.0.set_a(voltage),
            1 => self.pairs.0.set_b(voltage),
            2 => self.pairs.1.set_a(voltage),
            3 => self.pairs.1.set_b(voltage),
            _ => {}
        }
    }

    pub fn note_on(&mut self, key: u8) {
        self.count += 1;
        let channel = find_oldest_channel::<4>(self.notes);
        self.notes[channel] = Some((self.count, key));
        self.set_channel_note(channel, key);
        self.gates[channel].set_state(true);
    }

    pub fn note_off(&mut self, key: u8) {
        match find_by_note::<4>(self.notes, key) {
            Some(channel) => {
                self.notes[channel] = None;
                self.gates[channel].set_state(false).unwrap()
            }
            None => {}
        }
    }
}

pub struct TwoVoiceChannel<S: AnySlice> {
    offset: u8,
    cv_pair: CvPair<S>,
    gates: [PwmGate; 2],
    notes: [Option<(u16, u8)>; 2],
    count: u16,
}

impl<S: AnySlice> TwoVoiceChannel<S> {
    pub fn new(cv_pair: CvPair<S>, gates: [PwmGate; 2]) -> Self {
        return Self {
            offset: 0,
            count: 0,
            cv_pair,
            notes: [None, None],
            gates,
        };
    }
}
