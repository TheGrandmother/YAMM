use fugit::{Duration, Instant};
use rtic_monotonics::rp2040::prelude::*;

use rp_pico::hal::gpio::{self, DynPinId, FunctionSio, Interrupt, Pin, PinId, PullUp, SioInput};

use crate::commando_unit::{CommandEvent, Input};
use crate::midi_master::MessageSender;
use crate::Mono;

pub struct ButtonHandler {
    play_btn: Pin<gpio::bank0::Gpio11, gpio::FunctionSioInput, gpio::PullUp>,
    play_ts: Instant<u64, 1, 1_000_000>,
    step_btn: Pin<gpio::bank0::Gpio12, gpio::FunctionSioInput, gpio::PullUp>,
    step_ts: Instant<u64, 1, 1_000_000>,
    rec_btn: Pin<gpio::bank0::Gpio13, gpio::FunctionSioInput, gpio::PullUp>,
    rec_ts: Instant<u64, 1, 1_000_000>,
    commando_sender: MessageSender<CommandEvent>,
}

impl ButtonHandler {
    pub fn new(
        play_btn: Pin<gpio::bank0::Gpio11, gpio::FunctionSioInput, gpio::PullUp>,
        step_btn: Pin<gpio::bank0::Gpio12, gpio::FunctionSioInput, gpio::PullUp>,
        rec_btn: Pin<gpio::bank0::Gpio13, gpio::FunctionSioInput, gpio::PullUp>,
        commando_sender: MessageSender<CommandEvent>,
    ) -> Self {
        let mut me = ButtonHandler {
            play_btn,
            step_btn,
            rec_btn,
            commando_sender,
            play_ts: Mono::now(),
            rec_ts: Mono::now(),
            step_ts: Mono::now(),
        };
        me.play_btn.set_interrupt_enabled(Interrupt::EdgeLow, true);
        me.play_btn.set_interrupt_enabled(Interrupt::EdgeHigh, true);
        me.rec_btn.set_interrupt_enabled(Interrupt::EdgeLow, true);
        me.rec_btn.set_interrupt_enabled(Interrupt::EdgeHigh, true);
        me.step_btn.set_interrupt_enabled(Interrupt::EdgeLow, true);
        me.step_btn.set_interrupt_enabled(Interrupt::EdgeHigh, true);
        me.clear_interrupts();
        me
    }

    fn get_last_sat(&self, id: DynPinId) -> Instant<u64, 1, 1000000> {
        match id.num {
            11 => self.play_ts,
            12 => self.step_ts,
            13 => self.rec_ts,
            _ => Mono::now(), // Really silly
        }
    }

    fn set_last_sat(&mut self, id: DynPinId) {
        match id.num {
            11 => self.play_ts = Mono::now(),
            12 => self.step_ts = Mono::now(),
            13 => self.rec_ts = Mono::now(),
            _ => {} // Really silly
        }
    }

    fn clear_interrupts(&mut self) {
        self.play_btn.clear_interrupt(Interrupt::EdgeLow);
        self.rec_btn.clear_interrupt(Interrupt::EdgeLow);
        self.step_btn.clear_interrupt(Interrupt::EdgeLow);
        self.play_btn.clear_interrupt(Interrupt::EdgeHigh);
        self.rec_btn.clear_interrupt(Interrupt::EdgeHigh);
        self.step_btn.clear_interrupt(Interrupt::EdgeHigh);
    }

    pub fn handle_irq(&mut self) {
        let holdoff = 10.millis::<1, 1_000_000>();
        if self.play_btn.interrupt_status(Interrupt::EdgeLow) {
            if Mono::now() > self.get_last_sat(self.play_btn.id()) + holdoff {
                self.commando_sender
                    .try_send(CommandEvent::Down(Input::Play))
                    .ok();
                self.set_last_sat(self.play_btn.id())
            }
            self.play_btn.clear_interrupt(Interrupt::EdgeLow);
        }
        if self.play_btn.interrupt_status(Interrupt::EdgeHigh) {
            if Mono::now() > self.get_last_sat(self.play_btn.id()) + holdoff {
                self.commando_sender
                    .try_send(CommandEvent::Up(Input::Play))
                    .ok();
                self.set_last_sat(self.play_btn.id())
            }
            self.play_btn.clear_interrupt(Interrupt::EdgeHigh);
        }

        if self.step_btn.interrupt_status(Interrupt::EdgeLow) {
            if Mono::now() > self.get_last_sat(self.step_btn.id()) + holdoff {
                self.commando_sender
                    .try_send(CommandEvent::Down(Input::Step))
                    .ok();
                self.set_last_sat(self.step_btn.id())
            }
            self.step_btn.clear_interrupt(Interrupt::EdgeLow);
        }
        if self.step_btn.interrupt_status(Interrupt::EdgeHigh) {
            if Mono::now() > self.get_last_sat(self.step_btn.id()) + holdoff {
                self.commando_sender
                    .try_send(CommandEvent::Up(Input::Step))
                    .ok();
                self.set_last_sat(self.step_btn.id())
            }
            self.step_btn.clear_interrupt(Interrupt::EdgeHigh);
        }

        if self.rec_btn.interrupt_status(Interrupt::EdgeLow) {
            if Mono::now() > self.get_last_sat(self.rec_btn.id()) + holdoff {
                self.commando_sender
                    .try_send(CommandEvent::Down(Input::Rec))
                    .ok();
                self.set_last_sat(self.rec_btn.id())
            }
            self.rec_btn.clear_interrupt(Interrupt::EdgeLow);
        }
        if self.rec_btn.interrupt_status(Interrupt::EdgeHigh) {
            if Mono::now() > self.get_last_sat(self.rec_btn.id()) + holdoff {
                self.commando_sender
                    .try_send(CommandEvent::Up(Input::Rec))
                    .ok();
                self.set_last_sat(self.rec_btn.id())
            }
            self.rec_btn.clear_interrupt(Interrupt::EdgeHigh);
        }
    }
}
