//! Display-only removal of audio directions. The original speech text is kept
//! separately for TTS; square brackets outside Markdown/code/index expressions
//! are reserved for audio tags. This scanner makes no model or network calls.
use std::collections::HashSet;

/// Remove reserved audio-tag groups while retaining source citations, Markdown
/// links/code and attached indexing such as `array[index]`. Keep line endings.
pub fn display_text(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len());
    let references: HashSet<String> = raw.lines().filter_map(|line| {
        let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
        if indentation > 3 { return None; }
        let tail = &line[indentation..];
        if !tail.starts_with('[') { return None; }
        let end = tail.find(']')?;
        tail[end + 1..].starts_with(':').then(|| reference_name(&tail[1..end]))
    }).collect();
    let mut fence: Option<(u8, usize)> = None;
    let mut inline_ticks = 0usize;
    // Splitting CR and LF separately also preserves CRLF byte-for-byte.
    for segment in raw.split_inclusive(['\r', '\n']) {
        let (line, ending) = if segment.ends_with(['\r', '\n']) {
            segment.split_at(segment.len() - 1)
        } else { (segment, "") };
        if let Some((character, length)) = fence {
            result.push_str(line);
            if let Some((next, count, tail)) = fence_marker(line) {
                if character == next && count >= length && tail.trim().is_empty() { fence = None; }
            }
        } else if inline_ticks == 0 {
            if let Some((character, count, _)) = fence_marker(line) {
                fence = Some((character, count)); result.push_str(line);
            } else if line.starts_with("    ") || line.starts_with('\t') {
                result.push_str(line);
            } else { result.push_str(&clean_line(line, &mut inline_ticks, &references)); }
        } else { result.push_str(&clean_line(line, &mut inline_ticks, &references)); }
        result.push_str(ending);
    }
    result
}

fn fence_marker(line: &str) -> Option<(u8, usize, &str)> {
    let indentation = line.bytes().take_while(|byte| *byte == b' ').count();
    if indentation > 3 { return None; }
    let tail = &line[indentation..];
    let character = *tail.as_bytes().first()?;
    if !matches!(character, b'`' | b'~') { return None; }
    let count = tail.bytes().take_while(|byte| *byte == character).count();
    (count >= 3).then_some((character, count, &tail[count..]))
}

fn horizontal_space(character: char) -> bool { character.is_whitespace() && character != '\r' && character != '\n' }
fn reference_name(content: &str) -> String { content.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase() }
fn numeric_reference(content: &str) -> bool {
    content.chars().any(|character| character.is_ascii_digit())
        && content.chars().all(|character| character.is_ascii_digit() || character.is_whitespace() || matches!(character, ',' | ';' | ':' | '-' | '–' | '—' | '.' | '^'))
}
fn attached_index(output: &str) -> bool {
    output.chars().next_back().is_some_and(|character| character.is_alphanumeric() || matches!(character, '_' | '$' | ']' | ')' | '}' | '\'' | '"'))
}
fn matching_bracket(line: &str, start: usize) -> Option<(usize, bool)> {
    matching_delimiter(line, start, '[', ']')
}
fn matching_delimiter(line: &str, start: usize, opening: char, closing: char) -> Option<(usize, bool)> {
    let mut depth = 1usize;
    let mut nested = false;
    let mut escaped = false;
    for (offset, character) in line[start + 1..].char_indices() {
        if escaped { escaped = false; continue; }
        if character == '\\' { escaped = true; continue; }
        if character == opening { depth += 1; nested = true; }
        if character == closing {
            depth -= 1;
            if depth == 0 { return Some((start + 1 + offset, nested)); }
        }
    }
    None
}

fn clean_line(line: &str, inline_ticks: &mut usize, references: &HashSet<String>) -> String {
    let mut output = String::with_capacity(line.len());
    let bytes = line.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'`' {
            let count = bytes[at..].iter().take_while(|byte| **byte == b'`').count();
            if *inline_ticks == 0 { *inline_ticks = count; }
            else if *inline_ticks == count { *inline_ticks = 0; }
            output.push_str(&line[at..at + count]); at += count; continue;
        }
        if *inline_ticks > 0 {
            let character = line[at..].chars().next().expect("valid UTF-8 cursor");
            output.push(character); at += character.len_utf8(); continue;
        }
        if bytes[at] == b'\\' {
            output.push('\\'); at += 1;
            if at < bytes.len() { let character = line[at..].chars().next().expect("valid UTF-8 cursor"); output.push(character); at += character.len_utf8(); }
            continue;
        }
        if bytes[at] == b'[' {
            let Some((end, nested)) = matching_bracket(line, at) else {
                // An unmatched opener is literal text; avoid rescanning every
                // nested '[' in malformed input (the complete scan stays O(n)).
                output.push_str(&line[at..]); break;
            };
            let content = &line[at + 1..end];
            let after = &line[end + 1..];
            if after.starts_with('(') {
                if let Some((close, _)) = matching_delimiter(line, end + 1, '(', ')') {
                    output.push_str(&line[at..=close]); at = close + 1; continue;
                }
                // Preserve malformed/incomplete Markdown conservatively, and
                // do not repeatedly scan unmatched parentheses further on.
                output.push_str(&line[at..]); break;
            }
            let reference = references.contains(&reference_name(content)) || (after.starts_with('[') && matching_bracket(after, 0).is_some_and(|(end, _)| references.contains(&reference_name(&after[1..end]))));
            let markdown = after.starts_with(['(', ':']) || content.starts_with('^') || reference;
            if nested || content.trim().is_empty() || markdown || numeric_reference(content) || attached_index(&output) {
                output.push_str(&line[at..=end]); at = end + 1; continue;
            }
            let had_left_space = output.chars().next_back().is_some_and(horizontal_space);
            while output.chars().next_back().is_some_and(horizontal_space) { output.pop(); }
            let mut next = end + 1;
            for character in after.chars().take_while(|character| horizontal_space(*character)) { next += character.len_utf8(); }
            let had_right_space = next > end + 1;
            let punctuation = line[next..].chars().next().is_some_and(|character| matches!(character, ',' | '.' | '!' | '?' | ';' | ':' | ')' | ']' | '}'));
            // Adjacent tag groups belong to one direction sequence, not to an
            // index chain. Add spacing only when the original had a separator.
            if !output.is_empty() && next < bytes.len() && (had_left_space || had_right_space) && !punctuation { output.push(' '); }
            at = next; continue;
        }
        let character = line[at..].chars().next().expect("valid UTF-8 cursor");
        output.push(character); at += character.len_utf8();
    }
    output
}

