pub enum Operation {
    Advance,
    Back,
    Audit,
    Restart,
    PlayerConf(u8),
    ModifierSwitch,
    Modify(u8),
    Tie,
    Begin(u8),
    Commit,
}

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
                    Operation::Begin(_) => self.state = CommandState::Editing,
                    Operation::Commit => self.state = CommandState::Normal,
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
                [Up(MidiKey(key)), Empty, Empty] if Some(key) == self.comitted_key => {
                    Done(Operation::Commit)
                }
                [Down(_), Empty, Empty] => Continue,
                [Down(_), Down(_), Empty] => Continue,
                [Down(Step), Up(Step), Empty] => Done(Operation::Tie),
                [Down(Rec), Up(Rec), Empty] => Done(Operation::ModifierSwitch),
                [Down(MidiKey(key1)), Up(MidiKey(key2)), Empty] if key1 == key2 => {
                    Done(Operation::Modify(key1))
                }
                _ => Invalid,
            },
            CommandState::Normal => match self.sequence {
                [Down(MidiKey(key)), Empty, Empty] => {
                    self.comitted_key = Some(key);
                    Done(Operation::Begin(key))
                }
                [Down(_), Empty, Empty] => Continue,
                [Down(Play), Up(Play), Empty] => Done(Operation::Audit),
                [Down(Step), Up(Step), Empty] => Done(Operation::Advance),
                [Down(_), Down(_), Empty] => Continue,
                [Down(i), Down(j), Up(k)] if j == k => match i {
                    Play => match j {
                        MidiKey(key) => Done(Operation::PlayerConf(key)),
                        Step => Done(Operation::Restart),
                        _ => Invalid,
                    },
                    Step => match j {
                        Rec => Done(Operation::Back),
                        _ => Invalid,
                    },
                    _ => Invalid,
                },
                _ => Invalid,
            },
        }
    }
}
