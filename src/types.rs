use embedded_hal::digital::v2::{InputPin, OutputPin, PinState};
use embedded_hal::PwmPin;
use hal::gpio::bank0::*;
use hal::gpio::PinId;
use hal::gpio::{self};
use hal::pwm;
use hal::pwm::SliceId;
use midly::num::u7;
use rp_pico::hal;
use rp_pico::hal::gpio::AnyPin;
use rp_pico::hal::gpio::Pin;
use rp_pico::hal::pwm::AnySlice;
use rp_pico::hal::pwm::Channel;
use rp_pico::hal::pwm::Slices;
use rp_pico::hal::pwm::ValidPwmOutputPin;

/*

GATE_A 5
GATE_B 6
GATE_C 7
DATE_D 8

O_HH    9
CL      10
SD      11
BD      12
ACCENT  21
FX      20

START 13
CLOCK 19
STOP  18


CONF 2,3,4

*/

pub type OpenHH = gpio::Pin<Gpio9, gpio::FunctionSioOutput, gpio::PullDown>;
pub type Clap = gpio::Pin<Gpio10, gpio::FunctionSioOutput, gpio::PullDown>;
pub type Snare = gpio::Pin<Gpio11, gpio::FunctionSioOutput, gpio::PullDown>;
pub type BD = gpio::Pin<Gpio12, gpio::FunctionSioOutput, gpio::PullDown>;
pub type FX = gpio::Pin<Gpio20, gpio::FunctionSioOutput, gpio::PullDown>;
pub type Accent = gpio::Pin<Gpio21, gpio::FunctionSioOutput, gpio::PullDown>;
pub type ClosedHH = gpio::Pin<Gpio22, gpio::FunctionSioOutput, gpio::PullDown>;

pub type Start = gpio::Pin<Gpio16, gpio::FunctionSioOutput, gpio::PullDown>; //wack
pub type Ctrl = gpio::Pin<Gpio17, gpio::FunctionSioOutput, gpio::PullDown>;
pub type Stop = gpio::Pin<Gpio18, gpio::FunctionSioOutput, gpio::PullDown>;
pub type Clock = gpio::Pin<Gpio19, gpio::FunctionSioOutput, gpio::PullDown>;

pub type ConfA = gpio::Pin<Gpio2, gpio::SioInput, gpio::PullDown>;
pub type ConfB = gpio::Pin<Gpio2, gpio::SioInput, gpio::PullDown>;
pub type ConfC = gpio::Pin<Gpio2, gpio::SioInput, gpio::PullDown>;

pub type GateA = gpio::Pin<Gpio5, gpio::FunctionSioOutput, gpio::PullDown>;
pub type GateB = gpio::Pin<Gpio6, gpio::FunctionSioOutput, gpio::PullDown>;
pub type GateC = gpio::Pin<Gpio7, gpio::FunctionSioOutput, gpio::PullDown>;
pub type GateD = gpio::Pin<Gpio8, gpio::FunctionSioOutput, gpio::PullDown>;

pub enum PwmGate {
    GateA(gpio::Pin<Gpio5, gpio::FunctionSioOutput, gpio::PullDown>),
    GateB(gpio::Pin<Gpio6, gpio::FunctionSioOutput, gpio::PullDown>),
    GateC(gpio::Pin<Gpio7, gpio::FunctionSioOutput, gpio::PullDown>),
    GateD(gpio::Pin<Gpio8, gpio::FunctionSioOutput, gpio::PullDown>),
}
impl PwmGate {
    pub(crate) fn set_state(&mut self, state: bool) -> Option<()> {
        match self {
            PwmGate::GateA(x) => x.set_state(PinState::from(state)).ok(),
            PwmGate::GateB(x) => x.set_state(PinState::from(state)).ok(),
            PwmGate::GateC(x) => x.set_state(PinState::from(state)).ok(),
            PwmGate::GateD(x) => x.set_state(PinState::from(state)).ok(),
        }
    }
}

// PWM_A and PWM_B Map to PWM Channel 7A and 7B
// PWM_C and PWM_D Map to PWM Channel 0A and 0B
// PWM_A 14
// PWM_B 15
// PWM_C 17
// PWM_D 16

