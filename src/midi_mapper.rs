use midly::live::LiveEvent;
use midly::num::{u4, u7};
use midly::MidiMessage;

use crate::midi_master::MessageSender;
use crate::outs::{Cv, Gate, OutputRequest};

#[derive(Copy, Clone)]
enum Port {
    A,
    B,
    C,
    D,
}

impl Port {
    fn index(self) -> usize {
        match self {
            Port::A => 0,
            Port::B => 1,
            Port::C => 2,
            Port::D => 3,
        }
    }

    fn make_set_note(self, key: u7) -> OutputRequest {
        return OutputRequest::SetNote(self.to_output_cv(), key.into());
    }

    fn make_set_val(self, val: f32) -> OutputRequest {
        return OutputRequest::SetVal(self.to_output_cv(), val);
    }

    fn make_gate_on(self) -> OutputRequest {
        return OutputRequest::GateOn(self.to_output_gate());
    }

    fn make_gate_off(self) -> OutputRequest {
        return OutputRequest::GateOff(self.to_output_gate());
    }

    fn to_output_cv(self) -> Cv {
        match self {
            Port::A => Cv::A,
            Port::B => Cv::B,
            Port::C => Cv::C,
            Port::D => Cv::D,
        }
    }

    fn to_output_gate(self) -> Gate {
        match self {
            Port::A => Gate::GateA,
            Port::B => Gate::GateB,
            Port::C => Gate::GateC,
            Port::D => Gate::GateD,
        }
    }
}

type PortMapping = [Option<Port>; 4];

enum ChannelType {
    Drumms,
    Pitch([Option<Port>; 4]),
    None,
}

pub struct Config {
    drum_channel: u4,
    port_mappings: [Option<PortMapping>; 4], // Maps channels to ports
    vel_mappings: [Option<Port>; 4],         // Maps pitch ports to vel ports by index
    aftertouch: Option<Port>,
}

impl Config {
    fn get_channel_type(&mut self, ch: u4) -> ChannelType {
        if ch > 4 {
            return ChannelType::None;
        }
        if ch == self.drum_channel {
            return ChannelType::Drumms;
        }
        match self.port_mappings[usize::from(ch.as_int())] {
            Some(ports) => ChannelType::Pitch(ports),
            None => ChannelType::None,
        }
    }

    pub fn four_poly() -> Self {
        Config {
            drum_channel: 5.into(),
            port_mappings: [
                Some([Some(Port::A), Some(Port::B), Some(Port::C), Some(Port::D)]),
                None,
                None,
                None,
            ],
            vel_mappings: [None; 4],
            aftertouch: None,
        }
    }
}

const CAPACITY: usize = 16;

#[derive(Copy, Clone)]
struct TrackedMessage {
    msg: MidiMessage,
    ts: u32,
    port: Port,
}

struct TrackedSet {
    active_messages: [Option<TrackedMessage>; CAPACITY],
    port_age: [u32; 4],
    insertions: u32,
}

impl TrackedSet {
    fn new() -> Self {
        Self {
            active_messages: [None; CAPACITY],
            port_age: [0; 4],
            insertions: 0,
        }
    }

    fn add(&mut self, msg: MidiMessage, port: Port) {
        self.insertions += 1;
        let tm = TrackedMessage {
            msg,
            ts: self.insertions,
            port,
        };

        self.port_age[tm.port.index()] += 1;
        for i in 0..CAPACITY {
            match &self.active_messages[i] {
                Some(_) => {}
                None => {
                    self.active_messages[i] = Some(tm);
                    return;
                }
            }
        }
    }

    fn count(&mut self) -> usize {
        let mut count = 0;
        for i in 0..CAPACITY {
            match self.active_messages[i] {
                Some(_) => count += 1,
                None => {}
            }
        }
        return count;
    }

    fn remove_oldest(&mut self) {
        let mut oldest_index = 0;
        let mut oldest_ts = 0xffffffff;
        for i in 0..CAPACITY {
            match &self.active_messages[i] {
                Some(tm) => {
                    if tm.ts < oldest_ts {
                        oldest_index = i;
                        oldest_ts = tm.ts;
                    }
                }
                None => return,
            }
        }
        self.active_messages[oldest_index] = None;
    }

    fn find_port(&mut self, assigned_ports: PortMapping) -> Option<Port> {
        let mut oldest_port = None;
        let mut lowest_count = 0xffff_ffff;
        for port in assigned_ports {
            match port {
                Some(p) => {
                    let allocation_count = self.port_age[p.index()];
                    if allocation_count == 0 {
                        return Some(p);
                    } else if allocation_count < lowest_count {
                        oldest_port = Some(p);
                        lowest_count = allocation_count;
                    }
                }
                None => {}
            }
        }
        return oldest_port;
    }
}

pub struct MidiMapper {
    tracked_messages: TrackedSet,
    config: Config,
    io_sender: MessageSender<OutputRequest>,
}

impl MidiMapper {
    pub fn new(config: Config, io_sender: MessageSender<OutputRequest>) -> Self {
        Self {
            tracked_messages: TrackedSet::new(),
            config,
            io_sender,
        }
    }

    pub fn handle_message(&mut self, msg: LiveEvent) {
        match msg {
            LiveEvent::Midi { channel, message } => match self.config.get_channel_type(channel) {
                ChannelType::Drumms => {}
                ChannelType::Pitch(ports) => self.handle_pitched_channel(message, ports),
                ChannelType::None => {}
            },
            LiveEvent::Common(_) => {}
            LiveEvent::Realtime(_) => {}
        }
    }
    fn handle_pitched_channel(&mut self, msg: MidiMessage, ports: PortMapping) {
        match msg {
            MidiMessage::NoteOn { key, vel } => self.on_note_on(key, vel, msg, ports),
            _ => {}
        }
    }

    fn on_note_on(&mut self, key: u7, _vel: u7, msg: MidiMessage, ports: PortMapping) {
        if self.tracked_messages.count() >= CAPACITY {
            self.tracked_messages.remove_oldest()
        }

        match self.tracked_messages.find_port(ports) {
            None => {}
            Some(port) => {
                self.tracked_messages.add(msg, port);
                self.io_sender.try_send(port.make_set_note(key)).ok();
                self.io_sender.try_send(port.make_gate_on()).ok();
            }
        }
    }
}
