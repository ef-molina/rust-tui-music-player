pub fn previous_char_boundary(text: &str, cursor: usize) -> usize {
    text[..cursor]
        .char_indices()
        .next_back()
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

pub fn next_char_boundary(text: &str, cursor: usize) -> usize {
    if cursor >= text.len() {
        return text.len();
    }

    let ch = text[cursor..].chars().next().unwrap_or_default();
    cursor + ch.len_utf8()
}

pub fn insert_char(buffer: &mut String, cursor: &mut usize, ch: char) {
    buffer.insert(*cursor, ch);
    *cursor += ch.len_utf8();
}

pub fn backspace_char(buffer: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }

    let start = previous_char_boundary(buffer, *cursor);
    buffer.drain(start..*cursor);
    *cursor = start;
}

pub fn delete_char(buffer: &mut String, cursor: &mut usize) {
    if *cursor >= buffer.len() {
        return;
    }

    let end = next_char_boundary(buffer, *cursor);
    buffer.drain(*cursor..end);
}

pub fn move_cursor_left(buffer: &str, cursor: &mut usize) {
    if *cursor > 0 {
        *cursor = previous_char_boundary(buffer, *cursor);
    }
}

pub fn move_cursor_right(buffer: &str, cursor: &mut usize) {
    if *cursor < buffer.len() {
        *cursor = next_char_boundary(buffer, *cursor);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previous_and_next_char_boundaries_handle_unicode() {
        let text = "A—B";

        assert_eq!(next_char_boundary(text, 0), 1);
        assert_eq!(next_char_boundary(text, 1), 4);
        assert_eq!(previous_char_boundary(text, 4), 1);
        assert_eq!(previous_char_boundary(text, text.len()), 4);
    }

    #[test]
    fn insert_and_backspace_char_track_utf8_cursor_positions() {
        let mut buffer = String::from("AB");
        let mut cursor = 1;

        insert_char(&mut buffer, &mut cursor, '—');
        assert_eq!(buffer, "A—B");
        assert_eq!(cursor, 4);

        backspace_char(&mut buffer, &mut cursor);
        assert_eq!(buffer, "AB");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn delete_char_removes_full_unicode_scalar() {
        let mut buffer = String::from("A—B");
        let mut cursor = 1;

        delete_char(&mut buffer, &mut cursor);
        assert_eq!(buffer, "AB");
        assert_eq!(cursor, 1);
    }

    #[test]
    fn cursor_movement_stops_at_character_boundaries() {
        let buffer = String::from("A—B");
        let mut cursor = 0;

        move_cursor_right(&buffer, &mut cursor);
        assert_eq!(cursor, 1);

        move_cursor_right(&buffer, &mut cursor);
        assert_eq!(cursor, 4);

        move_cursor_left(&buffer, &mut cursor);
        assert_eq!(cursor, 1);

        move_cursor_left(&buffer, &mut cursor);
        assert_eq!(cursor, 0);
    }
}