#[cfg(test)]
mod tests {
    use super::display_text;

    #[test]
    fn removes_audio_tags_and_only_normalizes_their_surrounding_spaces() {
        assert_eq!(display_text("[crying] Hallo [shouts] Welt!"), "Hallo Welt!");
        assert_eq!(display_text("Hallo   [voice trembling with emotion]  Welt."), "Hallo Welt.");
        assert_eq!(display_text("[whispering]Leise."), "Leise.");
        assert_eq!(display_text("Hallo [crying], Welt [whispering] !"), "Hallo, Welt!");
        assert_eq!(display_text("Unberührt   mit\tAbstand"), "Unberührt   mit\tAbstand");
    }

    #[test]
    fn preserves_line_endings_and_unicode() {
        assert_eq!(display_text("[crying] Äpfel\r\n[whispering] Grüße\n[shouts]\n"), "Äpfel\r\nGrüße\n\n");
        assert_eq!(display_text("[crying] eins\r[shouts] zwei"), "eins\rzwei");
        assert_eq!(display_text("[crying]\u{a0}Überraschung 🦊!"), "Überraschung 🦊!");
    }

    #[test]
    fn protects_numeric_sources_and_markdown_links() {
        let raw = "Fakten [1], [2–4] und [1, 3]. [Quelle](https://example.org/a) ![Bild](img.png) [Text][ref]\n[ref]: https://example.org\nFußnote [^beleg].";
        assert_eq!(display_text(raw), raw);
        assert_eq!(display_text("[crying] Belegt [1]."), "Belegt [1].");
        let link = "[Quelle](https://example.org/[section] \"[crying] im Titel\")";
        assert_eq!(display_text(link), link);
    }

    #[test]
    fn protects_inline_code_and_index_expressions() {
        let raw = "`[crying]` und ``array[index] + `[shouts]` `` sowie array[index], foo()[i], matrix[row][column].";
        assert_eq!(display_text(raw), raw);
        assert_eq!(display_text("[whispering] Verwende `array[index]`."), "Verwende `array[index]`.");
    }

    #[test]
    fn protects_fenced_and_indented_code_but_resumes_after_closing_fence() {
        let raw = "[crying] Vorher\n```js\nconst a = [crying];\n```\n[shouts] Nachher\n    array[index] = [crying];\n";
        let expected = "Vorher\n```js\nconst a = [crying];\n```\nNachher\n    array[index] = [crying];\n";
        assert_eq!(display_text(raw), expected);
        assert_eq!(display_text("~~~\n[crying]\n~~~\n[shouts] Ende"), "~~~\n[crying]\n~~~\nEnde");
    }

    #[test]
    fn handles_repeated_escaped_nested_and_unmatched_groups_without_losing_text() {
        assert_eq!(display_text("[crying][shouts] Hallo"), "Hallo");
        assert_eq!(display_text("[crying][shouts]Hallo"), "Hallo");
        assert_eq!(display_text("[crying] [shouts] Hallo"), "Hallo");
        assert_eq!(display_text("Hallo [crying][whispering] Welt"), "Hallo Welt");
        assert_eq!(display_text(r"\[crying] ist wörtlich"), r"\[crying] ist wörtlich");
        assert_eq!(display_text("[[1]] und [] bleiben"), "[[1]] und [] bleiben");
        assert_eq!(display_text("Offen [crying"), "Offen [crying");
        assert_eq!(display_text("[".repeat(50_000).as_str()).len(), 50_000);
        assert_eq!(display_text(""), "");
    }

    #[test]
    fn history_sanitizes_only_assistant_text_without_rewriting_user_content() {
        let root = std::env::temp_dir().join(format!("cohost-speech-history-{}", uuid::Uuid::new_v4()));
        {
            let store = crate::storage::Store::new(root.clone()).unwrap();
            store.add_message("assistant", "[crying] Hallo", "operator", "Nova").unwrap();
            store.add_message("user", "[crying] wieso?", "operator", "Streamer").unwrap();
            let history = store.history().unwrap();
            assert_eq!(history[0]["text"], "Hallo");
            assert_eq!(history[1]["text"], "[crying] wieso?");
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
