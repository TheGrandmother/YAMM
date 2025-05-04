use fugit::{Duration, Instant};
use rtic_monotonics::rp2040::prelude::*;

use rp_pico::hal::gpio::{self, DynPinId, FunctionSio, Interrupt, Pin, PinId, PullUp, SioInput};

use crate::commando_unit::{CommandEvent, Input};
use crate::midi_master::MessageSender;
use crate::Mono;

type Mjau = Instant<u64, 1, 1_000_000>;

#[derive(Copy, Clone)]
enum State {
    Unknown,
    Invalid,
    Down(Mjau), // Went down at
    Up(Mjau),
}

pub struct ButtonHandler {
    play_btn: Pin<gpio::bank0::Gpio11, gpio::FunctionSioInput, gpio::PullUp>,
    play_state: State,
    step_btn: Pin<gpio::bank0::Gpio12, gpio::FunctionSioInput, gpio::PullUp>,
    step_state: State,
    rec_btn: Pin<gpio::bank0::Gpio13, gpio::FunctionSioInput, gpio::PullUp>,
    rec_state: State,
    holdoff: Duration<u64, 1, 1000000>,
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
            play_state: State::Unknown,
            rec_state: State::Unknown,
            step_state: State::Unknown,
            holdoff: 5.millis::<1, 1_000_000>(),
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

    fn get_state(&mut self, id: DynPinId) -> State {
        match id.num {
            11 => self.play_state,
            12 => self.step_state,
            13 => self.rec_state,
            _ => State::Invalid,
        }
    }

    fn can_down(&mut self, id: DynPinId) -> bool {
        match self.get_state(id) {
            State::Unknown => true,
            State::Up(x) if Mono::now() > x + self.holdoff => true,
            _ => false,
        }
    }

    fn can_up(&mut self, id: DynPinId) -> bool {
        match self.get_state(id) {
            State::Unknown => true,
            State::Down(x) if Mono::now() > x + self.holdoff => true,
            _ => false,
        }
    }

    fn go_down(&mut self, id: DynPinId) {
        match id.num {
            11 => self.play_state = State::Down(Mono::now()),
            12 => self.step_state = State::Down(Mono::now()),
            13 => self.rec_state = State::Down(Mono::now()),
            _ => {}
        }
    }

    fn go_up(&mut self, id: DynPinId) {
        match id.num {
            11 => self.play_state = State::Up(Mono::now()),
            12 => self.step_state = State::Up(Mono::now()),
            13 => self.rec_state = State::Up(Mono::now()),
            _ => {}
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
        if self.play_btn.interrupt_status(Interrupt::EdgeLow) {
            if self.can_down(self.play_btn.id()) {
                self.commando_sender
                    .try_send(CommandEvent::Down(Input::Play))
                    .ok();

                self.go_down(self.play_btn.id())
            }
            self.play_btn.clear_interrupt(Interrupt::EdgeLow);
        }
        if self.play_btn.interrupt_status(Interrupt::EdgeHigh) {
            if self.can_up(self.play_btn.id()) {
                self.commando_sender
                    .try_send(CommandEvent::Up(Input::Play))
                    .ok();

                self.go_up(self.play_btn.id())
            }
            self.play_btn.clear_interrupt(Interrupt::EdgeHigh);
        }

        if self.step_btn.interrupt_status(Interrupt::EdgeLow) {
            if self.can_down(self.step_btn.id()) {
                self.commando_sender
                    .try_send(CommandEvent::Down(Input::Step))
                    .ok();

                self.go_down(self.step_btn.id())
            }
            self.step_btn.clear_interrupt(Interrupt::EdgeLow);
        }
        if self.step_btn.interrupt_status(Interrupt::EdgeHigh) {
            if self.can_up(self.step_btn.id()) {
                self.commando_sender
                    .try_send(CommandEvent::Up(Input::Step))
                    .ok();

                self.go_up(self.step_btn.id())
            }
            self.step_btn.clear_interrupt(Interrupt::EdgeHigh);
        }

        if self.rec_btn.interrupt_status(Interrupt::EdgeLow) {
            if self.can_down(self.rec_btn.id()) {
                self.commando_sender
                    .try_send(CommandEvent::Down(Input::Rec))
                    .ok();

                self.go_down(self.rec_btn.id())
            }
            self.rec_btn.clear_interrupt(Interrupt::EdgeLow);
        }
        if self.rec_btn.interrupt_status(Interrupt::EdgeHigh) {
            if self.can_up(self.rec_btn.id()) {
                self.commando_sender
                    .try_send(CommandEvent::Up(Input::Rec))
                    .ok();

                self.go_up(self.rec_btn.id())
            }
            self.rec_btn.clear_interrupt(Interrupt::EdgeHigh);
        }
    }
}
