/// Bound server-authored prose before retaining it in host session state.
pub(super) fn bounded_prose(value: &str) -> String {
    const MAX_CHARS: usize = 512;
    let mut bounded = String::with_capacity(MAX_CHARS + 3);
    let mut count = 0_usize;
    let mut pending_space = false;
    let mut truncated = false;
    for character in value.chars() {
        if character.is_whitespace() || character.is_control() {
            pending_space = !bounded.is_empty();
            continue;
        }
        if pending_space {
            if count == MAX_CHARS {
                truncated = true;
                break;
            }
            bounded.push(' ');
            count += 1;
            pending_space = false;
        }
        if count == MAX_CHARS {
            truncated = true;
            break;
        }
        bounded.push(character);
        count += 1;
    }
    if truncated {
        bounded.push('…');
    }
    bounded
}
