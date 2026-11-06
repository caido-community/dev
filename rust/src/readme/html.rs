use std::cell::RefCell;

use html5ever::serialize::Serializer;
use html5ever::{
    buffer_queue::BufferQueue,
    serialize::{HtmlSerializer, SerializeOpts},
    tendril::StrTendril,
    tokenizer::{TagKind, Token, TokenSink, TokenSinkResult, Tokenizer},
    QualName,
};

use super::url_policy::{Context, Work};

#[derive(Default)]
struct HtmlTokens(RefCell<Vec<Token>>);
impl TokenSink for HtmlTokens {
    type Handle = ();
    fn process_token(&self, token: Token, _: u64) -> TokenSinkResult<()> {
        self.0.borrow_mut().push(token);
        TokenSinkResult::Continue
    }
}

/// Serialize tokens, not a DOM: inline opening/closing tags are separate Markdown
/// nodes, and constructing a tree would insert closing tags at those boundaries.
pub(super) fn transform(value: &str, work: &mut Work<'_>) -> Result<String, String> {
    let tokenizer = Tokenizer::new(HtmlTokens::default(), Default::default());
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(value));
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    let mut output = Vec::new();
    for token in tokenizer.sink.0.into_inner() {
        // A fresh serializer avoids its DOM stack suppressing subsequent tokens
        // after void elements, or requiring matching tags within this fragment.
        let mut writer = HtmlSerializer::new(
            &mut output,
            SerializeOpts {
                create_missing_parent: true,
                ..Default::default()
            },
        );
        let result = match token {
            Token::TagToken(mut tag) => {
                let name = QualName::new(None, html5ever::ns!(html), tag.name);
                if tag.kind == TagKind::EndTag {
                    writer.end_elem(name)
                } else {
                    for attr in &mut tag.attrs {
                        let key = attr.name.local.as_ref();
                        let value = if matches!(key, "style" | "srcset") {
                            String::new()
                        } else if matches!(
                            key,
                            "action"
                                | "background"
                                | "data"
                                | "formaction"
                                | "href"
                                | "poster"
                                | "src"
                                | "xlink:href"
                        ) {
                            let context = if key == "href" {
                                Context::Link
                            } else {
                                Context::Resource
                            };
                            work.url(
                                &attr.value,
                                context,
                                if context == Context::Link {
                                    "HTML link"
                                } else {
                                    "HTML resource"
                                },
                            )?
                        } else {
                            attr.value.to_string()
                        };
                        attr.value = value.into();
                    }
                    writer.start_elem(
                        name,
                        tag.attrs
                            .iter()
                            .map(|attr| (&attr.name, attr.value.as_ref())),
                    )
                }
            }
            Token::CharacterTokens(text) => writer.write_text(&text),
            Token::NullCharacterToken => writer.write_text("\u{fffd}"),
            Token::CommentToken(text) => writer.write_comment(&text),
            Token::DoctypeToken(doctype) => {
                writer.write_doctype(doctype.name.as_deref().unwrap_or("html"))
            }
            Token::ParseError(_) | Token::EOFToken => Ok(()),
        };
        result.map_err(|e| format!("Unable to serialize README HTML: {e}"))?;
    }
    String::from_utf8(output).map_err(|e| e.to_string())
}
