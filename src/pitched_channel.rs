#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unreachable_code)]

use heapless::Vec;
use rp2040_hal::gpio::{DynPin, PinId, DYN_PUSH_PULL_OUTPUT};
use rp2040_hal::pwm::{DynSliceId, SliceId, FreeRunning};
use rp2040_hal::{gpio, pwm};
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::PwmPin;


pub struct PitchedChannel<'a> {
    channel: u8,
    played_key: Option<u8>,
    set_pitch_and_vel: &'a dyn FnMut(u16, u16),
    set_gate: &'a dyn FnMut(bool),
}

impl<'a> PitchedChannel<'a> {
    pub fn new(channel: u8, set_gate: &'a dyn FnMut(bool), set_pitch_and_vel: &'a dyn FnMut(u16, u16)) -> Self {
        return Self {
            channel,
            played_key: None,
            set_gate,
            set_pitch_and_vel,
        }
    }

    pub fn note_off(&mut self, key: u8) {
        todo!()
    }

    pub fn note_on(&mut self, key: u8, vel: u8) {
        todo!()
    }
}
