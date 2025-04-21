use midly::live::LiveEvent;
use midly::num::{u4, u7};
use midly::MidiMessage;

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
}

struct TrackedMessage {
    msg: MidiMessage,
    ts: u32,
    port: Port,
}

enum ChannelType {
    Drumms,
    Pitch([Option<Port>; 4]),
    None,
}

struct Config {
    drum_channel: u4,
    port_mappings: [Option<[Option<Port>; 4]>; 4], // Maps channels to ports
    vel_mappings: [Option<Port>; 4],               // Maps pitch ports to vel ports
    aftertouch: Option<Port>,
}

impl Config {
    fn is_channel_active(self, ch: u4) -> ChannelType {
        if ch > 4 {
            return ChannelType::None;
        }
        if ch == self.drum_channel {
            return ChannelType::Drumms;
        }
        let ports = self.port_mappings[usize::from(ch.as_int())];
        if ports.is_some() {
            return ChannelType::Pitch(ports.unwrap());
        }
        return ChannelType::None;
    }
}

const CAPCAITY: usize = 16;

pub struct MidiMapper {
    active_messages: [Option<TrackedMessage>; CAPCAITY],
    tracked_count: usize,
    order: u32,
    config: Config,
}

impl MidiMapper {
    pub fn handle_message(self, msg: LiveEvent) {
        match msg {
            LiveEvent::Midi { channel, message } => match self.config.is_channel_active(channel) {
                ChannelType::Drumms => todo!(),
                ChannelType::Pitch(_) => todo!(),
                ChannelType::None => {}
            },
            LiveEvent::Common(_) => {}
            LiveEvent::Realtime(_) => {}
        }
    }
    fn handle_pitched_channel(self, ch: u4, msg: MidiMessage) {
        match msg {
            MidiMessage::NoteOn { key, vel } => self.on_note_on(ch, key, vel),
            _ => {}
        }
    }

    fn on_note_on(self, ch: u4, key: u7, vel: u7) {
        if self.tracked_count > CAPCAITY { /*remove oldest*/ }
        /*find oldest port*/
    }
}
