use super::*;

const DECLARATION: &str = "<meta name='generator' content='WordPress 6.8.1'>";

fn document(head: &str) -> String {
    format!("<html><head>{head}</head><body></body></html>")
}

fn reason(result: &Inspection) -> Option<StopReason> {
    match result.coverage {
        Coverage::ExplicitHeadClosed => None,
        Coverage::Partial { reason, .. } => Some(reason),
    }
}

fn assert_invariants(input: &str, result: &Inspection) {
    assert!(result.observations.len() <= MAX_OBSERVATIONS);
    assert!(result.tag_candidates_inspected <= MAX_TAG_CANDIDATES);
    assert!(result.input_bytes_considered <= MAX_INPUT_BYTES);
    assert!(input.is_char_boundary(result.input_bytes_considered));
    assert_eq!(result.input_bytes_available, input.len());
    assert_eq!(result.input_limit_exceeded, input.len() > MAX_INPUT_BYTES);
    if result.additional_tag_candidate_encountered {
        assert_eq!(reason(result), Some(StopReason::TagCandidateLimit));
        assert_eq!(result.tag_candidates_inspected, MAX_TAG_CANDIDATES);
    }
    if result.additional_observation_encountered {
        assert_eq!(reason(result), Some(StopReason::ObservationLimit));
        assert_eq!(result.observations.len(), MAX_OBSERVATIONS);
    }
    for observation in &result.observations {
        let spans = [
            observation.tag_span,
            observation.content_span,
            observation.product_span,
        ];
        for span in spans {
            assert!(span.start <= span.end);
            assert!(span.end <= result.input_bytes_considered);
            assert!(input.is_char_boundary(span.start));
            assert!(input.is_char_boundary(span.end));
        }
        let tag = &input[observation.tag_span.start..observation.tag_span.end];
        assert!(tag.starts_with('<') && tag.ends_with('>'));
        assert!(tag.len() <= MAX_TAG_BYTES);
        assert!(observation.tag_span.start < observation.content_span.start);
        assert!(observation.content_span.end < observation.tag_span.end);
        assert_eq!(
            observation.product_span.start,
            observation.content_span.start
        );
        assert!(observation.product_span.end <= observation.content_span.end);
        let product = &input[observation.product_span.start..observation.product_span.end];
        assert!(product.eq_ignore_ascii_case(match observation.product {
            Product::WordPress => "WordPress",
            Product::Drupal => "Drupal",
        }));
        match (
            observation.version_status,
            &observation.reported_version,
            observation.version_span,
        ) {
            (VersionStatus::Reported, Some(version), Some(span)) => {
                assert_eq!(&input[span.start..span.end], version);
                assert!(span.start > observation.product_span.end);
                let separator = &input.as_bytes()[observation.product_span.end..span.start];
                assert!((1..=MAX_VERSION_SEPARATOR_BYTES).contains(&separator.len()));
                assert!(separator.iter().all(|b| space(*b)));
                assert_eq!(span.end, observation.content_span.end);
                assert!(valid_version(version.as_bytes()));
            }
            (VersionStatus::NotReported | VersionStatus::UnsupportedFormat, None, None) => {}
            _ => panic!("inconsistent version status/provenance"),
        }
    }
}

#[test]
fn recognizes_exact_products_case_attribute_order_and_quote_variants() {
    for product in ["WordPress", "Drupal", "WORDPRESS", "dRuPaL"] {
        for version in ["0", "6.8.1", "001.02", "12345678901234567890123456789012"] {
            for quote in ['\'', '"'] {
                for attrs in [
                    format!("NAME={quote}GeNeRaToR{quote} CONTENT={quote}{product} {version}{quote}"),
                    format!("content = {quote}{product} {version}{quote}\nname = {quote}generator{quote}"),
                ] {
                    let input = format!("<!DoCtYpE \tHTML ><HTML lang='en'><HEAD><MeTa {attrs} /></hEaD>");
                    let result = inspect_html_generator(&input);
                    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
                    assert_eq!(result.observations.len(), 1);
                    assert_eq!(result.observations[0].reported_version.as_deref(), Some(version));
                    assert_eq!(result.observations[0].version_status, VersionStatus::Reported);
                    assert_invariants(&input, &result);
                }
            }
        }
    }
}

#[test]
fn original_multibyte_bom_crlf_offsets_are_not_normalized() {
    let input = "\u{feff}<!--🦀 café-->\r\n<html lang='日本語'><head data-x='☃'><meta data-note=\"<meta name='generator' content='Drupal 1'>\" content='wOrDpReSs 6.8' name='generator'></head>";
    let result = inspect_html_generator(input);
    assert_eq!(result.observations.len(), 1);
    let o = &result.observations[0];
    assert_eq!(o.tag_span.start, input.find("<meta data-note").unwrap());
    assert_eq!(o.content_span.start, input.find("wOrDpReSs").unwrap());
    assert_eq!(o.product_span.end - o.product_span.start, 9);
    assert_eq!(o.version_span.unwrap().start, input.find("6.8").unwrap());
    assert_eq!(o.offset_basis, OffsetBasis::RetainedUtf8PreviewBytes);
    assert_invariants(input, &result);
}