pub type VoiceSliceAB = hal::pwm::Slice<hal::pwm::Pwm7, pwm::FreeRunning>;
pub type VoiceSliceCD = hal::pwm::Slice<hal::pwm::Pwm0, pwm::FreeRunning>;
// pub type PwmA = hal::gpio::Pin<Gpio14, <Gpio14 as PinId>::Reset>;
// pub type PwmB = hal::gpio::Pin<Gpio15, <Gpio15 as PinId>::Reset>;
// pub type PwmC = hal::gpio::Pin<Gpio17, <Gpio17 as PinId>::Reset>;
// pub type PwmD = hal::gpio::Pin<Gpio16, <Gpio16 as PinId>::Reset>;

pub struct Drums {
    pub kick: BD,
    pub snare: Snare,
    pub open_hh: OpenHH,
    pub closed_hh: ClosedHH,
    pub clap: Clap,
    pub fx: FX,
    pub accent: Accent,
}

impl Drums {
    pub fn reset(&mut self) {
        self.open_hh.set_low().unwrap();
        self.clap.set_low().unwrap();
        self.snare.set_low().unwrap();
        self.kick.set_low().unwrap();
        self.fx.set_low().unwrap();
        self.accent.set_low().unwrap();
        self.closed_hh.set_low().unwrap();
    }

    pub fn set(&mut self, key: u7, state: bool) {
        match u8::from(key) {
            36 => self.kick.set_state(PinState::from(state)).unwrap(),
            37 => self.snare.set_state(PinState::from(state)).unwrap(),
            38 => self.clap.set_state(PinState::from(state)).unwrap(),
            39 => self.open_hh.set_state(PinState::from(state)).unwrap(),
            40 => self.closed_hh.set_state(PinState::from(state)).unwrap(),
            41 => self.fx.set_state(PinState::from(state)).unwrap(),
            42 => self.accent.set_state(PinState::from(state)).unwrap(),
            _ => (),
        }
    }
}

pub enum BusSignals {
    START,
    STOP,
    CLOCK,
}

pub struct Bus {
    pub start: Start,
    pub stop: Stop,
    pub clock: Clock,
}

impl Bus {
    pub fn reset(&mut self) {
        self.start.set_low().unwrap();
        self.clock.set_low().unwrap();
        self.stop.set_low().unwrap();
    }

    pub fn set(&mut self, signal: BusSignals, state: bool) {
        match signal {
            BusSignals::START => self.start.set_state(PinState::from(state)).unwrap(),
            BusSignals::STOP => self.stop.set_state(PinState::from(state)).unwrap(),
            BusSignals::CLOCK => self.clock.set_state(PinState::from(state)).unwrap(),
        }
    }
}

pub struct CvPair<S, const TOP: u16 = 0xA00>
where
    S: AnySlice,
{
    slice: hal::pwm::Slice<S::Id, pwm::FreeRunning>,
    max_voltage: f32,
    max_duty: u16,
}

impl<S, const TOP: u16> CvPair<S, TOP>
where
    S: AnySlice,
{
    fn new<PA, PB>(
        mut slice: hal::pwm::Slice<S::Id, pwm::FreeRunning>,
        pin_a: PA,
        pin_b: PB,
    ) -> Self
    where
        PA: AnyPin,
        PA::Id: ValidPwmOutputPin<S::Id, pwm::A>,
        PB: AnyPin,
        PB::Id: ValidPwmOutputPin<S::Id, pwm::B>,
    {
        slice.set_div_int(1u8);
        slice.set_div_frac(0u8);
        slice.set_top(TOP);
        slice.enable();
        slice = slice.into_mode::<hal::pwm::FreeRunning>();
        slice.channel_a.output_to(pin_a);
        slice.channel_b.output_to(pin_b);
        slice.channel_b.set_inverted();
        slice.channel_a.set_inverted();
        slice.channel_a.set_duty(0x0);
        slice.channel_b.set_duty(0x0);
        let max_duty = slice.channel_a.get_max_duty();

        return Self {
            max_voltage: 5.00,
            max_duty,
            slice,
        };
    }
}
