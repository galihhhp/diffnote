use ratatui::style::Color;
use std::sync::OnceLock;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;

const THEME: &str = "base16-ocean.dark";

type Assets = (SyntaxSet, ThemeSet);

pub type Token = (String, Color);

fn assets() -> &'static Assets {
    static ASSETS: OnceLock<Assets> = OnceLock::new();
    ASSETS.get_or_init(|| (two_face::syntax::extra_newlines(), ThemeSet::load_defaults()))
}

fn syntax_for(path: &str) -> &'static syntect::parsing::SyntaxReference {
    let set = &assets().0;
    let name = path.rsplit('/').next().unwrap_or(path);
    let dotted = name.strip_prefix('.').unwrap_or("");
    name.rsplit_once('.')
        .and_then(|(_, extension)| set.find_syntax_by_extension(extension))
        .or_else(|| set.find_syntax_by_extension(name))
        .or_else(|| set.find_syntax_by_extension(dotted))
        .unwrap_or_else(|| set.find_syntax_plain_text())
}

fn tame(color: Color) -> Color {
    if let Color::Rgb(r, g, b) = color {
        let (r, g, b) = (r as i32, g as i32, b as i32);
        if r > g + 40 && r > b + 40 {
            let mut muted_r = (r + 143) / 2;
            let muted_g = (g + 161) / 2;
            let muted_b = (b + 179) / 2;
            let cap = muted_g.max(muted_b) + 30;
            if muted_r > cap {
                muted_r = cap;
            }
            return Color::Rgb(muted_r as u8, muted_g as u8, muted_b as u8);
        }
    }
    color
}

pub fn highlight(path: &str, lines: &[String]) -> Vec<Vec<Token>> {
    let (set, themes) = assets();
    let Some(theme) = themes.themes.get(THEME) else {
        return lines.iter().map(|_| Vec::new()).collect();
    };
    let mut highlighter = HighlightLines::new(syntax_for(path), theme);
    lines
        .iter()
        .map(|line| {
            highlighter
                .highlight_line(line, set)
                .unwrap_or_default()
                .into_iter()
                .map(|(style, chunk)| {
                    let color = style.foreground;
                    (chunk.to_string(), tame(Color::Rgb(color.r, color.g, color.b)))
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_known_languages_and_leaves_unknown_plain() {
        let rust = highlight("a.rs", &["fn main() {}".to_string()]);
        assert!(rust[0].len() > 1);
        let typescript = highlight("a.ts", &["const x = 1;".to_string()]);
        assert!(typescript[0].len() > 1);
        let unknown = highlight("a.zzz", &["some text".to_string()]);
        assert!(unknown[0].len() <= 1);
    }

    #[test]
    fn tame_removes_red_dominance() {
        let muted = tame(Color::Rgb(191, 97, 106));
        let Color::Rgb(r, g, b) = muted else {
            panic!("expected rgb");
        };
        let (r, g, b) = (r as i32, g as i32, b as i32);
        assert!(!(r > g + 40 && r > b + 40));
        assert_eq!(tame(Color::Rgb(192, 197, 206)), Color::Rgb(192, 197, 206));
    }
}