#[test]
fn raw_ampersands_in_unrelated_attributes_are_safely_opaque() {
    let input = concat!(
        "<html data-x='&quot;'><head data-note='é&amp;🦀'>",
        "<link href='https://example.com/?a=1&b=2'>",
        "<base href='https://example.com/?a=1&amp;b=2'>",
        "<meta name='description' content='&quot; WordPress 9 &amp;'>",
        "<meta content='Word&#80;ress 9'>",
        "<script src='https://example.com/a.js?x=1&y=2'>0</script>",
        "<style data-note='&'>x</style><title data-note='&'>plain title</title>",
        "<meta data-x='&quot;&amp;&#34;' name='generator' content='WordPress 6.8'>",
        "</head>"
    );
    let result = inspect_html_generator(input);
    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    assert_eq!(result.observations.len(), 1);
    assert_eq!(result.unknown_product_declarations, 0);
    assert_eq!(
        result.observations[0].reported_version.as_deref(),
        Some("6.8")
    );
    assert_invariants(input, &result);
}

#[test]
fn semantic_entities_fail_partial_regardless_of_attribute_order_or_case() {
    for attrs in [
        "NAME='gener&#97;tor' CoNtEnT='WordPress 6'",
        "CONTENT='WordPress 6' NaMe='gener&#97;tor'",
        "name='generator' content='WordPress &#54;'",
        "CONTENT='WordPress &#54;' NAME='GENERATOR'",
        "name='generator' content='Word&#80;ress 6'",
        "content='WordPress&#32;6' name='generator'",
        "name='generator' content='Unrecognized&amp; 6'",
        "name='gener&#97;tor'",
    ] {
        let input = document(&format!("<meta data-x='&quot;' {attrs}>{DECLARATION}"));
        let result = inspect_html_generator(&input);
        assert_eq!(
            reason(&result),
            Some(StopReason::UnsupportedEntity),
            "{attrs}"
        );
        assert!(result.observations.is_empty());
        assert_eq!(result.unknown_product_declarations, 0);
        let Coverage::Partial { at_byte, .. } = result.coverage else {
            unreachable!()
        };
        assert_eq!(input.as_bytes()[at_byte], b'&');
        assert!(at_byte > input.find("&quot;").unwrap());
    }
}

#[test]
fn duplicate_semantic_attributes_are_rejected_before_any_claim() {
    for attrs in [
        "name='generator' NAME='other' content='Drupal 10'",
        "name='other' NAME='generator' content='Drupal 10'",
        "content='Drupal 10' CONTENT='WordPress 6' name='generator'",
        "content='Drupal &#49;0' CONTENT='WordPress 6' name='generator'",
        "name='gener&#97;tor' NAME='generator' content='Drupal 10'",
        "name='generator' content='Drupal 10' data-x='&' DATA-X='second'",
    ] {
        let result = inspect_html_generator(&document(&format!("<meta {attrs}>{DECLARATION}")));
        assert_eq!(
            reason(&result),
            Some(StopReason::DuplicateAttribute),
            "{attrs}"
        );
        assert!(result.observations.is_empty());
    }
}

#[test]
fn bounded_ascii_separators_preserve_case_order_quote_and_unicode_offsets() {
    let separators = [
        " ".to_string(),
        "  ".to_string(),
        "\t".to_string(),
        "\n".to_string(),
        "\r\n".to_string(),
        "\x0c".to_string(),
        " \t\r\n\x0c ".to_string(),
        "\t".repeat(MAX_VERSION_SEPARATOR_BYTES),
    ];
    for product in ["WordPress", "dRuPaL"] {
        for separator in &separators {
            for quote in ['\'', '"'] {
                for attrs in [
                    format!("NaMe={quote}GeNeRaToR{quote} CONTENT={quote}{product}{separator}10.2{quote}"),
                    format!("CoNtEnT={quote}{product}{separator}10.2{quote} NAME={quote}GENERATOR{quote}"),
                ] {
                    let input = format!("\u{feff}<!--é🦀--><html><head><meta data-note='日本語&amp;' {attrs}></head>");
                    let result = inspect_html_generator(&input);
                    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
                    assert_eq!(result.observations.len(), 1);
                    let observation = &result.observations[0];
                    assert_eq!(observation.reported_version.as_deref(), Some("10.2"));
                    assert_eq!(observation.version_span.unwrap().start, input.find("10.2").unwrap());
                    assert_eq!(observation.product_span.start, input.find(product).unwrap());
                    assert_invariants(&input, &result);
                }
            }
        }
    }
}

