use embedded_hal::digital::v2::PinState as V2PinState;
use embedded_hal::digital::v2::{InputPin, OutputPin};
use rp_pico::hal::gpio;
use rp_pico::hal::gpio::bank0::*;
use rp_pico::hal::gpio::PinState;
use rp_pico::hal::gpio::Pins;

#[derive(Copy, Clone)]
pub enum Gate {
    BD,
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
    pub bd: gpio::Pin<Gpio27, gpio::FunctionSioOutput, gpio::PullDown>,
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
        self.bd.set_low().unwrap();
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
            Gate::BD => self.bd.set_state(V2PinState::from(state)).ok(),
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

pub enum OutputRequest {
    GateOn(Gate),
    GateOff(Gate),
}

pub struct OutputHandler {
    gates: GateMappings,
}

impl OutputHandler {
    pub fn new(mappings: GateMappings) -> Self {
        return Self { gates: mappings };
    }

    pub fn reset(&mut self) {
        self.gates.reset_all()
    }

    pub fn handle_message(&mut self, message: OutputRequest) -> Option<()> {
        match message {
            OutputRequest::GateOn(gate) => self.gates.set_state(gate, true),
            OutputRequest::GateOff(gate) => self.gates.set_state(gate, false),
        }
    }
}
