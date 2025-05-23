use midly::live::LiveEvent;
use midly::MidiMessage;

use crate::commando_unit::Operation;
use crate::midi_master::MessageSender;
use crate::outs::{Gate, OutputRequest};
use crate::player::{PlayerAction, PlayerMessage, INITIAL_LENGTH, MAX_LENGTH};
use crate::utils::key_names::{is_white, key_to_note, to_deg, Note};

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
            vel: 0.50,
            shift: 0.0,
        }
    }

    fn tie(self) -> Self {
        EventProps { gate: None, ..self }
    }
}

pub struct Programmer {
    channel: u8,
    step: u8,
    length: u8,
    mode: Mode,
    modifier: Modifier,
    props: Option<EventProps>,
    lengths: [u8; 5],
    player_sender: MessageSender<PlayerMessage>,
    output_sender: MessageSender<OutputRequest>,
}

impl Programmer {
    pub fn new(
        player_sender: MessageSender<PlayerMessage>,
        output_sender: MessageSender<OutputRequest>,
    ) -> Self {
        Programmer {
            channel: 0,
            step: 0,
            length: INITIAL_LENGTH,
            lengths: [INITIAL_LENGTH; 5],
            mode: Mode::Normal,
            modifier: Modifier::Gate,
            player_sender,
            props: None,
            output_sender,
        }
    }

    pub fn handle_operation(&mut self, op: Operation) {
        match self.mode {
            Mode::Insert => {
                self.output_sender
                    .try_send(OutputRequest::GateOn(Gate::Accent))
                    .ok();
            }
            Mode::Normal => {
                self.output_sender
                    .try_send(OutputRequest::GateOff(Gate::Accent))
                    .ok();
            }
        }
        match self.mode {
            Mode::Insert => match op {
                Operation::ModifierSwitch => self.switch_modifier(),
                Operation::Modify(key, commit) => {
                    self.modify(key);
                    if commit {
                        self.commit()
                    }
                }
                Operation::Tie => match self.props {
                    Some(e) => self.props = Some(e.tie()),
                    None => {}
                },
                Operation::Commit => self.commit(),
                Operation::Abort => {
                    self.mode = Mode::Normal;
                    self.props = None;
                }
                _ => {}
            },
            Mode::Normal => match op {
                Operation::Advance => {
                    self.advance();
                }
                Operation::Back if self.step > 0 => self.step -= 1,
                Operation::Restart => self.step = 0,
                Operation::Audit => {}
                Operation::PlayerConf(key) => {
                    self.set_conf(key);
                }
                Operation::Begin(key) => {
                    self.mode = Mode::Insert;
                    self.modifier = Modifier::Gate;
                    self.props = Some(EventProps::new(key))
                }
                Operation::ClearStep => self.send_action(PlayerAction::ClearStep(self.step as u32)),
                Operation::ClearPattern => self.send_action(PlayerAction::ClearPattern),
                _ => {}
            },
        }
        match self.mode {
            Mode::Insert => {
                self.output_sender
                    .try_send(OutputRequest::GateOn(Gate::Accent))
                    .ok();
            }
            Mode::Normal => {
                self.output_sender
                    .try_send(OutputRequest::GateOff(Gate::Accent))
                    .ok();
            }
        }
    }

    fn advance(&mut self) {
        if self.step < self.length - 1 {
            self.step += 1;
            self.output_sender
                .try_send(OutputRequest::Flash(Gate::Clock))
                .ok();
        } else {
            self.output_sender
                .try_send(OutputRequest::Flash(Gate::Stop))
                .ok();
        }
    }

    fn commit(&mut self) {
        self.emit();
        self.mode = Mode::Normal;
        self.props = None;
        self.advance();
    }

    fn emit(&mut self) {
        match self.props {
            Some(EventProps {
                key,
                gate,
                vel,
                shift,
            }) => {
                self.send_action(PlayerAction::Insert(
                    LiveEvent::Midi {
                        channel: self.channel.into(),
                        message: MidiMessage::NoteOn {
                            key: key.into(),
                            vel: ((vel * 127.0) as u8).into(),
                        },
                    },
                    self.step.into(),
                    shift,
                ));
                match gate {
                    Some(g) => {
                        self.send_action(PlayerAction::Insert(
                            LiveEvent::Midi {
                                channel: self.channel.into(),
                                message: MidiMessage::NoteOff {
                                    key: key.into(),
                                    vel: ((vel * 127.0) as u8).into(),
                                },
                            },
                            self.step.into(),
                            shift + g,
                        ));
                    }
                    None => {}
                }
            }
            None => panic!(),
        }
    }

    fn set_conf(&mut self, key: u8) {
        match key_to_note(key) {
            Note::C => self.send_action(PlayerAction::SetDivisor(1)),
            Note::Db => self.channel = 0,
            Note::D => self.send_action(PlayerAction::SetDivisor(2)),
            Note::Eb => self.channel = 1,
            Note::E => self.send_action(PlayerAction::SetDivisor(4)),
            Note::F => self.send_action(PlayerAction::SetDivisor(8)),
            Note::Gb => self.channel = 2,
            Note::G => self.send_action(PlayerAction::SetDivisor(16)),
            Note::Ab => self.channel = 3,
            Note::A => {
                self.length = if self.length < (MAX_LENGTH - 4) as u8 {
                    self.length + 4
                } else {
                    self.length
                };
                self.lengths[self.channel as usize] = self.length;
                self.send_action(PlayerAction::SetLength(self.length));
            }
            Note::Bb => self.channel = 4,
            Note::B => {
                self.length = if self.length > 4 {
                    self.length - 4
                } else {
                    self.length
                };
                self.lengths[self.channel as usize] = self.length;
                self.send_action(PlayerAction::SetLength(self.length))
            }
        }
        self.step = 0;
        self.length = self.lengths[self.channel as usize];
    }

    fn send_action(&mut self, action: PlayerAction) {
        self.player_sender
            .try_send(PlayerMessage::Action(self.channel, action))
            .ok();
    }

    fn modify(&mut self, key: u8) {
        match self.props {
            Some(mut p) => {
                let mut diff = key as i8 - p.key as i8;
                diff = if diff > 0 { diff } else { diff * -1 };
                if diff < 6 {
                    let value = match diff {
                        0 => return,
                        1 => 0.1,
                        2 => 0.25,
                        3 => 0.5,
                        4 => 0.75,
                        5 => 0.9,
                        _ => return,
                    };
                    match self.modifier {
                        Modifier::Gate => p.gate = Some(value),
                        Modifier::Vel => p.vel = value,
                        Modifier::Timing => p.shift = value,
                    }
                } else {
                    match diff {
                        6 => p.key = if p.key < 127 - 12 { p.key + 12 } else { p.key },
                        7 => p.key = if p.key >= 12 { p.key - 12 } else { p.key },
                        _ => return,
                    };
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
