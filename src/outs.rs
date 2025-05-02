use embedded_hal::digital::v2::PinState as V2PinState;
use embedded_hal::digital::v2::{InputPin, OutputPin};
use fugit::{Duration, Instant};
use rp_pico::hal::gpio;
use rp_pico::hal::gpio::bank0::*;
use rp_pico::hal::gpio::PinState;
use rp_pico::hal::gpio::Pins;
use rtic_monotonics::rp2040::prelude::*;

use crate::pwm_pair::{CvPair, SliceAB, SliceCD};
use crate::Mono;

#[derive(Copy, Clone, PartialEq)]
pub enum Gate {
    Kick,
    OpenHH,
    Clap,
    Snare,
    FX,
    ClosedHH,
    Accent,
    Start,
    Stop,
    Clock,
    GateA,
    GateB,
    GateC,
    GateD,
}

pub struct GateMappings {
    pub kick: gpio::Pin<Gpio27, gpio::FunctionSioOutput, gpio::PullDown>,
    pub open_hh: gpio::Pin<Gpio7, gpio::FunctionSioOutput, gpio::PullDown>,
    pub clap: gpio::Pin<Gpio9, gpio::FunctionSioOutput, gpio::PullDown>,
    pub snare: gpio::Pin<Gpio10, gpio::FunctionSioOutput, gpio::PullDown>,
    pub fx: gpio::Pin<Gpio6, gpio::FunctionSioOutput, gpio::PullDown>,
    pub closed_hh: gpio::Pin<Gpio8, gpio::FunctionSioOutput, gpio::PullDown>,

    pub accent: gpio::Pin<Gpio5, gpio::FunctionSioOutput, gpio::PullDown>,

    pub start: gpio::Pin<Gpio26, gpio::FunctionSioOutput, gpio::PullDown>,
    pub stop: gpio::Pin<Gpio18, gpio::FunctionSioOutput, gpio::PullDown>,
    pub clock: gpio::Pin<Gpio28, gpio::FunctionSioOutput, gpio::PullDown>,

    pub gate_a: gpio::Pin<Gpio22, gpio::FunctionSioOutput, gpio::PullDown>,
    pub gate_b: gpio::Pin<Gpio21, gpio::FunctionSioOutput, gpio::PullDown>,
    pub gate_c: gpio::Pin<Gpio20, gpio::FunctionSioOutput, gpio::PullDown>,
    pub gate_d: gpio::Pin<Gpio19, gpio::FunctionSioOutput, gpio::PullDown>,
}

impl GateMappings {
    pub fn reset_drums(&mut self) {
        self.open_hh.set_low().unwrap();
        self.clap.set_low().unwrap();
        self.snare.set_low().unwrap();
        self.kick.set_low().unwrap();
        self.fx.set_low().unwrap();
        self.accent.set_low().unwrap();
        self.closed_hh.set_low().unwrap();
    }

    pub fn reset_gates(&mut self) {
        self.gate_a.set_low().unwrap();
        self.gate_b.set_low().unwrap();
        self.gate_c.set_low().unwrap();
        self.gate_d.set_low().unwrap();
    }

    pub fn reset_bus(&mut self) {
        self.start.set_low().unwrap();
        self.stop.set_low().unwrap();
        self.clock.set_low().unwrap();
    }

    pub fn reset_all(&mut self) {
        self.reset_drums();
        self.reset_gates();
        self.reset_bus();
    }

    pub(crate) fn set_state(&mut self, gate: Gate, state: bool) -> Option<()> {
        match gate {
            Gate::Kick => self.kick.set_state(V2PinState::from(state)).ok(),
            Gate::OpenHH => self.open_hh.set_state(V2PinState::from(state)).ok(),
            Gate::Clap => self.clap.set_state(V2PinState::from(state)).ok(),
            Gate::Snare => self.snare.set_state(V2PinState::from(state)).ok(),
            Gate::FX => self.fx.set_state(V2PinState::from(state)).ok(),
            Gate::ClosedHH => self.closed_hh.set_state(V2PinState::from(state)).ok(),
            Gate::Accent => self.accent.set_state(V2PinState::from(state)).ok(),
            Gate::Start => self.start.set_state(V2PinState::from(state)).ok(),
            Gate::Stop => self.stop.set_state(V2PinState::from(state)).ok(),
            Gate::Clock => self.clock.set_state(V2PinState::from(state)).ok(),
            Gate::GateA => self.gate_a.set_state(V2PinState::from(state)).ok(),
            Gate::GateB => self.gate_b.set_state(V2PinState::from(state)).ok(),
            Gate::GateC => self.gate_c.set_state(V2PinState::from(state)).ok(),
            Gate::GateD => self.gate_d.set_state(V2PinState::from(state)).ok(),
        }
    }
}

