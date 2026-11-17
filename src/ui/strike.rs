use std::ops::Range;

use rune::{Span, TextStyle};

pub fn struck(text: &str, spans: &[Span], style: TextStyle, width: f32, code: &[Range<usize>]) -> Vec<Range<usize>> {
    let words: Vec<Range<usize>> = words(text).collect();
    let height = |end: usize| rune::text::measure(&text[..end], &clipped(spans, end), style, Some(width)).1;
    let mut lines = Vec::new();
    let mut first = 0;
    while let Some(word) = words.get(first) {
        let line = height(word.end);
        let last = first + words[first..].partition_point(|w| height(w.end) <= line + 0.5) - 1;
        lines.push(word.start..words[last].end);
        first = last + 1;
    }
    lines.into_iter().flat_map(|line| split_at(line, code)).collect()
}

fn words(text: &str) -> impl Iterator<Item = Range<usize>> + '_ {
    text.split_whitespace().map(move |word| {
        let start = word.as_ptr() as usize - text.as_ptr() as usize;
        start..start + word.len()
    })
}

fn clipped(spans: &[Span], end: usize) -> Vec<Span> {
    let end = end as u32;
    spans.iter().filter(|s| s.start < end).map(|s| Span { end: s.end.min(end), ..s.clone() }).collect()
}

fn split_at(line: Range<usize>, code: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut cuts: Vec<usize> = code
        .iter()
        .flat_map(|c| [c.start, c.end])
        .filter(|&at| line.start < at && at < line.end)
        .chain([line.start, line.end])
        .collect();
    cuts.sort_unstable();
    cuts.dedup();
    cuts.windows(2).map(|pair| pair[0]..pair[1]).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::chrome::BODY;

    #[test]
    fn each_wrapped_line_is_struck_from_its_first_to_its_last_glyph() {
        let text = "Summarize the failing test output and suggest the smallest fix";
        let width = rune::text::measure("Summarize the failing test output", &[], BODY, None).0 + 1.0;
        let ranges = struck(text, &[], BODY, width, &[]);
        let lines: Vec<&str> = ranges.iter().map(|r| &text[r.clone()]).collect();
        assert_eq!(lines, ["Summarize the failing test output", "and suggest the smallest fix"]);
    }

    #[test]
    fn a_code_span_gets_its_own_stroke_inside_its_line() {
        let text = "Check whether retry_after is read";
        let code = 14..25;
        let ranges = struck(text, &[], BODY, 1000.0, std::slice::from_ref(&code));
        assert_eq!(ranges, [0..14, 14..25, 25..text.len()]);
    }
}