#[test]
fn separator_overflow_unicode_and_trailing_whitespace_do_not_become_versions() {
    for suffix in [
        " ".repeat(MAX_VERSION_SEPARATOR_BYTES + 1) + "6",
        "\t\r\n".to_string(),
        " \u{a0}6".to_string(),
        " 6\t".to_string(),
        " 6\r\n".to_string(),
    ] {
        let input = document(&format!(
            "<meta name='generator' content='WordPress{suffix}'>"
        ));
        let result = inspect_html_generator(&input);
        assert_eq!(result.observations.len(), 1);
        assert_eq!(
            result.observations[0].version_status,
            VersionStatus::UnsupportedFormat
        );
        assert_eq!(result.observations[0].reported_version, None);
        assert_eq!(result.observations[0].version_span, None);
    }
    for value in ["WordPress\u{a0}6", " WordPress 6", "\tWordPress 6"] {
        let result = inspect_html_generator(&document(&format!(
            "<meta name='generator' content='{value}'>"
        )));
        assert!(result.observations.is_empty());
        assert_eq!(result.unknown_product_declarations, 1);
    }
}

#[test]
fn quoted_entity_spellings_never_change_actual_quote_boundaries() {
    let input = document(&format!(
        "<link data-note=\"&quot;> {DECLARATION} &#34; &apos;\"><meta data-note='&#39;> &quot;' name='generator' content='Drupal 10'>"
    ));
    let result = inspect_html_generator(&input);
    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    assert_eq!(result.observations.len(), 1);
    assert_eq!(result.observations[0].product, Product::Drupal);
    assert_invariants(&input, &result);

    let unclosed = format!("<html><head><link data-note=\"&quot;>{DECLARATION}</head>");
    let result = inspect_html_generator(&unclosed);
    assert_eq!(reason(&result), Some(StopReason::EndOfPreview));
    assert!(result.observations.is_empty());
    let no_separator = document("<meta data-x='&quot;'name='generator' content='Drupal 10'>");
    assert_eq!(
        reason(&inspect_html_generator(&no_separator)),
        Some(StopReason::MalformedTag)
    );
}

#[test]
fn unrelated_entities_do_not_weaken_script_or_comment_context_guards() {
    let input = document(&format!(
        "<!--&quot;{DECLARATION}--><script data-x='&quot;'>{DECLARATION}</script>{DECLARATION}"
    ));
    let result = inspect_html_generator(&input);
    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    assert_eq!(result.observations.len(), 1);
    assert_eq!(
        result.observations[0].tag_span.start,
        input.rfind(DECLARATION).unwrap()
    );
    for (head, expected) in [
        (
            format!("<script data-x='&quot;'><!--<script></script>{DECLARATION}"),
            StopReason::UnsupportedScriptEscape,
        ),
        (
            format!("<!--&amp; --!>{DECLARATION}"),
            StopReason::UnsupportedComment,
        ),
    ] {
        let result = inspect_html_generator(&document(&head));
        assert_eq!(reason(&result), Some(expected));
        assert!(result.observations.is_empty());
    }
}

#[test]
fn boolean_attributes_do_not_expose_script_string_declarations() {
    for attributes in [
        "defer async",
        "ASYNC DeFeR",
        "defer\tasync\r\n",
        "defer src='https://example.com/a.js?x=1&y=2' async",
        "data-note='&quot;🦀' async defer",
        "defer async=''",
    ] {
        let input = document(&format!(
            "<script {attributes}>const a = \"{DECLARATION}\"; const b = '&lt;/script&gt;';</script><meta hidden name='generator' content='Drupal 10' data-x>"
        ));
        let result = inspect_html_generator(&input);
        assert_eq!(
            result.coverage,
            Coverage::ExplicitHeadClosed,
            "{attributes}"
        );
        assert_eq!(result.observations.len(), 1);
        assert_eq!(result.observations[0].product, Product::Drupal);
        assert_eq!(
            result.observations[0].tag_span.start,
            input.find("<meta hidden").unwrap()
        );
        assert_invariants(&input, &result);
    }
}

#[test]
fn title_entities_are_opaque_and_never_become_tags_or_delimiters() {
    for text in [
        "A &amp; B",
        "&lt;/title&gt;",
        "&#60;/title&#62;",
        "&#x3C;/TiTlE&#x3E;",
        "&lt;meta name=&quot;generator&quot; content=&quot;WordPress 99&quot;&gt;",
        "&unknown; &#0; &#; &#x; &quot; &apos; é🦀 &",
    ] {
        let input = format!(
            "\u{feff}<html><head><title data-x>A {text}{DECLARATION}</TiTlE >{DECLARATION}</head>"
        );
        let result = inspect_html_generator(&input);
        assert_eq!(result.coverage, Coverage::ExplicitHeadClosed, "{text}");
        assert_eq!(result.observations.len(), 1);
        assert_eq!(
            result.observations[0].tag_span.start,
            input.rfind(DECLARATION).unwrap()
        );
        assert_invariants(&input, &result);
    }
}

