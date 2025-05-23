use core::cmp::Ordering;
use core::ops::Not;

use heapless::spsc::Queue;
use heapless::Vec;
use midly::live::LiveEvent;
use midly::MidiMessage;

use crate::midi_mapper::make_all_notes_off;
use crate::midi_master::MessageSender;
use crate::outs::{Gate, OutputRequest};

type Ticks = u32;

pub const SUBS_PER_STEP: u32 = 128;
const STEP_CAP: usize = 8;
pub const MAX_LENGTH: usize = 32;
pub const INITIAL_LENGTH: u8 = 16;

#[derive(Copy, Clone, PartialEq, Eq)]
struct TimeStamp {
    step: u32,
    sub: u32,
}

impl PartialOrd for TimeStamp {
    // This does not handle last step....
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(if self.step < other.step {
            Ordering::Less
        } else if self.step > other.step {
            Ordering::Greater
        } else {
            if self.sub < other.sub {
                Ordering::Less
            } else if self.sub > other.sub {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
    }
}

#[derive(Copy, Clone)]
struct Event {
    midi_event: LiveEvent<'static>,
    ts: TimeStamp,
}

impl Event {
    fn replaces(self, other: Event) -> bool {
        if self.ts.step == other.ts.step {
            match (self.midi_event, other.midi_event) {
                (
                    LiveEvent::Midi {
                        channel: c1,
                        message: m1,
                    },
                    LiveEvent::Midi {
                        channel: c2,
                        message: m2,
                    },
                ) if c1 == c2 => match (m1, m2) {
                    (
                        MidiMessage::NoteOff { key: k1, .. },
                        MidiMessage::NoteOff { key: k2, .. },
                    ) => k1 == k2,
                    (MidiMessage::NoteOn { key: k1, .. }, MidiMessage::NoteOn { key: k2, .. }) => {
                        k1 == k2
                    }
                    _ => false,
                },
                _ => false,
            }
        } else {
            false
        }
    }
}

type Step = [Option<Event>; STEP_CAP];
struct Sequence {
    steps: [Step; MAX_LENGTH],
    sender: MessageSender<LiveEvent<'static>>,
    channel: u8,
    overflow: Queue<LiveEvent<'static>, 8>,
}

impl Sequence {
    fn new(sender: MessageSender<LiveEvent>, channel: u8) -> Self {
        Sequence {
            steps: [[None; STEP_CAP]; MAX_LENGTH],
            sender,
            channel,
            overflow: Queue::new(),
        }
    }

    fn clear_queue(&mut self) {
        self.overflow = Queue::new()
    }

    fn clear_step(&mut self, step: usize) {
        self.steps[step] = [None; STEP_CAP];
    }

    fn count(&self) -> u32 {
        let mut count = 0;
        for step in self.steps {
            for me in step {
                match me {
                    Some(_) => count += 1,
                    None => {}
                }
            }
        }
        return count;
    }

    fn insert(&mut self, event: Event) {
        for i in 0..STEP_CAP {
            match self.steps[event.ts.step as usize][i] {
                Some(e) if event.replaces(e) => {
                    self.steps[event.ts.step as usize][i] = Some(event);
                    return;
                }
                None => {
                    self.steps[event.ts.step as usize][i] = Some(event);
                    return;
                }
                _ => {}
            }
        }
    }

    fn emit(&mut self, ts1: TimeStamp, ts2: TimeStamp) {
        let step = self.steps[ts1.step as usize];

        let mut emitted_ts: Option<TimeStamp> = None;

        for maybe_event in step {
            match maybe_event {
                Some(Event { ts, midi_event }) => {
                    if ts >= ts1 && (ts <= ts2 || ts1 > ts2) {
                        if emitted_ts == None || Some(ts) == emitted_ts {
                            self.sender.try_send(midi_event).ok();
                            emitted_ts = Some(ts)
                        } else {
                            self.overflow.enqueue(midi_event).ok();
                        }
                    }
                }
                None => {}
            }
        }
        if ts1.step != ts2.step {
            let step = self.steps[ts2.step as usize];
            for maybe_event in step {
                match maybe_event {
                    Some(Event { ts, midi_event }) => {
                        if ts <= ts2 {
                            if emitted_ts == None || Some(ts) == emitted_ts {
                                self.sender.try_send(midi_event).ok();
                                emitted_ts = Some(ts)
                            } else {
                                self.overflow.enqueue(midi_event).ok();
                            }
                        }
                    }
                    None => {}
                }
            }
        }

        if emitted_ts == None {
            match self.overflow.dequeue() {
                Some(event) => {
                    self.sender.try_send(event).ok();
                }
                None => {}
            }
        }
    }
}

enum State {
    Playing,
    Stopped,
}

const PPQ: Ticks = 24;

#[derive(Copy, Clone)]
pub enum PlayerAction {
    Play,
    Tick,
    Stop,
    SetDivisor(u8),
    SetLength(u8),
    Insert(LiveEvent<'static>, u32, f32),
    ClearStep(u32),
    ClearPattern,
    ToggleMute,
    ToggleHold,  // Local clock will not update
    SoftRestart, // Local clock sat to 0
    Snap,        // Local clock syncronizes with global one
}

#[derive(Copy, Clone)]
pub enum PlayerMessage {
    Broadcast(PlayerAction),
    Action(u8, PlayerAction),
}

#[derive(Copy, Clone, PartialEq)]
enum Modal {
    True,
    WillBeFalse,
    WillBeTrue,
    False,
}

impl Not for Modal {
    type Output = Self;
    fn not(self) -> Self::Output {
        match self {
            Modal::True | Modal::WillBeTrue => Modal::WillBeFalse,
            _ => Modal::WillBeTrue,
        }
    }
}

impl Modal {
    fn change(self) -> Self {
        match self {
            Modal::WillBeFalse => Modal::False,
            Modal::WillBeTrue => Modal::True,
            _ => self,
        }
    }
}

pub struct Player {
    channel: u8,
    length: u8,
    clock: Ticks,
    global_clock: Ticks,
    state: State,
    sequence: Sequence,
    pps: Ticks,
    midi_sender: MessageSender<LiveEvent<'static>>,
    output_sender: MessageSender<OutputRequest>,
    mute: bool,
    hold: Modal,
    snap: bool,
    should_restart: bool,
}

impl Player {
    pub fn new(
        channel: u8,
        divisor: u32,
        midi_sender: MessageSender<LiveEvent<'static>>,
        output_sender: MessageSender<OutputRequest>,
    ) -> Self {
        Player {
            length: INITIAL_LENGTH,
            clock: 0,
            global_clock: 0,
            state: State::Stopped,
            sequence: Sequence::new(midi_sender.clone(), channel),
            pps: (PPQ * 4) / divisor,
            midi_sender,
            output_sender,
            channel,
            mute: false,
            hold: Modal::False,
            snap: false,
            should_restart: false,
        }
    }

    pub fn handle_message(&mut self, msg: PlayerMessage) {
        match msg {
            PlayerMessage::Action(ch, _) if ch != self.channel => {}
            PlayerMessage::Broadcast(action) | PlayerMessage::Action(_, action) => match action {
                PlayerAction::Play => self.play(),
                PlayerAction::Tick => self.tick(),
                PlayerAction::Stop => {
                    self.midi_sender
                        .try_send(make_all_notes_off(self.channel))
                        .ok();
                    self.stop()
                }
                PlayerAction::Insert(e, s, o) => self.insert(e, s, o),
                PlayerAction::SetDivisor(d) => {
                    if d > 0 {
                        self.pps = (PPQ * 4) / (d as u32)
                    }
                }
                PlayerAction::SetLength(length) => {
                    if length > 0 && length <= 32 {
                        self.length = length
                    }
                }
                PlayerAction::ClearStep(step) => self.sequence.clear_step(step as usize),
                PlayerAction::ClearPattern => {
                    self.sequence = Sequence::new(self.midi_sender.clone(), self.channel)
                }
                PlayerAction::ToggleMute => {
                    self.mute = !self.mute;
                    if self.mute {
                        self.midi_sender
                            .try_send(make_all_notes_off(self.channel))
                            .ok();
                    }
                }
                PlayerAction::ToggleHold => self.hold = !self.hold,
                PlayerAction::SoftRestart => self.should_restart = true,
                PlayerAction::Snap => {
                    self.hold = Modal::WillBeFalse;
                    self.snap = true;
                }
            },
        }
    }

    fn update_modals(&mut self, change: bool) {
        if change {
            self.hold = self.hold.change();
            if self.hold == Modal::False && self.snap {
                self.clock = self.global_clock;
                self.snap = false;
            }
            if self.should_restart {
                self.clock = 0;
                self.should_restart = false;
            }
        }
    }

    fn tick(&mut self) {
        match self.state {
            State::Playing => {
                let did_change =
                    self.get_step(self.global_clock) != self.get_step(self.global_clock + 1);
                self.global_clock += 1;
                let old_ts = self.get_ts();
                self.update_modals(did_change);
                self.clock += if self.hold == Modal::False { 1 } else { 0 };
                if !self.mute && self.hold == Modal::False {
                    self.sequence.emit(old_ts, self.get_ts())
                };
            }
            State::Stopped => {}
        }
    }

    fn play(&mut self) {
        match self.state {
            State::Playing => {}
            State::Stopped => self.state = State::Playing,
        }
    }

    fn stop(&mut self) {
        match self.state {
            State::Playing => {
                /*Issue all notes off*/
                self.clock = 0;
                self.global_clock = 0; // Assming that all channels receives this
                self.state = State::Stopped;
                self.hold = Modal::False;
                self.mute = false;
                self.sequence.clear_queue();
            }
            State::Stopped => {}
        }
    }

    fn get_step(&self, tick: Ticks) -> u32 {
        (tick / self.pps) % self.length as u32
    }

    fn get_ts(&self) -> TimeStamp {
        TimeStamp {
            step: self.get_step(self.clock),
            sub: (self.clock % self.pps) * SUBS_PER_STEP / self.pps,
        }
    }

    pub fn insert(&mut self, event: LiveEvent, step: u32, _offset: f32) {
        if step >= self.length as u32 {
            panic!();
        }
        let offset = if _offset > 1.0 { 1.0 } else { _offset };
        self.sequence.insert(Event {
            midi_event: event.to_static(),
            ts: TimeStamp {
                step,
                sub: (offset * SUBS_PER_STEP as f32) as u32,
            },
        });
    }
}
