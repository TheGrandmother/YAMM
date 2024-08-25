use embedded_hal::digital::v2::OutputPin;
use hal::gpio::PinId;
use midly::num::u7;
use rp2040_hal as hal;
use rp2040_hal::pwm;
use rp2040_hal::gpio::{self, PinState};
use rp2040_hal::gpio::pin::bank0::*;


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

PWM_A 14
PWM_B 15
PWM_C 17
PWM_D 16

CONF 2,3,4

*/


pub type OpenHH = gpio::Pin<Gpio9, gpio::PushPullOutput>;
pub type Clap = gpio::Pin<Gpio10, gpio::PushPullOutput>;
pub type Snare = gpio::Pin<Gpio11, gpio::PushPullOutput>;
pub type BD = gpio::Pin<Gpio12, gpio::PushPullOutput>;
pub type FX = gpio::Pin<Gpio20, gpio::PushPullOutput>;
pub type Accent = gpio::Pin<Gpio21, gpio::PushPullOutput>;
pub type ClosedHH = gpio::Pin<Gpio22, gpio::PushPullOutput>;

pub type Start = gpio::Pin<Gpio16, gpio::PushPullOutput>; //wack
pub type Ctrl = gpio::Pin<Gpio17, gpio::PushPullOutput>; 
pub type Stop = gpio::Pin<Gpio18, gpio::PushPullOutput>;
pub type Clock = gpio::Pin<Gpio19, gpio::PushPullOutput>;



pub type GateA = gpio::Pin<Gpio13, gpio::PushPullOutput>;

pub type VoiceSlice = hal::pwm::Slice<hal::pwm::Pwm7, pwm::FreeRunning>;
pub type VoicePwmPins = (hal::gpio::Pin<Gpio14, <Gpio14 as PinId>::Reset>, hal::gpio::Pin<Gpio15, <Gpio15 as PinId>::Reset>);



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
        self.open_hh.set_high().unwrap();
        self.clap.set_high().unwrap();
        self.snare.set_high().unwrap();
        self.kick.set_high().unwrap();
        self.fx.set_high().unwrap();
        self.accent.set_high().unwrap();
        self.closed_hh.set_high().unwrap();
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
            _ => ()
        }

    }
}