#[test]
fn encoded_title_closer_cannot_end_an_unterminated_title() {
    let input = format!("<html><head><title>A &amp; B &lt;/title&gt;{DECLARATION}</head>");
    let result = inspect_html_generator(&input);
    assert_eq!(reason(&result), Some(StopReason::EndOfPreview));
    assert!(result.observations.is_empty());
    assert_eq!(result.tag_candidates_inspected, 3);
}

#[test]
fn meta_name_and_content_never_accept_boolean_or_unquoted_values() {
    for attrs in [
        "name content='Drupal 10'",
        "NAME content='Drupal 10'",
        "name='generator' content",
        "content name='generator'",
        "name",
        "content",
        "name='description' content",
        "name=generator content='Drupal 10'",
        "name='generator' content=Drupal",
        "hidden name=generator content='Drupal 10'",
        "name='generator' content='Drupal 10' x=unquoted",
    ] {
        let result = inspect_html_generator(&document(&format!("<meta {attrs}>{DECLARATION}")));
        assert_eq!(reason(&result), Some(StopReason::MalformedTag), "{attrs}");
        assert!(result.observations.is_empty());
    }
}

#[test]
fn boolean_attributes_preserve_duplicate_checks_and_quote_boundaries() {
    for tag in [
        "<script defer DEFER>",
        "<script defer='yes' DEFER>",
        "<script defer DEFER='yes'>",
        "<meta name='generator' content='Drupal 10' hidden HIDDEN>",
        "<meta name='generator' NAME content='Drupal 10'>",
        "<meta content='Drupal 10' CONTENT name='generator'>",
    ] {
        let result = inspect_html_generator(&document(&format!("{tag}{DECLARATION}")));
        assert_eq!(
            reason(&result),
            Some(StopReason::DuplicateAttribute),
            "{tag}"
        );
        assert!(result.observations.is_empty());
    }
    for tag in [
        "<script defer\"async\">",
        "<script defer/async>",
        "<script defer=true>",
        "<script data-x='&quot;'defer>",
        "<script defer data-x='&quot;'async>",
    ] {
        let result = inspect_html_generator(&document(&format!("{tag}{DECLARATION}</script>")));
        assert_eq!(reason(&result), Some(StopReason::MalformedTag), "{tag}");
        assert!(result.observations.is_empty());
    }
}

#[test]
fn boolean_attributes_consume_the_same_exact_attribute_budget() {
    for count in [MAX_ATTRIBUTES, MAX_ATTRIBUTES + 1] {
        let attrs = (0..count).map(|n| format!(" a{n}")).collect::<String>();
        let result = inspect_html_generator(&document(&format!("<meta{attrs}>")));
        assert_eq!(
            reason(&result),
            if count == MAX_ATTRIBUTES {
                None
            } else {
                Some(StopReason::AttributeLimit)
            }
        );
    }
}

#[test]
fn boolean_attributes_and_title_entities_do_not_relax_context_stops() {
    for (head, expected) in [
        (
            format!("<script defer async><!--<script></script>{DECLARATION}"),
            StopReason::UnsupportedScriptEscape,
        ),
        (
            format!("<!--&amp; --!>{DECLARATION}"),
            StopReason::UnsupportedComment,
        ),
        (
            format!("<title>A &amp; B</title><template hidden>{DECLARATION}</template>"),
            StopReason::UnsupportedHeadContext,
        ),
        (
            format!("<script defer/>{DECLARATION}"),
            StopReason::UnsupportedSelfClosing,
        ),
        (
            format!("<title>A &amp; B\0</title>{DECLARATION}"),
            StopReason::NullByte,
        ),
    ] {
        let result = inspect_html_generator(&document(&head));
        assert_eq!(reason(&result), Some(expected));
        assert!(result.observations.is_empty());
    }
}

