use core::cmp::Ordering;

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
const MAX_LENGTH: usize = 16;

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

// Abstract away the midi implementation.
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

    pub fn insert(&mut self, event: Event) {
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

    pub fn emit(&mut self, ts1: TimeStamp, ts2: TimeStamp) {
        let step = self.steps[ts1.step as usize];

        let mut emitted_ts: Option<TimeStamp> = None;

        for maybe_event in step {
            match maybe_event {
                Some(Event { ts, midi_event }) => {
                    if ts >= ts1 && ts <= ts2 {
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
                        if emitted_ts == None || Some(ts) == emitted_ts {
                            self.sender.try_send(midi_event).ok();
                            emitted_ts = Some(ts)
                        } else {
                            self.overflow.enqueue(midi_event).ok();
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

pub enum PlayerAction {
    Play,
    Tick,
    Stop,
    Insert(LiveEvent<'static>, u32, f32),
}

pub struct Player {
    channel: u8,
    length: u32,
    clock: Ticks,
    state: State,
    sequence: Sequence,
    pps: Ticks,
    midi_sender: MessageSender<LiveEvent<'static>>,
    output_sender: MessageSender<OutputRequest>,
}

impl Player {
    pub fn new(
        channel: u8,
        divisor: u32,
        length: u8,
        midi_sender: MessageSender<LiveEvent<'static>>,
        output_sender: MessageSender<OutputRequest>,
    ) -> Self {
        Player {
            length: if (length as usize) < MAX_LENGTH {
                length.into()
            } else {
                MAX_LENGTH as u32 - 1
            },
            clock: 0,
            state: State::Stopped,
            sequence: Sequence::new(midi_sender.clone(), channel),
            pps: (PPQ * 4) / divisor,
            midi_sender,
            output_sender,
            channel,
        }
    }

    pub fn handle_message(&mut self, action: PlayerAction) {
        match action {
            PlayerAction::Play => self.play(),
            PlayerAction::Tick => self.tick(),
            PlayerAction::Stop => {
                self.midi_sender
                    .try_send(make_all_notes_off(self.channel))
                    .ok();
                self.stop()
            }
            PlayerAction::Insert(e, s, o) => self.insert(e, s, o),
        }
    }

    pub fn tick(&mut self) {
        match self.state {
            State::Playing => {
                let old_ts = self.get_ts();
                self.clock += 1;
                self.sequence.emit(old_ts, self.get_ts());
            }
            State::Stopped => {}
        }
    }

    pub fn play(&mut self) {
        match self.state {
            State::Playing => {}
            State::Stopped => self.state = State::Playing,
        }
    }

    pub fn stop(&mut self) {
        match self.state {
            State::Playing => {
                /*Issue all notes off*/
                self.clock = 0;
                self.state = State::Stopped;
                self.sequence.clear_queue();
            }
            State::Stopped => {}
        }
    }

    fn get_ts(&self) -> TimeStamp {
        TimeStamp {
            step: (self.clock / self.pps) % self.length,
            sub: (self.clock % self.pps) * SUBS_PER_STEP / self.pps,
        }
    }

    pub fn insert(&mut self, event: LiveEvent, step: u32, _offset: f32) {
        if step >= self.length {
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
