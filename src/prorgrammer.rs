use midly::live::LiveEvent;
use midly::MidiMessage;

use crate::commando_unit::Operation;
use crate::midi_master::MessageSender;
use crate::player::PlayerAction;
use crate::utils::key_names::{key_to_note, Note};

enum Mode {
    Insert,
    Normal,
}

enum Modifier {
    Gate,
    Vel,
    Timing,
}

#[derive(Copy, Clone)]
struct EventProps {
    key: u8,
    gate: Option<f32>,
    vel: f32,
    shift: f32,
}

impl EventProps {
    fn new(key: u8) -> Self {
        EventProps {
            key,
            gate: Some(0.5),
            vel: 0.75,
            shift: 0.0,
        }
    }

    fn tie(&mut self) {
        self.gate = None
    }
}

pub struct Programmer {
    channel: u8,
    step: u8,
    length: u8,
    mode: Mode,
    modifier: Modifier,
    props: Option<EventProps>,
    player_sender: MessageSender<PlayerAction>,
}

impl Programmer {
    pub fn new(player_sender: MessageSender<PlayerAction>) -> Self {
        Programmer {
            channel: 0,
            step: 0,
            length: 4,
            mode: Mode::Normal,
            modifier: Modifier::Vel,
            player_sender,
            props: None,
        }
    }

    pub fn handle_operation(&mut self, op: Operation) {
        match self.mode {
            Mode::Insert => match op {
                Operation::ModifierSwitch => {}
                Operation::Modify(key) => self.modify(key),
                Operation::Tie => match self.props {
                    Some(mut e) => e.tie(),
                    None => {}
                },
                Operation::Commit => {
                    self.emit();
                    self.mode = Mode::Normal;
                    self.props = None;
                }
                _ => {}
            },
            Mode::Normal => match op {
                Operation::Advance if self.step < self.length - 1 => self.step += 1,
                Operation::Back if self.step > 0 => self.step -= 1,
                Operation::Restart => self.step = 0,
                Operation::Audit => {}
                Operation::PlayerConf(key) => self.set_conf(key),
                Operation::Begin(key) => {
                    self.mode = Mode::Insert;
                    self.modifier = Modifier::Vel;
                    self.props = Some(EventProps::new(key))
                }
                _ => {}
            },
        }
    }

    fn emit(&mut self) {
        match self.props {
            Some(EventProps {
                key,
                gate,
                vel,
                shift,
            }) => {
                self.player_sender
                    .try_send(PlayerAction::Insert(
                        LiveEvent::Midi {
                            channel: self.channel.into(),
                            message: MidiMessage::NoteOn {
                                key: key.into(),
                                vel: ((vel * 127.0) as u8).into(),
                            },
                        },
                        self.step.into(),
                        shift,
                    ))
                    .ok();
                match gate {
                    Some(g) => {
                        self.player_sender
                            .try_send(PlayerAction::Insert(
                                LiveEvent::Midi {
                                    channel: self.channel.into(),
                                    message: MidiMessage::NoteOff {
                                        key: key.into(),
                                        vel: ((vel * 127.0) as u8).into(),
                                    },
                                },
                                self.step.into(),
                                shift + g,
                            ))
                            .ok();
                    }
                    None => {}
                }
            }
            None => panic!(),
        }
    }

    fn set_conf(&mut self, key: u8) {
        match key_to_note(key) {
            Note::C => self.channel = 0,
            Note::Db => {}
            Note::D => self.channel = 1,
            Note::Eb => {}
            Note::E => self.channel = 2,
            Note::F => self.channel = 3,
            Note::Gb => {}
            Note::G => self.channel = 4,
            Note::Ab => {}
            Note::A => {}
            Note::Bb => {}
            Note::B => {}
        }
    }

    fn modify(&mut self, key: u8) {
        match self.props {
            Some(mut p) => {
                let mut diff = key_to_note(key) as i8 - key_to_note(p.key) as i8;
                diff = if diff > 0 { diff } else { diff * -1 };
                let value = match diff {
                    0 => return,
                    1 => 0.0,
                    2 => 0.25,
                    3 => 0.5,
                    4 => 0.75,
                    5 => 1.0,
                    _ => return,
                };
                match self.modifier {
                    Modifier::Gate => p.gate = Some(value),
                    Modifier::Vel => p.vel = value,
                    Modifier::Timing => p.shift = value,
                }
                self.props = Some(p)
            }
            None => {}
        }
    }

    fn switch_modifier(&mut self) {
        match self.modifier {
            Modifier::Gate => self.modifier = Modifier::Vel,
            Modifier::Vel => self.modifier = Modifier::Timing,
            Modifier::Timing => self.modifier = Modifier::Gate,
        }
    }
}