#[test]
fn tag_limit_stop_cursor_is_raw_bytes_not_a_utf8_slicing_boundary() {
    // Independent review's exact repro: the cap falls on é's second byte.
    let input = format!("<html><head><meta data-x='{}é'></head>", "x".repeat(4081));
    let result = inspect_html_generator(&input);
    assert_eq!(
        result.coverage,
        Coverage::Partial {
            reason: StopReason::TagByteLimit,
            at_byte: 4108
        }
    );
    assert!(!input.is_char_boundary(4108));
    assert!(result.observations.is_empty());
    assert_invariants(&input, &result);

    let prefix = format!("<html><head>{DECLARATION}");
    let tag_prefix = "<meta data-x='";
    let input = format!(
        "{prefix}{tag_prefix}{}🦀'></head>",
        "x".repeat(MAX_TAG_BYTES - tag_prefix.len() - 1)
    );
    let result = inspect_html_generator(&input);
    let expected = prefix.len() + MAX_TAG_BYTES;
    assert_eq!(
        result.coverage,
        Coverage::Partial {
            reason: StopReason::TagByteLimit,
            at_byte: expected
        }
    );
    assert!(!input.is_char_boundary(expected));
    assert_eq!(result.observations.len(), 1);
    // Accepted observation spans retain valid boundaries despite the cursor.
    assert_invariants(&input, &result);
}

#[test]
fn absent_versions_are_nullable_not_reported() {
    for product in ["WordPress", "Drupal"] {
        let input = document(&format!("<meta name='generator' content='{product}'>"));
        let result = inspect_html_generator(&input);
        assert_eq!(result.observations[0].reported_version, None);
        assert_eq!(result.observations[0].version_span, None);
        assert_eq!(
            result.observations[0].version_status,
            VersionStatus::NotReported
        );
    }
}

#[test]
fn unsupported_versions_are_whole_value_rejected_not_prefix_extracted() {
    for suffix in [
        " ",
        " 6 ",
        " 6.8-dev",
        " 6.8.1+local",
        " v6",
        " 6 (https://example.com)",
        " 123456789012345678901234567890123",
        " .6",
        " 6.",
        " 6..8",
        " ６.８",
        " 6/8",
        " 6,8",
        " 6 SECRET_REJECTED_MARKER",
    ] {
        let input = document(&format!(
            "<meta name='generator' content='WordPress{suffix}'>"
        ));
        let result = inspect_html_generator(&input);
        assert_eq!(result.observations.len(), 1, "suffix {suffix:?}");
        assert_eq!(
            result.observations[0].version_status,
            VersionStatus::UnsupportedFormat
        );
        assert_eq!(result.observations[0].reported_version, None);
        assert_eq!(result.observations[0].version_span, None);
        assert!(!format!("{result:?}").contains("SECRET_REJECTED_MARKER"));
        assert_invariants(&input, &result);
    }
}

#[test]
fn product_matching_is_exact_and_never_exposes_unknown_values() {
    for value in [
        "NotWordPress 6",
        "WordPress.com 6",
        "WordPress6",
        "WordPress/6",
        " Drupal 10",
        "DRUPALISH 9",
        "WordPress-6",
        "secret_UNKNOWN_PRODUCT_MARKER",
        "",
        "other WordPress 6",
    ] {
        let result = inspect_html_generator(&document(&format!(
            "<meta name='generator' content='{value}'>"
        )));
        assert!(result.observations.is_empty());
        assert_eq!(result.unknown_product_declarations, 1);
        assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
        assert!(!format!("{result:?}").contains("UNKNOWN_PRODUCT_MARKER"));
    }
}

#[test]
fn only_complete_exact_generator_attributes_qualify() {
    for attributes in [
        "name='x-generator' content='WordPress 6'",
        "name='generator ' content='WordPress 6'",
        "property='generator' content='WordPress 6'",
        "name='generator'",
        "content='WordPress 6'",
        "data-name='generator' content='WordPress 6'",
        "name='generator' data-content='WordPress 6'",
    ] {
        let result = inspect_html_generator(&document(&format!("<meta {attributes}>")));
        assert!(result.observations.is_empty());
        assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    }
}

#[test]
fn skipped_supported_contexts_cannot_emit_decoy_tags() {
    let input = document(&format!(
        "<!--{DECLARATION}--><script>{DECLARATION}; const s = '</scriptx>'; let a = 1 && 2;</SCRIPT ><style>{DECLARATION} x::after {{content:'</styler>'}}</style><title>{DECLARATION}</title><link data-note=\"{DECLARATION}\"><base href='https://example.com/'>{DECLARATION}"
    ));
    let result = inspect_html_generator(&input);
    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    assert_eq!(result.observations.len(), 1);
    assert_eq!(
        result.observations[0].tag_span.start,
        input.rfind(DECLARATION).unwrap()
    );
    assert_invariants(&input, &result);
}

#[test]
fn requires_explicit_document_and_head_context() {
    for input in [
        DECLARATION.to_string(),
        format!("<head>{DECLARATION}</head>"),
        format!("<html>{DECLARATION}<head></head>"),
        format!("<html><body><head>{DECLARATION}</head>"),
        format!("plain text<html><head>{DECLARATION}</head>"),
        format!("<svg><html><head>{DECLARATION}</head>"),
    ] {
        let result = inspect_html_generator(&input);
        assert!(result.observations.is_empty());
        assert_ne!(result.coverage, Coverage::ExplicitHeadClosed);
    }
}

