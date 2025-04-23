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
use rp_pico::hal::pwm::ValidPwmOutputPin;
use rp_pico::hal::pwm::{AnySlice, Slice};
use rp_pico::hal::pwm::{Channel, DynChannelId};
use rp_pico::hal::pwm::{DynSliceId, Slices};

const PWM_TOP: u16 = 0xA00;

pub type SliceAB = hal::pwm::Slice<hal::pwm::Pwm7, pwm::FreeRunning>;
pub type SliceCD = hal::pwm::Slice<hal::pwm::Pwm0, pwm::FreeRunning>;
pub type PwmA = hal::gpio::Pin<Gpio14, gpio::FunctionPwm, gpio::PullDown>;
pub type PwmB = hal::gpio::Pin<Gpio15, gpio::FunctionPwm, gpio::PullDown>;
pub type PwmC = hal::gpio::Pin<Gpio17, gpio::FunctionPwm, gpio::PullDown>;
pub type PwmD = hal::gpio::Pin<Gpio16, gpio::FunctionPwm, gpio::PullDown>;

pub struct CvPair<S>
where
    S: AnySlice,
{
    slice: hal::pwm::Slice<S::Id, pwm::FreeRunning>,
    max_voltage: f32,
    max_duty: u16,
}
impl<S> CvPair<S>
where
    S: AnySlice,
{
    pub fn new<PA, PB>(
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
        slice.set_top(PWM_TOP);
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

    pub fn set(&mut self, ch: DynChannelId, voltage: f32) -> Option<()> {
        let duty: u16;
        if voltage > 5.0 {
            duty = self.max_duty;
        } else if voltage < 0.0 {
            duty = 1
        } else {
            duty = (self.max_duty as f32 * voltage / 5.0) as u16;
        }
        match ch {
            DynChannelId::A => self.slice.channel_a.set_duty(self.max_duty - duty),
            DynChannelId::B => self.slice.channel_b.set_duty(self.max_duty - duty),
        }
        return Some(());
    }

    pub fn set_a(&mut self, voltage: f32) -> Option<()> {
        self.set(DynChannelId::A, voltage)
    }
    pub fn set_b(&mut self, voltage: f32) -> Option<()> {
        self.set(DynChannelId::B, voltage)
    }
}
