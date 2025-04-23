use midly::live::LiveEvent;
use midly::num::{u4, u7};
use midly::MidiMessage;

use crate::midi_master::MessageSender;
use crate::outs::{Cv, Gate, OutputRequest};

#[derive(Copy, Clone, PartialEq)]
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

    fn get_vel_mapping(&mut self, port: Port) -> Option<Port> {
        return self.vel_mappings[port.index()];
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

    pub fn two_duo() -> Self {
        Config {
            drum_channel: 5.into(),
            port_mappings: [
                Some([Some(Port::A), Some(Port::B), None, None]),
                Some([None, None, Some(Port::C), Some(Port::D)]),
                None,
                None,
            ],
            vel_mappings: [None; 4],
            aftertouch: None,
        }
    }

    pub fn two_mono() -> Self {
        Config {
            drum_channel: 5.into(),
            port_mappings: [
                Some([Some(Port::A), None, None, None]),
                Some([None, None, Some(Port::C), None]),
                None,
                None,
            ],
            vel_mappings: [Some(Port::B), None, Some(Port::D), None],
            aftertouch: None,
        }
    }

    pub fn one_duo() -> Self {
        Config {
            drum_channel: 5.into(),
            port_mappings: [
                Some([Some(Port::A), Some(Port::B), None, None]),
                None,
                None,
                None,
            ],
            vel_mappings: [Some(Port::C), Some(Port::D), None, None],
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
    key: u7,
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

    fn add(&mut self, msg: MidiMessage, key: u7, port: Port) {
        self.insertions += 1;
        let tm = TrackedMessage {
            msg,
            ts: self.insertions,
            port,
            key,
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
        match self.active_messages[oldest_index] {
            Some(TrackedMessage { port, .. }) => {
                self.port_age[port.index()] = 0;
            }
            None => {}
        }
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

    fn find_newest_by_port(&mut self, port: Port) -> Option<TrackedMessage> {
        let mut newest_message = None;
        let mut newest_ts = 0;
        for tm in self.active_messages {
            match tm {
                Some(TrackedMessage {
                    ts, port: port_, ..
                }) => {
                    if port_ == port && ts > newest_ts {
                        newest_message = tm;
                        newest_ts = ts;
                    }
                }
                None => {}
            }
        }
        return newest_message;
    }

    fn remove(&mut self, lifted_key: u7) -> Option<Port> {
        for i in 0..CAPACITY {
            let tm = self.active_messages[i];
            match tm {
                Some(TrackedMessage { key, port, .. }) if key == lifted_key => {
                    self.active_messages[i] = None;
                    self.port_age[port.index()] = 0;
                    return Some(port);
                }
                _ => {}
            }
        }
        return None;
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
            MidiMessage::NoteOff { key, vel } => self.on_note_off(key, vel),
            _ => {}
        }
    }

    fn on_note_on(&mut self, key: u7, vel: u7, msg: MidiMessage, ports: PortMapping) {
        if self.tracked_messages.count() >= CAPACITY {
            self.tracked_messages.remove_oldest()
        }

        match self.tracked_messages.find_port(ports) {
            None => {}
            Some(port) => {
                self.tracked_messages.add(msg, key, port);
                self.io_sender.try_send(port.make_set_note(key)).ok();
                self.io_sender.try_send(port.make_gate_on()).ok();
                match self.config.get_vel_mapping(port) {
                    Some(port) => self
                        .io_sender
                        .try_send(port.make_set_val(vel.as_int() as f32 / 127.0))
                        .ok(),
                    None => None,
                };
            }
        }
    }

    fn on_note_off(&mut self, key: u7, _vel: u7) {
        match self.tracked_messages.remove(key) {
            Some(port) => match self.tracked_messages.find_newest_by_port(port) {
                Some(tm) => {
                    self.io_sender.try_send(port.make_set_note(tm.key)).ok();
                }
                None => {
                    self.io_sender.try_send(port.make_gate_off()).ok();
                }
            },
            None => {}
        }
    }
}
