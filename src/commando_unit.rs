pub enum Operation {
    Advance,
    Back,
    Audit,
    Restart,
    PlayerConf(u8),
    ModifierSwitch,
    Modify(u8, bool),
    Tie,
    Begin(u8),
    Commit,
    Abort,
}
use Operation::*;

#[derive(Copy, Clone, PartialEq)]
pub enum Input {
    MidiKey(u8),
    Play,
    Step,
    Rec,
}
use midly::num::u7;
use Input::*;

#[derive(Copy, Clone, PartialEq)]
pub enum CommandEvent {
    Empty,
    Up(Input),
    Down(Input),
}
use CommandEvent::*;

enum CommandState {
    Editing,
    Normal,
}

enum Progress {
    Invalid,
    Continue,
    Done(Operation),
}
use Progress::*;

pub struct CommandoUnit {
    state: CommandState,
    sequence: [CommandEvent; 3],
    comitted_key: Option<u8>,
}

impl CommandoUnit {
    pub fn new() -> Self {
        CommandoUnit {
            state: CommandState::Normal,
            sequence: [Empty; 3],
            comitted_key: None,
        }
    }

    fn append(&mut self, e: CommandEvent) {
        for i in 0..self.sequence.len() {
            match self.sequence[i] {
                Empty => {
                    self.sequence[i] = e;
                    return;
                }
                _ => {}
            }
        }
    }

    fn reset(&mut self) {
        self.sequence = [Empty; 3]
    }

    pub fn handle_event(&mut self, event: CommandEvent) -> Option<Operation> {
        self.append(event);
        match self.interpret_sequence() {
            Progress::Invalid => {
                self.reset();
                None
            }
            Progress::Done(op) => {
                match op {
                    Begin(_) => self.state = CommandState::Editing,
                    Commit | Modify(_, true) | Abort => self.state = CommandState::Normal,
                    _ => {}
                };
                self.reset();
                Some(op)
            }
            Progress::Continue => None,
        }
    }

    fn interpret_sequence(&mut self) -> Progress {
        match self.state {
            CommandState::Editing => match self.sequence {
                [Up(MidiKey(key)), Empty, Empty] if Some(key) == self.comitted_key => Done(Commit),
                [Down(_), Empty, Empty] => Continue,
                [Down(_), Down(_), Empty] => Continue,
                [Down(Step), Up(Step), Empty] => Done(Tie),
                [Down(Rec), Up(Rec), Empty] => Done(ModifierSwitch),
                [Down(MidiKey(k1)), Up(MidiKey(k2)), Empty] if k1 == k2 => Done(Modify(k1, false)),
                [Down(MidiKey(k1)), Up(MidiKey(k2)), hmm] if Some(k2) == self.comitted_key => {
                    match hmm {
                        Empty => Continue,
                        Up(MidiKey(k3)) if k3 == k1 => Done(Modify(k1, true)),
                        Down(MidiKey(k3)) if k3 == k2 => {
                            self.sequence = [Down(MidiKey(k1)), Empty, Empty];
                            Continue
                        }
                        Down(MidiKey(_)) => Done(Abort),
                        _ => Invalid,
                    }
                    //
                }
                _ => Invalid,
            },
            CommandState::Normal => match self.sequence {
                [Down(MidiKey(key)), Empty, Empty] => {
                    self.comitted_key = Some(key);
                    Done(Begin(key))
                }
                [Down(_), Empty, Empty] => Continue,
                [Down(Play), Up(Play), Empty] => Done(Audit),
                [Down(Step), Up(Step), Empty] => Done(Advance),
                [Down(_), Down(_), Empty] => Continue,
                [Down(i), Down(j), Up(k)] if j == k => match i {
                    Play => match j {
                        MidiKey(key) => Done(PlayerConf(key)),
                        Step => Done(Restart),
                        _ => Invalid,
                    },
                    Step => match j {
                        Rec => Done(Back),
                        _ => Invalid,
                    },
                    _ => Invalid,
                },
                _ => Invalid,
            },
        }
    }
}
