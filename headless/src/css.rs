//! The page's colors without a browser: CSS custom properties read from the site's stylesheets (its light theme,
//! which a PDF uses), and CSS colors resolved as the page resolves them: var(), color-mix() in srgb, hex, rgb()
//! and rgba(), and a few names.

use std::collections::HashMap;

use wgw_scene::Rgba;

/// Custom properties (`--name: value`) from rules for `:root`, `html` or `.wgw`, outside @media blocks and dark
/// themes; later ones win, as in the page.
pub fn variables(css: &str) -> HashMap<String, String> {
    let css = strip_comments(css);
    let bytes = css.as_bytes();
    let mut vars = HashMap::new();
    let mut start = 0; // where the current rule's selector begins
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                let end = block_end(bytes, i + 1); // a whole block, @media and all, is taken or skipped at once
                let selector = css[start..i].trim();
                let wanted = !selector.starts_with('@')
                    && !selector.contains("dark")
                    && selector.split(',').any(|s| matches!(s.trim(), ":root" | "html" | ".wgw"));
                if wanted {
                    for decl in css[i + 1..end].split(';') {
                        if let Some((name, value)) = decl.split_once(':') {
                            if name.trim().starts_with("--") {
                                vars.insert(name.trim().to_string(), value.trim().to_string());
                            }
                        }
                    }
                }
                i = end + 1;
                start = i;
            }
            b';' => {
                i += 1;
                start = i; // the end of an @import or the like
            }
            _ => i += 1,
        }
    }
    vars
}

fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        rest = rest[start + 2..].find("*/").map_or("", |end| &rest[start + 2 + end + 2..]);
    }
    out.push_str(rest);
    out
}

/// Where the block opened just before `from` closes.
fn block_end(bytes: &[u8], from: usize) -> usize {
    let mut depth = 1;
    for (k, &b) in bytes[from..].iter().enumerate() {
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return from + k;
                }
            }
            _ => {}
        }
    }
    bytes.len()
}

/// A value with every var() replaced by its property's value.
pub fn substitute(value: &str, vars: &HashMap<String, String>) -> String {
    let mut out = value.to_string();
    for _ in 0..16 {
        let Some(start) = out.find("var(") else { break };
        let end = matching_paren(&out, start + 3);
        let inner = &out[start + 4..end];
        let (name, fallback) = match inner.split_once(',') {
            Some((n, f)) => (n.trim(), Some(f.trim())),
            None => (inner.trim(), None),
        };
        let replacement = vars.get(name).map(String::as_str).or(fallback).unwrap_or("").to_string();
        out.replace_range(start..=end, &replacement);
    }
    out
}

fn matching_paren(s: &str, open: usize) -> usize {
    let mut depth = 0;
    for (k, c) in s[open..].char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return open + k;
                }
            }
            _ => {}
        }
    }
    s.len() - 1
}

