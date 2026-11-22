use anyhow::{Context, Result, ensure};

use crate::notes::key_line;

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

struct Frontmatter<'a> {
    bom: &'a str,
    opening_eol: &'a str,
    lines: Vec<(&'a str, &'a str)>,
    closing_eol: &'a str,
    body: &'a str,
}

fn frontmatter(text: &str) -> Result<Frontmatter<'_>> {
    let (bom, text) = text
        .strip_prefix('\u{feff}')
        .map_or(("", text), |rest| ("\u{feff}", rest));
    let (opening, opening_eol, mut rest) = line(text);
    ensure!(
        opening == "---",
        "no exact frontmatter opening fence; refusing to rewrite"
    );
    let mut lines = Vec::new();
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
        lines.push((value, eol));
    };
    Ok(Frontmatter {
        bom,
        opening_eol,
        lines,
        closing_eol,
        body: rest,
    })
}

pub(super) fn field_lines(text: &str) -> Result<impl Iterator<Item = &str>> {
    Ok(frontmatter(text)?.lines.into_iter().map(|(value, _)| value))
}

pub(super) fn edit(
    bytes: &[u8],
    updates: &[(&str, &str)],
    insert_after: Option<(&str, &str)>,
    banner: Option<&str>,
) -> Result<Vec<u8>> {
    rewrite(
        bytes,
        updates,
        insert_after.map_or(Missing::Refuse, |(key, anchor)| Missing::After(key, anchor)),
        banner.map_or(Body::Keep, Body::Banner),
    )
}

pub(super) fn patch(bytes: &[u8], updates: &[(&str, &str)], body: Option<&str>) -> Result<Vec<u8>> {
    rewrite(
        bytes,
        updates,
        Missing::Append,
        body.map_or(Body::Keep, Body::Replace),
    )
}

pub(super) fn patch_with_banner(
    bytes: &[u8],
    updates: &[(&str, &str)],
    banner: &str,
) -> Result<Vec<u8>> {
    rewrite(bytes, updates, Missing::Append, Body::Banner(banner))
}

#[derive(Clone, Copy)]
enum Missing<'a> {
    Refuse,
    After(&'a str, &'a str),
    Append,
}

enum Body<'a> {
    Keep,
    Banner(&'a str),
    Replace(&'a str),
}

fn write_field(out: &mut String, key: &str, value: &str, eol: &str) {
    out.push_str(key);
    out.push_str(": ");
    out.push_str(value);
    out.push_str(eol);
}

fn rewrite(
    bytes: &[u8],
    updates: &[(&str, &str)],
    missing: Missing<'_>,
    tail: Body<'_>,
) -> Result<Vec<u8>> {
    let text = std::str::from_utf8(bytes).context("note is not UTF-8")?;
    let Frontmatter {
        bom,
        opening_eol,
        lines,
        closing_eol,
        body,
    } = frontmatter(text)?;
    let mut output = String::from(bom);
    output.push_str("---");
    output.push_str(opening_eol);
    let mut done = vec![false; updates.len()];
    let insertion = match missing {
        Missing::After(new, anchor)
            if !lines
                .iter()
                .any(|(value, _)| key_line(value).is_some_and(|(key, _)| key == new)) =>
        {
            Some((new, anchor))
        }
        Missing::Refuse | Missing::After(..) | Missing::Append => None,
    };
    let mut rewriting = false;
    for (value, eol) in lines {
        if let Some((key, _)) = key_line(value) {
            let found = updates.iter().position(|(name, _)| *name == key);
            rewriting = found.is_some();
            if let Some(i) = found {
                ensure!(
                    !done[i],
                    "duplicate frontmatter field {key}; refusing to rewrite"
                );
                done[i] = true;
                write_field(&mut output, key, updates[i].1, eol);
            } else {
                output.push_str(value);
                output.push_str(eol);
            }
            if let Some((new, anchor)) = insertion
                && anchor == key
            {
                let i = updates
                    .iter()
                    .position(|(name, _)| *name == new)
                    .context("missing insertion value")?;
                write_field(&mut output, new, updates[i].1, eol);
                done[i] = true;
            }
        } else {
            let trimmed = value.trim_start();
            if rewriting && trimmed != value && trimmed.starts_with('-') {
                ensure!(
                    matches!(missing, Missing::Append),
                    "cannot safely rewrite a block-list field; convert it to an inline list first"
                );
                if trimmed
                    .strip_prefix('-')
                    .is_some_and(|rest| rest.starts_with(char::is_whitespace))
                {
                    continue;
                }
            }
            output.push_str(value);
            output.push_str(eol);
        }
    }
    for (key, value) in done
        .iter()
        .zip(updates)
        .filter_map(|(done, field)| (!done).then_some(field))
    {
        ensure!(
            matches!(missing, Missing::Append),
            "frontmatter has no {key}: line; refusing to rewrite"
        );
        write_field(&mut output, key, value, opening_eol);
    }
    output.push_str("---");
    output.push_str(closing_eol);
    match tail {
        Body::Keep => output.push_str(body),
        Body::Banner(banner) => {
            let nl = if closing_eol.is_empty() {
                output.push_str(opening_eol);
                opening_eol
            } else {
                closing_eol
            };
            output.push_str(nl);
            output.push_str(banner);
            output.push_str(closing_eol);
            output.push_str(body);
        }
        Body::Replace(body) => {
            if closing_eol.is_empty() && !body.is_empty() {
                output.push_str(opening_eol);
            }
            output.push_str(body);
        }
    }
    Ok(output.into_bytes())
}
