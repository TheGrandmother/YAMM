use rp_pico::hal::gpio::{self, Interrupt, Pin};

use crate::commando_unit::{CommandEvent, Input};
use crate::midi_master::MessageSender;

pub struct ButtonHandler {
    pub play_btn: Pin<gpio::bank0::Gpio11, gpio::FunctionSioInput, gpio::PullUp>,
    pub step_btn: Pin<gpio::bank0::Gpio12, gpio::FunctionSioInput, gpio::PullUp>,
    pub rec_btn: Pin<gpio::bank0::Gpio13, gpio::FunctionSioInput, gpio::PullUp>,
    pub commando_sender: MessageSender<CommandEvent>,
}

impl ButtonHandler {
    pub fn init(&mut self) {
        self.play_btn
            .set_interrupt_enabled(Interrupt::EdgeLow, true);
        self.play_btn
            .set_interrupt_enabled(Interrupt::EdgeHigh, true);
        self.rec_btn.set_interrupt_enabled(Interrupt::EdgeLow, true);
        self.rec_btn
            .set_interrupt_enabled(Interrupt::EdgeHigh, true);
        self.step_btn
            .set_interrupt_enabled(Interrupt::EdgeLow, true);
        self.step_btn
            .set_interrupt_enabled(Interrupt::EdgeHigh, true);
        self.play_btn.clear_interrupt(Interrupt::EdgeLow);
        self.rec_btn.clear_interrupt(Interrupt::EdgeLow);
        self.step_btn.clear_interrupt(Interrupt::EdgeLow);
        self.play_btn.clear_interrupt(Interrupt::EdgeHigh);
        self.rec_btn.clear_interrupt(Interrupt::EdgeHigh);
        self.step_btn.clear_interrupt(Interrupt::EdgeHigh);
    }

    pub fn handle_irq(&mut self) {
        if self.play_btn.interrupt_status(Interrupt::EdgeLow) {
            self.commando_sender
                .try_send(CommandEvent::Down(Input::Play))
                .ok();
            self.play_btn.clear_interrupt(Interrupt::EdgeLow);
        }
        if self.play_btn.interrupt_status(Interrupt::EdgeHigh) {
            self.commando_sender
                .try_send(CommandEvent::Up(Input::Play))
                .ok();
            self.play_btn.clear_interrupt(Interrupt::EdgeHigh);
        }

        if self.step_btn.interrupt_status(Interrupt::EdgeLow) {
            self.commando_sender
                .try_send(CommandEvent::Down(Input::Step))
                .ok();
            self.step_btn.clear_interrupt(Interrupt::EdgeLow);
        }
        if self.step_btn.interrupt_status(Interrupt::EdgeHigh) {
            self.commando_sender
                .try_send(CommandEvent::Up(Input::Step))
                .ok();
            self.step_btn.clear_interrupt(Interrupt::EdgeHigh);
        }

        if self.rec_btn.interrupt_status(Interrupt::EdgeLow) {
            self.commando_sender
                .try_send(CommandEvent::Down(Input::Rec))
                .ok();
            self.rec_btn.clear_interrupt(Interrupt::EdgeLow);
        }
        if self.rec_btn.interrupt_status(Interrupt::EdgeHigh) {
            self.commando_sender
                .try_send(CommandEvent::Up(Input::Rec))
                .ok();
            self.rec_btn.clear_interrupt(Interrupt::EdgeHigh);
        }
    }
}
