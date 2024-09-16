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
use rp_pico::Pins;

use crate::types::{GateA, PwmGate, VoiceSliceAB, VoiceSliceCD};

// pub struct PwmCV<S: AnySlice, C: ChannelId + ?Sized> {
//     channel: Channel<S,rp2040_hal::pwm::FreeRunning, C>,
//     max_voltage: f32,
//     max_duty: u16,
//     offset: f32,
// }
// impl PwmCV<dyn SliceId<Reset=rp2040_hal::pwm::FreeRunning>, dyn ChannelId> {
//     pub fn new<S>(
//         channel: u8,
//         mut slice: S,
//         pwm_pins: VoicePwmPins,
//     ) -> Self {
//         slice.set_div_int(1u8);
//         slice.set_div_frac(0u8);
//         slice.set_top(0xA00);
//         slice.enable();
//         slice = slice.into_mode::<rp2040_hal::pwm::FreeRunning>();
//         let (pin_a, pin_b) = pwm_pins;
//         slice.channel_a.output_to(pin_a);
//         slice.channel_b.output_to(pin_b);
//         slice.channel_b.set_inverted();
//         slice.channel_a.set_inverted();
//         slice.channel_a.set_duty(0x0);
//         slice.channel_b.set_duty(0x0);
//         gate.set_state(false);
//         let max_duty = slice.channel_a.get_max_duty();
//
//         return Self {
//             channel,
//             max_voltage: 5.00,
//             max_duty,
//             offset: 12.0,
//         };
//     }
//
//     /**
//     - 1V = C1(1)= MIDI note 24 = 32.703 Hz
//     - 3V = C3 = MIDI note 48 = 130.81 Hz
//      */
//
//     fn note_to_duty(&self, key: u8) -> u16 {
//         let duty_per_voltage = self.max_duty as f32 / self.max_voltage;
//         let volt = (key as f32 - self.offset) / 12.0;
//         let duty = volt * duty_per_voltage;
//         return duty as u16;
//     }
// }

// Gate pitch value channel
pub struct GpvChannel {
    channel: u8,
    played_key: Option<u8>,
    gate: PwmGate,
    slice: VoiceSliceAB,
    max_voltage: f32,
    max_duty: u16,
    offset: f32,
}

impl GpvChannel {
    pub fn new(
        channel: u8,
        mut gate: PwmGate,
        mut slice: VoiceSliceAB,
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

    /**
    - 1V = C1(1)= MIDI note 24 = 32.703 Hz
    - 3V = C3 = MIDI note 48 = 130.81 Hz
     */

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