/**
- 1V = C1(1)= MIDI note 24 = 32.703 Hz
- 3V = C3 = MIDI note 48 = 130.81 Hz
 */

fn note_to_voltage(key: u8) -> f32 {
    return (key - 12) as f32 / 12.0;
}

#[derive(Copy, Clone)]
pub enum Cv {
    A,
    B,
    C,
    D,
}

pub struct CvPorts {
    pub ab_pair: CvPair<SliceAB>,
    pub cd_pair: CvPair<SliceCD>,
}

impl CvPorts {
    fn reset(&mut self) {
        self.ab_pair.set_a(0.0);
        self.ab_pair.set_b(0.0);
        self.cd_pair.set_a(0.0);
        self.cd_pair.set_b(0.0);
    }

    fn set_output(&mut self, cv: Cv, voltage: f32) -> Option<()> {
        match cv {
            Cv::A => self.ab_pair.set_a(voltage),
            Cv::B => self.ab_pair.set_b(voltage),
            Cv::C => self.cd_pair.set_b(voltage),
            Cv::D => self.cd_pair.set_a(voltage),
        }
    }

    fn set_note(&mut self, cv: Cv, note: u8) -> Option<()> {
        self.set_output(cv, note_to_voltage(note))
    }

    fn set_val(&mut self, cv: Cv, val: f32) -> Option<()> {
        self.set_output(cv, val * 5.0)
    }
}

pub enum OutputRequest {
    GateOn(Gate),
    GateOff(Gate),
    SetNote(Cv, u8),
    SetVal(Cv, f32),
    Flash(Gate),
}

type Time = Instant<u64, 1, 1_000_000>;
const TIMERS: usize = 8;

pub struct OutputHandler {
    gates: GateMappings,
    ports: CvPorts,
    flashes: [Option<(Gate, Time)>; TIMERS],
    flash_time: Duration<u64, 1, 1000000>,
}

impl OutputHandler {
    pub fn new(gates: GateMappings, cv_ports: CvPorts) -> Self {
        return Self {
            gates,
            ports: cv_ports,
            flashes: [None; TIMERS],
            flash_time: 20.millis(),
        };
    }

    pub fn reset(&mut self) {
        self.gates.reset_all();
        self.ports.reset();
    }

    pub fn check_flashes(&mut self) -> Option<Duration<u64, 1, 1000000>> {
        let mut shortest_wait = None;
        for i in 0..TIMERS {
            match self.flashes[i] {
                Some((gate, start)) => {
                    let age = Mono::now() - start;
                    if age >= self.flash_time {
                        self.gates.set_state(gate, false);
                        self.flashes[i] = None
                    } else {
                        if shortest_wait == None || Some(age) < shortest_wait {
                            shortest_wait = Some(age);
                        }
                    }
                }
                None => {}
            }
        }
        return shortest_wait;
    }

    // Not super efficient, same gate can appear multiple times
    // Is fine. Probably.
    fn add_flash(&mut self, gate: Gate) {
        for i in 0..TIMERS {
            match self.flashes[i] {
                Some((g, _)) if g == gate => self.flashes[i] = Some((gate, Mono::now())),
                Some(_) => {}
                None => self.flashes[i] = Some((gate, Mono::now())),
            }
        }
    }

    pub fn handle_message(&mut self, message: OutputRequest) -> Option<()> {
        match message {
            OutputRequest::GateOn(gate) => self.gates.set_state(gate, true),
            OutputRequest::GateOff(gate) => self.gates.set_state(gate, false),
            OutputRequest::SetNote(port, note) => self.ports.set_note(port, note),
            OutputRequest::SetVal(port, val) => self.ports.set_val(port, val),
            OutputRequest::Flash(gate) => {
                self.gates.set_state(gate, true);
                self.add_flash(gate);
                None
            }
        }
    }
}