#[test]
fn stops_at_explicit_head_end_without_scanning_body_or_second_head() {
    let input = format!("<html><head></head><body>{DECLARATION}<head>{DECLARATION}</head>");
    let result = inspect_html_generator(&input);
    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    assert_eq!(result.tag_candidates_inspected, 3);
    assert!(result.observations.is_empty());
}

#[test]
fn unsupported_contexts_stop_before_descendants_and_never_resynchronize() {
    for opener in [
        "<template>",
        "<svg>",
        "<math>",
        "<noscript>",
        "<iframe>",
        "<object>",
        "<div>",
        "<body>",
        "<head>",
        "<html>",
        "<frameset>",
        "<textarea>",
        "<plaintext>",
        "<xmp>",
        "<noembed>",
        "<noframes>",
        "</html>",
    ] {
        let input = document(&format!("{opener}{DECLARATION}</template>{DECLARATION}"));
        let result = inspect_html_generator(&input);
        assert_eq!(
            reason(&result),
            Some(StopReason::UnsupportedHeadContext),
            "{opener}"
        );
        assert!(result.observations.is_empty());
    }
}

#[test]
fn script_escape_and_double_escape_constructs_stop_partial() {
    for body in [
        "<!--</script>",
        "<!--<script></script>",
        "<!--<script>--></script>",
        "let s = '<!--'; </script>",
    ] {
        let input = document(&format!(
            "<script>{body}{DECLARATION}</script>{DECLARATION}"
        ));
        let result = inspect_html_generator(&input);
        assert_eq!(reason(&result), Some(StopReason::UnsupportedScriptEscape));
        assert!(result.observations.is_empty());
    }
}

#[test]
fn html_script_termination_is_lexical_not_javascript_quote_aware() {
    let input = document(&format!("<script>const x = '</script>{DECLARATION}"));
    let result = inspect_html_generator(&input);
    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    assert_eq!(result.observations.len(), 1);
}

#[test]
fn malformed_comments_do_not_swallow_or_reinterpret_context() {
    for comment in [
        "<!-->",
        "<!--->",
        "<!-- x --!>",
        "<!-- x -- y -->",
        "<!--<!--x-->",
    ] {
        let input = document(&format!("{comment}{DECLARATION}"));
        let result = inspect_html_generator(&input);
        assert_eq!(
            reason(&result),
            Some(StopReason::UnsupportedComment),
            "{comment}"
        );
        assert!(result.observations.is_empty());
    }
}

#[test]
fn unterminated_comments_rawtext_and_tags_are_partial() {
    for tail in [
        "<!-- x",
        "<script> x",
        "<style> x",
        "<title> x",
        "<meta name='generator' content='WordPress 6",
        "<meta",
    ] {
        let result = inspect_html_generator(&format!("<html><head>{tail}"));
        assert_eq!(reason(&result), Some(StopReason::EndOfPreview), "{tail}");
        assert!(result.observations.is_empty());
    }
}

#[test]
fn malformed_attributes_end_tags_and_self_closing_contexts_are_partial() {
    for tag in [
        "<meta name=generator content='WordPress 6'>",
        "<meta name='generator'content='WordPress 6'>",
        "<meta name='generator' content='WordPress 6' name='generator'>",
        "<meta NAME='generator' name='generator'>",
        "<meta name='generator' content='WordPress 6' / >",
        "<meta boolean=unquoted>",
        "<meta =x>",
        "</head x='x'>",
        "</head/>",
        "<script/>",
        "<style/>",
        "<title/>",
        "<?xml version='1.0'?>",
        "<![CDATA[",
        "<!bogus>",
        "< meta>",
        "<<meta>",
    ] {
        let result = inspect_html_generator(&document(&format!("{tag}{DECLARATION}")));
        assert_ne!(result.coverage, Coverage::ExplicitHeadClosed, "{tag}");
        assert!(result.observations.is_empty(), "{tag}");
    }
    for input in ["<html/><head></head>", "<html><head/></head>"] {
        assert_eq!(
            reason(&inspect_html_generator(input)),
            Some(StopReason::UnsupportedSelfClosing)
        );
    }
}

#[test]
fn nulls_and_entities_never_decode_into_accepted_declarations() {
    for tag in [
        "<meta name='gener&#97;tor' content='WordPress 6'>",
        "<meta name='generator' content='WordPress &#54;'>",
        "<meta name='generator' content='Word&#80;ress 6'>",
        "<meta x='&' name='gener&#97;tor' content='Drupal 10'>",
    ] {
        let result = inspect_html_generator(&document(&format!("{tag}{DECLARATION}")));
        assert_eq!(reason(&result), Some(StopReason::UnsupportedEntity));
        assert!(result.observations.is_empty());
    }
    for head in [
        "\0",
        "<!--\0-->",
        "<script>\0</script>",
        "<meta name='generator' content='WordPress\0 6'>",
    ] {
        let result = inspect_html_generator(&document(&format!("{head}{DECLARATION}")));
        assert_eq!(reason(&result), Some(StopReason::NullByte));
        assert!(result.observations.is_empty());
    }
}

