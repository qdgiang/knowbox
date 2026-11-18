use anyhow::{Context, Result, ensure};

// Keep each original terminator with its line. A body containing CR, CRLF, or
// mixed endings is copied as one opaque slice after the exact closing fence.
fn line(text: &str) -> (&str, &str, &str) {
    let Some(index) = text.find(['\r', '\n']) else {
        return (text, "", "");
    };
    let (line, rest) = text.split_at(index);
    let (eol, rest) = rest.split_at(if rest.starts_with("\r\n") { 2 } else { 1 });
    (line, eol, rest)
}

pub(super) fn edit(
    bytes: &[u8],
    updates: &[(&str, &str)],
    insert_after: Option<(&str, &str)>,
    banner: Option<&str>,
) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes).context("note is not UTF-8")?;
    let (bom, text) = text
        .strip_prefix('\u{feff}')
        .map_or(("", text), |rest| ("\u{feff}", rest));
    let (opening, style, mut rest) = line(text);
    ensure!(
        opening == "---",
        "no exact frontmatter opening fence; refusing to rewrite"
    );
    let mut fm = Vec::new();
    let closing_eol = loop {
        ensure!(!rest.is_empty(), "unterminated frontmatter");
        let (value, eol, tail) = line(rest);
        rest = tail;
        if value == "---" {
            break eol;
        }
        ensure!(
            !value.starts_with("---"),
            "nonstandard frontmatter closing fence; refusing to rewrite"
        );
        fm.push((value, eol));
    };

    let mut output = String::from(bom);
    output.push_str(opening);
    output.push_str(style);
    let mut done = vec![false; updates.len()];
    let key_of = |s: &str, key: &str| {
        s.strip_prefix(key)
            .is_some_and(|rest| rest.starts_with(':'))
    };
    let insertion = insert_after.filter(|(new, _)| !fm.iter().any(|(s, _)| key_of(s, new)));
    let mut rewriting = false;
    for (value, eol) in fm {
        if let Some((key, _)) = value.split_once(':')
            && !key.is_empty()
            && key.bytes().all(|c| c.is_ascii_alphabetic() || c == b'_')
        {
            let found = updates.iter().position(|(name, _)| *name == key);
            rewriting = found.is_some();
            if let Some(i) = found {
                ensure!(
                    !done[i],
                    "duplicate frontmatter field {key}; refusing to rewrite"
                );
                done[i] = true;
                output.push_str(key);
                output.push_str(": ");
                output.push_str(updates[i].1);
            } else {
                output.push_str(value);
            }
            output.push_str(eol);
            if let Some((new, anchor)) = insertion
                && anchor == key
            {
                let i = updates
                    .iter()
                    .position(|(name, _)| *name == new)
                    .context("missing insertion value")?;
                output.push_str(new);
                output.push_str(": ");
                output.push_str(updates[i].1);
                output.push_str(eol);
                done[i] = true;
            }
        } else {
            let trimmed = value.trim_start();
            ensure!(
                !(rewriting && trimmed != value && trimmed.starts_with('-')),
                "cannot safely rewrite a block-list field; convert it to an inline list first"
            );
            output.push_str(value);
            output.push_str(eol);
        }
    }
    for (applied, (key, _)) in done.iter().zip(updates) {
        ensure!(
            *applied,
            "frontmatter has no {key}: line; refusing to rewrite"
        );
    }
    output.push_str("---");
    output.push_str(closing_eol);
    if let Some(banner) = banner {
        let nl = if closing_eol.is_empty() {
            output.push_str(style);
            style
        } else {
            closing_eol
        };
        output.push_str(nl);
        output.push_str(banner);
        output.push_str(closing_eol);
    }
    output.push_str(rest);
    Ok(output.into_bytes())
}
