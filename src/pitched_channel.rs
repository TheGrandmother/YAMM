#![allow(dead_code)]
#![allow(unused_imports)]
#![allow(unreachable_code)]

use heapless::Vec;
use rp2040_hal::gpio::{DynPin, PinId, DYN_PUSH_PULL_OUTPUT, PinState};
use rp2040_hal::pwm::{DynSliceId, SliceId, FreeRunning};
use rp2040_hal::{gpio, pwm};
use embedded_hal::digital::v2::OutputPin;
use embedded_hal::PwmPin;
use rp_pico::Pins;

use crate::types::{GateA, VoiceSlice, VoicePwmPins};


pub struct PitchedChannel {
    channel: u8,
    played_key: Option<u8>,
    gate: GateA,
    slice: VoiceSlice,
    max_voltage: f32,
    max_duty: u16,
    offset: f32,
}

impl PitchedChannel {
    pub fn new(channel: u8, mut gate: GateA, mut slice: VoiceSlice, pwm_pins: VoicePwmPins) -> Self {
        slice.set_div_int(1u8);
        slice.set_div_frac(0u8);
        slice.set_top(0xfff);
        slice.enable();
        slice = slice.into_mode::<rp2040_hal::pwm::FreeRunning>();
        let (pin_a, pin_b) = pwm_pins;
        slice.channel_a.output_to(pin_a);
        slice.channel_b.output_to(pin_b);
        slice.channel_b.clr_inverted();
        slice.channel_a.clr_inverted();
        slice.channel_a.set_duty(0x0);
        slice.channel_b.set_duty(0x0);
        gate.set_high().ok();
        let max_duty = slice.channel_a.get_max_duty();

        return Self {
            channel,
            played_key: None,
            gate,
            slice,
            max_voltage: 5.00, // Measured to 4.86 but I am retarded
            max_duty,
            offset: 48.0
        }
    }

/**
- 1V = C1(1)= MIDI note 24 = 32.703 Hz
- 3V = C3 = MIDI note 48 = 130.81 Hz
 */

    fn note_to_duty(&self, key: u8) -> u16 {
        let duty_per_voltage = self.max_duty as f32 / self.max_voltage;
        let volt = (key as f32 - self.offset) / 12.0;
        let duty = volt * duty_per_voltage;
        return duty as u16 & self.max_duty
    }

    fn set_velocity(&mut self, vel: u8) {
        let duty_per_voltage = self.max_duty as f32 / self.max_voltage;
        self.slice.channel_b.set_duty((((vel as f32 * 5.0) / 127.0) * duty_per_voltage) as u16);
    }

    fn set_gate(&mut self, on: bool) {
        self.gate.set_state(PinState::from(!on)).unwrap()
    }

    pub fn note_off(&mut self, key: u8) {
        match self.played_key {
            Some(k) if k == key => {
                self.played_key = None;
                self.set_gate(false)
            },
            _ => ()
        }
    }

    pub fn aftertouch(&mut self, key: u8, vel: u8) {
        match self.played_key {
            Some(k) if k == key => self.set_velocity(vel),
            _ => ()
        }
    }

    pub fn note_on(&mut self, key: u8, vel: u8) {
        self.set_velocity(vel);
        self.slice.channel_a.set_duty(self.note_to_duty(key));
        self.set_gate(true);
        self.played_key = Some(key);
    }
}