#[test]
fn doctypes_support_only_the_simple_html_declaration() {
    for doctype in [
        "<!doctype svg>",
        "<!doctype html PUBLIC 'x'>",
        "<!doctype html SYSTEM 'x'>",
        "<!doctypehtml>",
        "<!doctype html/>",
    ] {
        let input = format!("{doctype}{}", document(DECLARATION));
        let result = inspect_html_generator(&input);
        assert_eq!(reason(&result), Some(StopReason::UnsupportedDoctype));
        assert!(result.observations.is_empty());
    }
}

#[test]
fn valid_prior_observations_survive_a_later_partial_stop() {
    let result = inspect_html_generator(&document(&format!(
        "{DECLARATION}<template>{DECLARATION}</template>"
    )));
    assert_eq!(result.observations.len(), 1);
    assert_eq!(reason(&result), Some(StopReason::UnsupportedHeadContext));
}

#[test]
fn exact_candidate_limit_is_not_mislabeled_as_truncated() {
    let input = document(&"<meta>".repeat(MAX_TAG_CANDIDATES - 3));
    let result = inspect_html_generator(&input);
    assert_eq!(result.tag_candidates_inspected, MAX_TAG_CANDIDATES);
    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    assert!(!result.additional_tag_candidate_encountered);
}

#[test]
fn actual_next_candidate_sets_the_cap_without_inventing_skipped_counts() {
    let input = document(&"<meta>".repeat(MAX_TAG_CANDIDATES - 2));
    let result = inspect_html_generator(&input);
    assert_eq!(result.tag_candidates_inspected, MAX_TAG_CANDIDATES);
    assert_eq!(reason(&result), Some(StopReason::TagCandidateLimit));
    assert!(result.additional_tag_candidate_encountered);
    assert_invariants(&input, &result);
}

#[test]
fn candidate_cap_does_not_invent_next_tag_at_eof_or_non_tag_markup() {
    let prefix = format!("<html><head>{}", "<meta>".repeat(MAX_TAG_CANDIDATES - 2));
    for (tail, expected) in [
        ("", StopReason::EndOfPreview),
        ("  <!--safe comment-->", StopReason::EndOfPreview),
        ("<", StopReason::EndOfPreview),
        ("</", StopReason::EndOfPreview),
        ("<!bogus>", StopReason::MalformedTag),
    ] {
        let result = inspect_html_generator(&format!("{prefix}{tail}"));
        assert_eq!(reason(&result), Some(expected));
        assert_eq!(result.tag_candidates_inspected, MAX_TAG_CANDIDATES);
        assert!(!result.additional_tag_candidate_encountered);
    }
}

#[test]
fn comments_and_rawtext_decoys_do_not_consume_tag_candidates() {
    for wrapper in [0, 1] {
        let decoys = "<meta>".repeat(MAX_TAG_CANDIDATES + 1);
        let head = if wrapper == 0 {
            format!("<!--{decoys}-->")
        } else {
            format!("<script>{decoys}</script>")
        };
        let result = inspect_html_generator(&document(&head));
        assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
        assert_eq!(
            result.tag_candidates_inspected,
            if wrapper == 0 { 3 } else { 5 }
        );
    }
}

#[test]
fn exact_and_over_tag_byte_limits() {
    for delta in [0, 1] {
        let base = "<meta data-x=''>";
        let tag = format!(
            "<meta data-x='{}'>",
            "x".repeat(MAX_TAG_BYTES - base.len() + delta)
        );
        assert_eq!(tag.len(), MAX_TAG_BYTES + delta);
        let result = inspect_html_generator(&document(&tag));
        assert_eq!(
            reason(&result),
            if delta == 0 {
                None
            } else {
                Some(StopReason::TagByteLimit)
            }
        );
    }
}

#[test]
fn exact_and_over_attribute_limits() {
    for count in [MAX_ATTRIBUTES, MAX_ATTRIBUTES + 1] {
        let attrs = (0..count).map(|n| format!(" a{n}='x'")).collect::<String>();
        let result = inspect_html_generator(&document(&format!("<meta{attrs}>")));
        assert_eq!(
            reason(&result),
            if count == MAX_ATTRIBUTES {
                None
            } else {
                Some(StopReason::AttributeLimit)
            }
        );
    }
}

