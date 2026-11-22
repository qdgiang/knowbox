use pulldown_cmark::{Event, LinkType, Options, Parser, Tag, TagEnd, html};

use crate::notes;

pub(super) fn render(markdown: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_WIKILINKS;
    let mut link_opened = false;
    let parser = Parser::new_ext(markdown, options).filter_map(|event| match event {
        Event::Html(html) | Event::InlineHtml(html) => Some(Event::Text(html)),
        Event::Start(Tag::Image { .. }) | Event::End(TagEnd::Image) => None,
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => {
            let dest = if matches!(link_type, LinkType::WikiLink { .. }) && notes::is_id(&dest_url)
            {
                format!("/n/{dest_url}").into()
            } else if dest_url.starts_with("https://") || dest_url.starts_with("http://") {
                dest_url
            } else {
                return None;
            };
            link_opened = true;
            Some(Event::Start(Tag::Link {
                link_type,
                dest_url: dest,
                title,
                id,
            }))
        }
        Event::End(TagEnd::Link) => {
            std::mem::take(&mut link_opened).then_some(Event::End(TagEnd::Link))
        }
        event => Some(event),
    });
    let mut output = String::new();
    html::push_html(&mut output, parser);
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_keeps_external_requests_inert_until_a_safe_link_is_clicked() {
        let html = render(
            "<img src='https://example.org/canary'>\n\n[x](javascript:alert%281%29) ![alternate](https://example.org/image) [[kb-0042]] [reference](https://example.org/reference)",
        );
        assert_eq!(
            html,
            "&lt;img src='https://example.org/canary'&gt;\n<p>x alternate <a href=\"/n/kb-0042\">kb-0042</a> <a href=\"https://example.org/reference\">reference</a></p>\n",
        );
    }
}
