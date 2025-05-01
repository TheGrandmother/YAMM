#[repr(u8)]
pub enum Note {
    C,
    Db,
    D,
    Eb,
    E,
    F,
    Gb,
    G,
    Ab,
    A,
    Bb,
    B,
}

pub fn is_white(key: u8) -> bool {
    match key_to_note(key) {
        Note::Db => false,
        Note::Eb => false,
        Note::Gb => false,
        Note::Ab => false,
        Note::Bb => false,
        _ => true,
    }
}

pub fn key_to_note(key: u8) -> Note {
    match key % 12 {
        0 => Note::C,
        1 => Note::Db,
        2 => Note::D,
        3 => Note::Eb,
        4 => Note::E,
        5 => Note::F,
        6 => Note::Gb,
        7 => Note::G,
        8 => Note::Ab,
        9 => Note::A,
        10 => Note::Bb,
        11 => Note::B,
        _ => Note::C,
    }
}

pub fn to_deg(key: u8) -> i8 {
    match key_to_note(key) {
        Note::C => 0,
        Note::D => 1,
        Note::E => 2,
        Note::F => 3,
        Note::G => 4,
        Note::A => 5,
        Note::B => 6,
        _ => 0,
    }
}
