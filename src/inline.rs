use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Bold,
    Italic,
    Code,
}

#[derive(Debug, Default, PartialEq)]
pub struct Styled {
    pub text: String,
    pub marks: Vec<(Range<usize>, Mark)>,
}

const DELIMITERS: [(&str, Mark); 3] = [("`", Mark::Code), ("**", Mark::Bold), ("*", Mark::Italic)];

pub fn parse(source: &str) -> Styled {
    let mut styled = Styled::default();
    let mut rest = source;
    while let Some(c) = rest.chars().next() {
        match DELIMITERS.iter().find_map(|&(delimiter, mark)| enclosed(rest, delimiter).map(|inner| (delimiter, mark, inner))) {
            Some((delimiter, mark, inner)) => {
                let start = styled.text.len();
                styled.text.push_str(inner);
                styled.marks.push((start..styled.text.len(), mark));
                rest = &rest[inner.len() + 2 * delimiter.len()..];
            }
            None => {
                styled.text.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    styled
}

fn enclosed<'a>(rest: &'a str, delimiter: &str) -> Option<&'a str> {
    let after = rest.strip_prefix(delimiter)?;
    let inner = &after[..after.find(delimiter)?];
    let flanked = !inner.starts_with(char::is_whitespace) && !inner.ends_with(char::is_whitespace);
    (!inner.is_empty() && flanked && !inner.contains('\n')).then_some(inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked(source: &str) -> Vec<(String, Mark)> {
        let styled = parse(source);
        styled.marks.iter().map(|(range, mark)| (styled.text[range.clone()].to_owned(), *mark)).collect()
    }

    #[test]
    fn bold_italic_and_code_drop_their_markers() {
        let styled = parse("Use **TOML** with *care* and `cargo fmt`.");
        assert_eq!(styled.text, "Use TOML with care and cargo fmt.");
        assert_eq!(marked("Use **TOML** with *care* and `cargo fmt`."), [("TOML".into(), Mark::Bold), ("care".into(), Mark::Italic), ("cargo fmt".into(), Mark::Code)]);
    }

    #[test]
    fn underscores_and_stray_stars_stay_literal() {
        for source in ["rename snake_case_name and __init__", "2 * 3 * 4", "a ** b", "**unclosed", "`", "** spaced **"] {
            assert_eq!(parse(source), Styled { text: source.into(), marks: vec![] }, "{source:?}");
        }
    }

    #[test]
    fn code_keeps_stars_inside_it() {
        assert_eq!(parse("`a*b*c`").text, "a*b*c");
        assert_eq!(marked("`a*b*c`"), [("a*b*c".into(), Mark::Code)]);
    }
}
