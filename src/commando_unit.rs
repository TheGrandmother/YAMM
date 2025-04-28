pub enum Operation {
    Advance,
    Back,
    Audit,
    Exit,
    Restart,
    PlayerConf(u8),
    ModeSwitch,
    Modify(u8),
    Tie,
    Begin(u8),
    Abort(u8),
    Commit(u8),
}

#[derive(Copy, Clone, PartialEq)]
pub enum Input {
    Key(u8),
    Play,
    Step,
    Rec,
}

#[derive(Copy, Clone, PartialEq)]
pub enum CommandEvent {
    Up(Input),
    Down(Input),
}

enum CommandState {
    Editing,
    Normal,
}

enum Progress {
    Invalid,
    Continue,
    Done(Operation),
}

pub struct CommandoUnit {
    state: CommandState,
    sequence: [Option<CommandEvent>; 3],
}

impl CommandoUnit {
    pub fn new() -> Self {
        CommandoUnit {
            state: CommandState::Normal,
            sequence: [None; 3],
        }
    }

    fn append(&mut self, e: CommandEvent) {
        for i in 0..self.sequence.len() {
            match self.sequence[i] {
                None => {
                    self.sequence[i] = Some(e);
                    return;
                }
                _ => {}
            }
        }
    }

    fn reset(&mut self) {
        self.sequence = [None; 3]
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
                    Operation::Commit(_) => self.state = CommandState::Normal,
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
                [Some(CommandEvent::Up(Input::Key(key))), None, None] => {
                    Progress::Done(Operation::Commit(key))
                }
                [Some(CommandEvent::Down(_)), None, None] => Progress::Continue,
                [Some(CommandEvent::Down(i)), Some(CommandEvent::Up(j)), None] if i == j => match i
                {
                    Input::Step => Progress::Done(Operation::Tie),
                    Input::Rec => Progress::Done(Operation::ModeSwitch),
                    Input::Key(key) => Progress::Done(Operation::Modify(key)),
                    _ => Progress::Invalid,
                },
                _ => Progress::Invalid,
            },
            CommandState::Normal => match self.sequence {
                [Some(CommandEvent::Down(Input::Key(key))), None, None] => {
                    Progress::Done(Operation::Begin(key))
                }
                [Some(CommandEvent::Down(_)), None, None] => Progress::Continue,
                [Some(CommandEvent::Down(i)), Some(CommandEvent::Up(j)), None] if i == j => match i
                {
                    Input::Play => Progress::Done(Operation::Audit),
                    Input::Step => Progress::Done(Operation::Advance),
                    _ => Progress::Invalid,
                },
                [Some(CommandEvent::Down(_)), Some(CommandEvent::Down(_)), None] => {
                    Progress::Continue
                }
                [Some(CommandEvent::Down(i)), Some(CommandEvent::Down(j)), Some(CommandEvent::Up(k))]
                    if j == k =>
                {
                    match i {
                        Input::Play => match j {
                            Input::Key(key) => Progress::Done(Operation::PlayerConf(key)),
                            Input::Step => Progress::Done(Operation::Restart),
                            Input::Rec => Progress::Done(Operation::Exit),
                            _ => Progress::Invalid,
                        },
                        Input::Step => match j {
                            Input::Rec => Progress::Done(Operation::Back),
                            _ => Progress::Invalid,
                        },
                        _ => Progress::Invalid,
                    }
                }
                _ => Progress::Invalid,
            },
        }
    }
}