#[test]
fn exact_eight_observations_complete_ninth_sets_verified_omission() {
    let exact = inspect_html_generator(&document(&DECLARATION.repeat(MAX_OBSERVATIONS)));
    assert_eq!(exact.observations.len(), MAX_OBSERVATIONS);
    assert_eq!(exact.coverage, Coverage::ExplicitHeadClosed);
    assert!(!exact.additional_observation_encountered);
    let over = inspect_html_generator(&document(&DECLARATION.repeat(MAX_OBSERVATIONS + 1)));
    assert_eq!(over.observations.len(), MAX_OBSERVATIONS);
    assert_eq!(reason(&over), Some(StopReason::ObservationLimit));
    assert!(over.additional_observation_encountered);
}

#[test]
fn unknown_declarations_after_eight_are_counted_not_falsely_omitted() {
    let result = inspect_html_generator(&document(&format!(
        "{}<meta name='generator' content='Unknown'>",
        DECLARATION.repeat(8)
    )));
    assert_eq!(result.observations.len(), 8);
    assert_eq!(result.unknown_product_declarations, 1);
    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    assert!(!result.additional_observation_encountered);
}

#[test]
fn input_byte_bound_exact_over_and_utf8_boundary_are_honest() {
    for extra in [0, 1] {
        let input = format!("<html><head>{}", " ".repeat(MAX_INPUT_BYTES - 12 + extra));
        assert_eq!(input.len(), MAX_INPUT_BYTES + extra);
        let result = inspect_html_generator(&input);
        assert_eq!(result.input_bytes_considered, MAX_INPUT_BYTES);
        assert_eq!(result.input_limit_exceeded, extra != 0);
        assert_eq!(
            reason(&result),
            Some(if extra == 0 {
                StopReason::EndOfPreview
            } else {
                StopReason::InputLimit
            })
        );
    }
    let input = format!("<html><head>{}🦀", " ".repeat(MAX_INPUT_BYTES - 13));
    let result = inspect_html_generator(&input);
    assert_eq!(result.input_bytes_considered, MAX_INPUT_BYTES - 1);
    assert_eq!(reason(&result), Some(StopReason::InputLimit));
    assert_invariants(&input, &result);
}

#[test]
fn complete_head_inside_bounded_prefix_does_not_claim_body_coverage() {
    let input = format!("{}{}", document(DECLARATION), "x".repeat(MAX_INPUT_BYTES));
    let result = inspect_html_generator(&input);
    assert_eq!(result.coverage, Coverage::ExplicitHeadClosed);
    assert!(result.input_limit_exceeded);
    assert_eq!(result.observations.len(), 1);
}

#[test]
fn every_utf8_prefix_of_fixture_is_safe_and_has_valid_provenance() {
    let fixture = format!("\u{feff}<!--☃--><html lang='日本語'><head><title>🦀</title>{DECLARATION}<style>x</style><script>0</script></head>");
    for end in (0..=fixture.len()).filter(|end| fixture.is_char_boundary(*end)) {
        let input = &fixture[..end];
        let result = inspect_html_generator(input);
        assert_invariants(input, &result);
        if !input.ends_with("</head>") {
            assert_ne!(result.coverage, Coverage::ExplicitHeadClosed);
        }
    }
}

#[test]
fn deterministic_fuzz_like_mutations_never_panic_or_corrupt_spans() {
    let fixture = document(&format!(
        "<!--🦀--><script>0</script>{DECLARATION}<title>☃</title>"
    ));
    let mut state = 0x6a09e667f3bcc909_u64;
    let alphabet = [
        "<", ">", "'", "\"", "&", "\0", "/", "-", "=", " ", "\n", "é", "🦀", "script", "head",
        "meta", "!--",
    ];
    for _ in 0..20_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let mut input = fixture.clone();
        let mut at = (state as usize) % (input.len() + 1);
        while !input.is_char_boundary(at) {
            at -= 1;
        }
        let token = alphabet[((state >> 32) as usize) % alphabet.len()];
        if state & 1 == 0 && at < input.len() {
            let next = at + input[at..].chars().next().unwrap().len_utf8();
            input.replace_range(at..next, token);
        } else {
            input.insert_str(at, token);
        }
        let result = inspect_html_generator(&input);
        assert_invariants(&input, &result);
    }
}

#[test]
fn opaque_errors_and_debug_output_do_not_expose_source_or_rejected_values() {
    let marker = "PRIVATE_RAW_MARKER_abcdef";
    for input in [
        document(&format!(
            "<meta name='generator' content='WordPress {marker}'>"
        )),
        document(&format!("<meta name='generator' content='{marker}'>")),
        document(&format!(
            "<meta name='generator' content='WordPress 6 {marker}"
        )),
        document(&format!("<script><!--{marker}</script>{DECLARATION}")),
        document(&format!(
            "<template data-secret='{marker}'>{DECLARATION}</template>"
        )),
    ] {
        assert!(!format!("{:?}", inspect_html_generator(&input)).contains(marker));
    }
}
