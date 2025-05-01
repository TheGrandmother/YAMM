use midly::live::LiveEvent;

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
