use midly::live::LiveEvent;
use midly::MidiMessage;

// Not exhaustive
pub fn equivalent(m1: MidiMessage, m2: MidiMessage) -> bool {
    match (m1, m2) {
        (MidiMessage::NoteOff { key: k1, .. }, MidiMessage::NoteOff { key: k2, .. }) => k1 == k2,
        (MidiMessage::NoteOn { key: k1, .. }, MidiMessage::NoteOn { key: k2, .. }) => k1 == k2,
        _ => false,
    }
}

pub fn event_length(event: LiveEvent) -> usize {
    1 + match event {
        LiveEvent::Midi { message, .. } => match message {
            midly::MidiMessage::NoteOff { .. } => 2,
            midly::MidiMessage::NoteOn { .. } => 2,
            midly::MidiMessage::Aftertouch { .. } => 2,
            midly::MidiMessage::Controller { .. } => 2,
            midly::MidiMessage::ProgramChange { .. } => 1,
            midly::MidiMessage::ChannelAftertouch { .. } => 1,
            midly::MidiMessage::PitchBend { .. } => 2,
        },
        LiveEvent::Common(system_common) => match system_common {
            midly::live::SystemCommon::SysEx(u7s) => u7s.len(),
            midly::live::SystemCommon::MidiTimeCodeQuarterFrame(..) => 1,
            midly::live::SystemCommon::SongPosition(_) => 2,
            midly::live::SystemCommon::SongSelect(_) => 1,
            midly::live::SystemCommon::TuneRequest => 0,
            midly::live::SystemCommon::Undefined(_, u7s) => u7s.len() + 1,
        },
        LiveEvent::Realtime(_) => 0,
    }
}