/// Split a CSS list on the commas outside parentheses.
pub fn split_list(text: &str) -> Vec<String> {
    let (mut out, mut depth, mut current) = (Vec::new(), 0, String::new());
    for c in text.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if c == ',' && depth == 0 {
            out.push(current.trim().to_string());
            current.clear();
        } else {
            current.push(c);
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

/// A CSS color, with the page's variables; None if it can't be read.
pub fn color(value: &str, vars: &HashMap<String, String>) -> Option<Rgba> {
    parse(&substitute(value, vars))
}

fn parse(value: &str) -> Option<Rgba> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(hex) = v.strip_prefix('#') {
        let digit = |i: usize, n: usize| u8::from_str_radix(&hex[i..i + n], 16).ok().map(|x| if n == 1 { x * 17 } else { x } as f32 / 255.0);
        return match hex.len() {
            3 | 4 => Some([digit(0, 1)?, digit(1, 1)?, digit(2, 1)?, if hex.len() == 4 { digit(3, 1)? } else { 1.0 }]),
            6 | 8 => Some([digit(0, 2)?, digit(2, 2)?, digit(4, 2)?, if hex.len() == 8 { digit(6, 2)? } else { 1.0 }]),
            _ => None,
        };
    }
    if let Some(args) = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb(")) {
        let args = args.trim_end_matches(')').replace('/', " ");
        let parts: Vec<&str> = args.split(|c: char| c == ',' || c.is_whitespace()).filter(|s| !s.is_empty()).collect();
        let channel = |s: &str| s.strip_suffix('%').map_or_else(|| s.parse::<f32>().ok().map(|x| x / 255.0), |p| p.parse::<f32>().ok().map(|x| x / 100.0));
        let alpha = |s: &str| s.strip_suffix('%').map_or_else(|| s.parse::<f32>().ok(), |p| p.parse::<f32>().ok().map(|x| x / 100.0));
        return Some([channel(parts.first()?)?, channel(parts.get(1)?)?, channel(parts.get(2)?)?, parts.get(3).and_then(|a| alpha(a)).unwrap_or(1.0)]);
    }
    if let Some(args) = v.strip_prefix("color-mix(") {
        // color-mix(in srgb, a p%, b q%): the two mixed in sRGB, by their percentages (the rest to the other one).
        let args = &args[..args.rfind(')')?];
        let parts = split_list(args);
        let (a, b) = (parts.get(1)?, parts.get(2)?);
        let weighted = |s: &str| -> Option<(Rgba, Option<f32>)> {
            let s = s.trim();
            match s.rsplit_once(' ').filter(|(_, p)| p.ends_with('%')) {
                Some((c, p)) => Some((parse(c)?, Some(p.trim_end_matches('%').parse::<f32>().ok()? / 100.0))),
                None => Some((parse(s)?, None)),
            }
        };
        let ((ca, pa), (cb, pb)) = (weighted(a)?, weighted(b)?);
        let (pa, pb) = match (pa, pb) {
            (Some(x), Some(y)) => (x / (x + y), y / (x + y)),
            (Some(x), None) => (x, 1.0 - x),
            (None, Some(y)) => (1.0 - y, y),
            (None, None) => (0.5, 0.5),
        };
        // Premultiplied by alpha, as CSS mixes.
        let alpha = ca[3] * pa + cb[3] * pb;
        let mix = |c: usize| if alpha > 0.0 { (ca[c] * ca[3] * pa + cb[c] * cb[3] * pb) / alpha } else { 0.0 };
        return Some([mix(0), mix(1), mix(2), alpha]);
    }
    match v.as_str() {
        "black" => Some([0.0, 0.0, 0.0, 1.0]),
        "white" => Some([1.0, 1.0, 1.0, 1.0]),
        "transparent" => Some([0.0, 0.0, 0.0, 0.0]),
        "red" => Some([1.0, 0.0, 0.0, 1.0]),
        "gray" | "grey" => Some([0.5, 0.5, 0.5, 1.0]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_light_theme() {
        let css = ":root { --bg: #fffcf0; --link: #205ea6; }\n[data-theme=\"dark\"] { --bg: #000; }\n@media (prefers-color-scheme: dark) { :root { --bg: #111; } }\n.wgw { --wgw-colors: var(--bg), var(--link); }";
        let vars = variables(css);
        assert_eq!(vars["--bg"], "#fffcf0");
        let colors: Vec<Rgba> = split_list(&substitute(&vars["--wgw-colors"], &vars)).iter().map(|c| parse(c).unwrap()).collect();
        assert_eq!(colors.len(), 2);
        let mixed = color("color-mix(in srgb, var(--link), var(--bg) 50%)", &vars).unwrap();
        assert!((mixed[0] - (0x20 as f32 + 255.0) / 2.0 / 255.0).abs() < 1e-3);
        assert_eq!(color("rgba(16, 15, 15, 0.1)", &vars).unwrap()[3], 0.1);
    }
}
