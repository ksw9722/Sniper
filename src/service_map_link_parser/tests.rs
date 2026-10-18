use super::*;

fn one(value: &str) -> Report {
    inspect(&[RetainedHeader {
        name: "Link",
        value,
    }])
}

fn declaration(target: &str, rel: &str) -> String {
    format!("<{target}>; rel={rel}")
}

fn assert_invariants(headers: &[RetainedHeader<'_>], report: &Report) {
    assert!(report.observations.len() <= MAX_OBSERVATIONS);
    assert!(report.counts.headers_inspected <= MAX_HEADERS);
    assert!(report.counts.header_bytes_admitted <= MAX_HEADER_BYTES);
    assert!(report.counts.candidates_inspected <= MAX_CANDIDATES);
    assert!(report.counts.members_framed <= report.counts.candidates_inspected);
    assert!(report.counts.parameters_inspected <= MAX_CANDIDATES * MAX_PARAMETERS);
    assert!(report.counts.relation_tokens_inspected <= MAX_CANDIDATES * MAX_RELATION_TOKENS);
    assert!(
        report.counts.unknown_relation_tokens_inspected <= report.counts.relation_tokens_inspected
    );
    assert!(report.reasons.len() <= 12);
    assert_eq!(
        report.coverage == Coverage::Partial,
        !report.reasons.is_empty()
    );
    for obs in &report.observations {
        assert!(obs.header_index < report.counts.headers_inspected);
        let header = headers[obs.header_index];
        let value = header.value;
        assert_eq!(obs.header_value_bytes, value.len());
        assert!(header.name.eq_ignore_ascii_case("link"));
        assert_eq!(obs.header_name, "link");
        assert_eq!(obs.source, "response_link_header");
        assert_eq!(obs.strength, "literal_declaration");
        assert_eq!(obs.rule_id, obs.relation.rule_id());
        for span in [obs.link_value_span, obs.target_span, obs.relation_span] {
            assert!(span.start < span.end);
            assert!(span.end <= value.len());
            assert!(value.is_char_boundary(span.start));
            assert!(value.is_char_boundary(span.end));
        }
        assert_eq!(value.as_bytes()[obs.link_value_span.start], b'<');
        assert_eq!(obs.target_span.start, obs.link_value_span.start + 1);
        assert_eq!(value.as_bytes()[obs.target_span.end], b'>');
        assert!(obs.target_span.end < obs.relation_span.start);
        assert!(obs.relation_span.end <= obs.link_value_span.end);
        assert!(value[obs.relation_span.start..obs.relation_span.end]
            .eq_ignore_ascii_case(obs.relation.as_str()));
        assert_eq!(
            target_origin(&value[obs.target_span.start..obs.target_span.end]).as_deref(),
            Some(obs.target_origin.as_str())
        );
        assert_eq!(
            target_origin(&obs.target_origin).as_deref(),
            Some(obs.target_origin.as_str())
        );
        assert!(obs.target_origin.len() <= MAX_TARGET_ORIGIN_BYTES);
        assert!(!obs.target_origin.contains(['?', '#', '@', '\\']));
        assert!(obs.target_origin.is_ascii());
        let url = Url::parse(&obs.target_origin).unwrap();
        assert_eq!(url.path(), "/");
        assert!(url.query().is_none());
        assert!(url.fragment().is_none());
        assert_eq!(url.username(), "");
        assert!(url.password().is_none());
    }
}

#[test]
fn fixed_relations_case_and_exact_spans() {
    let value = " \t<HTTPS://EXAMPLE.COM:443/private?secret#fragment>; REL=\"SERVICE-DOC service-desc describedby preCONNECT\" \t";
    let report = one(value);
    assert_eq!(report.coverage, Coverage::CompleteWithinFixedGrammar);
    assert_eq!(report.observations.len(), 4);
    assert_eq!(report.counts.candidates_inspected, 1);
    assert_eq!(report.counts.relation_tokens_inspected, 4);
    for obs in &report.observations {
        assert_eq!(obs.target_origin, "https://example.com");
        assert_eq!(obs.link_value_span.start, 2);
        assert_eq!(obs.link_value_span.end, value.len() - 2);
    }
    assert_invariants(
        &[RetainedHeader {
            name: "Link",
            value,
        }],
        &report,
    );
}

#[test]
fn uri_and_quoted_commas_semicolons_are_not_member_boundaries() {
    let value = r#"<https://example.com/a,b;c?q=x,y;z#fragment>; title="label,semi;colon"; rel="service-doc preconnect", <http://example.com:80/b>; rel=describedby"#;
    let report = one(value);
    assert_eq!(report.observations.len(), 3);
    assert_eq!(report.counts.candidates_inspected, 2);
    assert_eq!(
        report.observations[0].link_value_span,
        report.observations[1].link_value_span
    );
    assert_eq!(
        report.observations[0].target_span,
        report.observations[1].target_span
    );
    assert_ne!(
        report.observations[0].relation_span,
        report.observations[1].relation_span
    );
    assert_invariants(
        &[RetainedHeader {
            name: "link",
            value,
        }],
        &report,
    );
}

#[test]
fn original_field_indexes_and_same_origin_members_stay_distinct() {
    let headers = [
        RetainedHeader {
            name: "Server",
            value: "not inspected as a link",
        },
        RetainedHeader {
            name: "LINK",
            value: "<https://example.com/a>; rel=service-doc",
        },
        RetainedHeader {
            name: "x-link",
            value: "<https://example.com/no>; rel=preconnect",
        },
        RetainedHeader {
            name: "LiNk",
            value:
                "<https://example.com/b>; rel=service-doc, <https://example.com/c>; rel=service-doc",
        },
    ];
    let report = inspect(&headers);
    assert_eq!(
        report
            .observations
            .iter()
            .map(|o| o.header_index)
            .collect::<Vec<_>>(),
        vec![1, 3, 3]
    );
    assert_eq!(report.counts.link_fields_inspected, 2);
    assert_ne!(
        report.observations[1].link_value_span,
        report.observations[2].link_value_span
    );
    assert_invariants(&headers, &report);
}

#[test]
fn first_rel_wins_even_when_empty_unknown_valueless_or_escaped() {
    for first in [
        "rel=unknown",
        "rel=\"\"",
        "rel",
        "rel=\"pre\\connect\"",
        "REL=\"https://example.com/extension\"",
    ] {
        let report = one(&format!("<https://example.com>; {first}; rel=preconnect"));
        assert!(report.observations.is_empty(), "{first}");
    }
    let report = one("<https://example.com>; rel=service-doc; REL=preconnect");
    assert_eq!(report.observations.len(), 1);
    assert_eq!(report.observations[0].relation, Relation::ServiceDoc);
    assert_eq!(report.counts.relation_tokens_inspected, 1);
    assert_eq!(report.counts.parameters_inspected, 2);
}

#[test]
fn every_anchor_form_suppresses_whole_member() {
    for anchor in [
        "anchor",
        "anchor=\"\"",
        "ANCHOR=\"#x\"",
        "anchor=\"https://example.com/elsewhere\"",
    ] {
        for value in [
            format!("<https://example.com>; rel=preconnect; {anchor}"),
            format!("<https://example.com>; {anchor}; rel=preconnect"),
        ] {
            let report = one(&value);
            assert!(report.observations.is_empty());
            assert_eq!(report.reasons, vec![Reason::AnchorPresent]);
            assert_eq!(report.counts.relation_tokens_inspected, 0);
        }
    }
}

#[test]
fn unrelated_quoted_pairs_are_framed_without_unescaping_rel() {
    let value = r#"<https://example.com>; title="a\"; anchor=\"x, \\ end"; rel=preconnect"#;
    let report = one(value);
    assert_eq!(report.observations.len(), 1);
    assert_eq!(report.counts.parameters_inspected, 2);
    assert_eq!(report.coverage, Coverage::CompleteWithinFixedGrammar);
    let escaped_rel = one(r#"<https://example.com>; rel="service\-doc preconnect""#);
    assert!(escaped_rel.observations.is_empty());
    assert_eq!(escaped_rel.reasons, vec![Reason::UnsupportedRelationSyntax]);
}

#[test]
fn ordinary_parameter_tokens_bws_and_empty_list_elements() {
    let report =
        one(" , \t, <https://example.com> \t; empty ; title = token ; rel = preconnect, ,");
    assert_eq!(report.observations.len(), 1);
    assert_eq!(report.counts.parameters_inspected, 3);
    assert_eq!(report.coverage, Coverage::CompleteWithinFixedGrammar);
    assert_eq!(one(" , \t, ").counts.candidates_inspected, 0);
}

#[test]
fn malformed_field_never_scans_ahead_but_other_fields_survive() {
    let good = "<https://example.com>; rel=preconnect";
    for bad in [
        "garbage, <https://example.com>; rel=service-doc",
        "<https://example.com; rel=service-doc, <https://example.com>; rel=service-doc",
        "<https://example.com>; rel=preconnect ;, <https://example.com>; rel=service-doc",
        "<https://example.com>; rel=preconnect junk, <https://example.com>; rel=service-doc",
        "<https://example.com>; rel=preconnect; title=\"unterminated, <https://example.com>; rel=service-doc",
        "<https://example.com>; rel=preconnect; title=\"bad\\",
        "<https://example.com>; rel=preconnect; title=\"bad\n\", <https://example.com>; rel=service-doc",
        "<https://example.com>; rel=preconnect; title=\"bad\\\n\", <https://example.com>; rel=service-doc",
        "<https://example.com>; rel=preconnect; title=\"bad\u{7f}\", <https://example.com>; rel=service-doc",
        "<https://example.com>; rel=, <https://example.com>; rel=service-doc",
    ] {
        let first = format!("{good}, {bad}");
        let headers = [RetainedHeader { name: "Link", value: &first }, RetainedHeader { name: "link", value: good }];
        let report = inspect(&headers);
        assert_eq!(report.observations.len(), 2, "{bad}");
        assert_eq!(report.reasons, vec![Reason::MalformedField], "{bad}");
        assert_eq!(report.observations[0].header_index, 0);
        assert_eq!(report.observations[1].header_index, 1);
        assert_invariants(&headers, &report);
    }
}

#[test]
fn unicode_attrs_preserve_original_utf8_byte_offsets() {
    let value = "<https://example.com/first>; title=\"é雪🦀,;\"; rel=unknown, <https://example.com/second>; title=\"π\"; rel=\"SERVICE-DOC preconnect\"";
    let report = one(value);
    assert_eq!(report.observations.len(), 2);
    assert_eq!(report.counts.unknown_relation_tokens_inspected, 1);
    let obs = &report.observations[0];
    assert_eq!(
        obs.link_value_span.start,
        value.find("<https://example.com/second>").unwrap()
    );
    assert_eq!(obs.relation_span.start, value.find("SERVICE-DOC").unwrap());
    assert!(obs.relation_span.start > value[..obs.relation_span.start].chars().count());
    assert_invariants(
        &[RetainedHeader {
            name: "link",
            value,
        }],
        &report,
    );
}

#[test]
fn relation_grammar_unknown_counts_and_no_partial_member_emission() {
    let report = one("<https://example.com>; rel=\"unknown preconnect UNKNOWN other-name.v1\"");
    assert_eq!(report.observations.len(), 1);
    assert_eq!(report.counts.relation_tokens_inspected, 4);
    assert_eq!(report.counts.unknown_relation_tokens_inspected, 3);
    assert_eq!(report.coverage, Coverage::CompleteWithinFixedGrammar);
    for rel in [
        "\" preconnect\"",
        "\"preconnect \"",
        "\"preconnect\tservice-doc\"",
        "\"preconnect 1bad\"",
        "\"preconnect service_doc\"",
        "\"preconnect https://example.com/rel\"",
        "\"prëconnect\"",
    ] {
        let report = one(&declaration("https://example.com", rel));
        assert!(report.observations.is_empty(), "{rel}");
        assert_eq!(report.reasons, vec![Reason::UnsupportedRelationSyntax]);
    }
    let report = one("<https://example.com>; rel=\"preconnect    service-doc\"");
    assert_eq!(report.observations.len(), 2);
    assert_eq!(
        one("<https://example.com>").reasons,
        vec![Reason::MissingRelation]
    );
}

#[test]
fn strict_absolute_target_validation_excludes_repairs_credentials_and_numeric_aliases() {
    for target in [
        "",
        "/path",
        "//example.com/path",
        "http:example.com",
        "https:/example.com",
        "https:///example.com",
        "ftp://example.com",
        "wss://example.com",
        "https://user:password@example.com/path",
        "https://@example.com",
        "https://example.com\\x",
        "https://example.com/a b",
        " https://example.com",
        "https://example.com/\n",
        "https://example.com/\u{7f}",
        "https://éxample.com",
        "https://example.com/雪",
        "https://%65xample.com",
        "https://example.com/%",
        "https://example.com/%1",
        "https://example.com/%zz",
        "https://example.com/#a#b",
        "https://example.com/<x",
        "https://example.com/\"x",
        "https://example.com/[x]",
        "https://example.com/{x}",
        "https://127.1",
        "https://2130706433",
        "https://0x7f000001",
        "https://0177.0.0.1",
        "https://127.0.0.1.",
        "https://127.00.0.1",
        "https://256.0.0.1",
        "https://[::1",
        "https://::1",
        "https://[fe80::1%25eth0]",
        "https://[v1.example]",
        "https://[::1]junk",
        "https://example.com:",
        "https://example.com:65536",
        "https://example.com:0443",
        "https://example.com:443:80",
        "https://-example.com",
        "https://example-.com",
        "https://example..com",
        "https://example_com",
        "https://example.com\t",
        "https://example.com\r",
    ] {
        assert_eq!(target_origin(target), None, "{target:?}");
        assert!(
            one(&declaration(target, "preconnect"))
                .observations
                .is_empty(),
            "{target:?}"
        );
    }
}

#[test]
fn safe_origin_normalization_and_long_valid_target() {
    for (target, expected) in [
        (
            "HTTPS://EXAMPLE.COM:443/path?query#fragment",
            "https://example.com",
        ),
        ("http://EXAMPLE.COM:80", "http://example.com"),
        ("https://example.com:8443/a", "https://example.com:8443"),
        (
            "https://[2001:0DB8:0000:0000:0000:0000:0000:0001]:443/a",
            "https://[2001:db8::1]",
        ),
        (
            "http://[::ffff:192.0.2.1]:8080",
            "http://[::ffff:c000:201]:8080",
        ),
        ("https://192.0.2.1/a", "https://192.0.2.1"),
        ("https://example.com./", "https://example.com."),
        (
            "https://example.com/a/../b?q=/:?@,;%20#frag?",
            "https://example.com",
        ),
    ] {
        assert_eq!(target_origin(target).as_deref(), Some(expected), "{target}");
    }
    let target = format!("https://example.com/{}", "a".repeat(3000));
    let report = one(&declaration(&target, "service-doc"));
    assert_eq!(report.observations[0].target_origin, "https://example.com");
    assert_eq!(
        report.observations[0].target_span.end - report.observations[0].target_span.start,
        target.len()
    );
}

#[test]
fn header_count_exact_and_over_budget() {
    let mut headers = vec![
        RetainedHeader {
            name: "X",
            value: ""
        };
        MAX_HEADERS
    ];
    let exact = inspect(&headers);
    assert_eq!(exact.coverage, Coverage::CompleteWithinFixedGrammar);
    assert_eq!(exact.counts.headers_inspected, MAX_HEADERS);
    headers.push(RetainedHeader {
        name: "Link",
        value: "<https://example.com>; rel=preconnect",
    });
    let over = inspect(&headers);
    assert_eq!(over.counts.headers_inspected, MAX_HEADERS);
    assert_eq!(over.reasons, vec![Reason::HeaderCountLimit]);
    assert!(over.observations.is_empty());
}

#[test]
fn header_bytes_exact_and_over_budget_preserve_earlier_work() {
    let good = "<https://example.com>; rel=preconnect";
    let fill = "x".repeat(MAX_HEADER_BYTES - 4 - good.len() - 1);
    let mut headers = vec![
        RetainedHeader {
            name: "link",
            value: good,
        },
        RetainedHeader {
            name: "X",
            value: &fill,
        },
    ];
    let exact = inspect(&headers);
    assert_eq!(exact.counts.header_bytes_admitted, MAX_HEADER_BYTES);
    assert_eq!(exact.coverage, Coverage::CompleteWithinFixedGrammar);
    headers.push(RetainedHeader {
        name: "X",
        value: "",
    });
    let over = inspect(&headers);
    assert_eq!(over.reasons, vec![Reason::HeaderByteLimit]);
    assert_eq!(over.observations, exact.observations);
    assert_eq!(over.counts.headers_inspected, 2);
    let huge_name = "x".repeat(MAX_HEADER_BYTES + 1);
    assert_eq!(
        inspect(&[RetainedHeader {
            name: &huge_name,
            value: ""
        }])
        .reasons,
        vec![Reason::HeaderByteLimit]
    );
}

#[test]
fn link_field_and_target_byte_exact_and_over_budget() {
    let prefix = "<https://example.com/";
    let suffix = ">; rel=preconnect";
    let exact = format!(
        "{prefix}{}{suffix}",
        "a".repeat(MAX_LINK_FIELD_BYTES - prefix.len() - suffix.len())
    );
    assert_eq!(exact.len(), MAX_LINK_FIELD_BYTES);
    assert_eq!(one(&exact).observations.len(), 1);
    let over = format!("{exact} ");
    let report = one(&over);
    assert_eq!(report.reasons, vec![Reason::LinkFieldByteLimit]);
    assert_eq!(report.counts.candidates_inspected, 0);
    let target = format!(
        "https://example.com/{}",
        "a".repeat(MAX_TARGET_BYTES - "https://example.com/".len())
    );
    assert_eq!(
        target_origin(&target).as_deref(),
        Some("https://example.com")
    );
    assert_eq!(target_origin(&(target + "a")), None);
}

#[test]
fn candidate_budget_exact_and_encountered_overflow_only() {
    let member = "<https://example.com>; rel=other";
    let field = std::iter::repeat_n(member, 64)
        .collect::<Vec<_>>()
        .join(",");
    let mut headers = vec![
        RetainedHeader {
            name: "link",
            value: &field
        };
        2
    ];
    let exact = inspect(&headers);
    assert_eq!(exact.counts.candidates_inspected, MAX_CANDIDATES);
    assert_eq!(
        exact.counts.unknown_relation_tokens_inspected,
        MAX_CANDIDATES
    );
    assert_eq!(exact.coverage, Coverage::CompleteWithinFixedGrammar);
    headers.push(RetainedHeader {
        name: "link",
        value: " , ",
    });
    assert_eq!(
        inspect(&headers).coverage,
        Coverage::CompleteWithinFixedGrammar
    );
    headers.push(RetainedHeader {
        name: "link",
        value: member,
    });
    let over = inspect(&headers);
    assert_eq!(over.counts.candidates_inspected, MAX_CANDIDATES);
    assert_eq!(
        over.counts.unknown_relation_tokens_inspected,
        MAX_CANDIDATES
    );
    assert_eq!(over.reasons, vec![Reason::CandidateLimit]);
}

#[test]
fn parameter_budget_exact_and_overflow_suppresses_member_and_field_tail() {
    let exact = format!(
        "<https://example.com>;rel=preconnect{}",
        ";x=y".repeat(MAX_PARAMETERS - 1)
    );
    let report = one(&exact);
    assert_eq!(report.counts.parameters_inspected, MAX_PARAMETERS);
    assert_eq!(report.observations.len(), 1);
    let over = format!("{exact};anchor=\"\", <https://example.com>;rel=preconnect");
    let report = one(&over);
    assert_eq!(report.counts.candidates_inspected, 1);
    assert_eq!(report.counts.parameters_inspected, MAX_PARAMETERS);
    assert_eq!(report.counts.members_framed, 0);
    assert_eq!(report.reasons, vec![Reason::ParameterLimit]);
    assert!(report.observations.is_empty());
}

#[test]
fn relation_budget_exact_and_overflow_never_exports_partial_member() {
    let rel = format!(
        "\"preconnect {}\"",
        std::iter::repeat_n("other", MAX_RELATION_TOKENS - 1)
            .collect::<Vec<_>>()
            .join(" ")
    );
    let exact = one(&declaration("https://example.com", &rel));
    assert_eq!(exact.counts.relation_tokens_inspected, MAX_RELATION_TOKENS);
    assert_eq!(
        exact.counts.unknown_relation_tokens_inspected,
        MAX_RELATION_TOKENS - 1
    );
    assert_eq!(exact.observations.len(), 1);
    let rel = rel.replace("\"preconnect ", "\"preconnect other ");
    let over = one(&format!(
        "{},<https://example.com>;rel=service-doc",
        declaration("https://example.com", &rel)
    ));
    assert_eq!(
        over.counts.relation_tokens_inspected,
        MAX_RELATION_TOKENS + 1
    );
    assert_eq!(
        over.counts.unknown_relation_tokens_inspected,
        MAX_RELATION_TOKENS - 1
    );
    assert_eq!(over.observations.len(), 1);
    assert_eq!(over.observations[0].relation, Relation::ServiceDoc);
    assert_eq!(over.reasons, vec![Reason::RelationTokenLimit]);
}

#[test]
fn observation_budget_flags_only_encountered_verified_omission() {
    let member = "<https://example.com>;rel=preconnect";
    let exact = std::iter::repeat_n(member, MAX_OBSERVATIONS)
        .collect::<Vec<_>>()
        .join(",");
    let report = one(&exact);
    assert_eq!(report.observations.len(), MAX_OBSERVATIONS);
    assert_eq!(report.coverage, Coverage::CompleteWithinFixedGrammar);
    let unknown = one(&format!("{exact},<https://example.com>;rel=other"));
    assert_eq!(unknown.coverage, Coverage::CompleteWithinFixedGrammar);
    assert_eq!(unknown.counts.candidates_inspected, MAX_OBSERVATIONS + 1);
    let over = one(&format!("{exact},{member}"));
    assert_eq!(
        over.observations,
        report
            .observations
            .iter()
            .map(|o| {
                let mut o = o.clone();
                o.header_value_bytes = exact.len() + 1 + member.len();
                o
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(over.counts.candidates_inspected, MAX_OBSERVATIONS + 1);
    assert_eq!(over.reasons, vec![Reason::ObservationLimit]);
}

#[test]
fn host_limits_and_origin_output_bound() {
    let domain = format!(
        "{}.{}.{}.{}.",
        "a".repeat(63),
        "b".repeat(63),
        "c".repeat(63),
        "d".repeat(61)
    );
    assert_eq!(domain.len(), 254);
    let origin = format!("https://{domain}:65535");
    assert_eq!(origin.len(), MAX_TARGET_ORIGIN_BYTES);
    assert_eq!(target_origin(&origin), Some(origin));
    assert_eq!(
        target_origin(&format!("https://{}.example.com", "a".repeat(64))),
        None
    );
    assert_eq!(target_origin(&format!("https://x{domain}")), None);
}

#[test]
fn output_contains_only_origins_fixed_tokens_offsets_and_counts() {
    let value = r#"<https://EXAMPLE.COM/PRIVATE_PATH_SENTINEL?PRIVATE_QUERY_SENTINEL#PRIVATE_FRAGMENT_SENTINEL>; title="PRIVATE_ATTR_SENTINEL"; rel="PRIVATE-REL-SENTINEL preconnect", <https://PRIVATE_USER_SENTINEL:PRIVATE_PASSWORD_SENTINEL@example.com>; rel=service-doc"#;
    let report = one(value);
    assert_eq!(report.observations.len(), 1);
    let output = format!("{report:?}");
    for sentinel in [
        "PRIVATE_PATH_SENTINEL",
        "PRIVATE_QUERY_SENTINEL",
        "PRIVATE_FRAGMENT_SENTINEL",
        "PRIVATE_ATTR_SENTINEL",
        "PRIVATE-REL-SENTINEL",
        "PRIVATE_USER_SENTINEL",
        "PRIVATE_PASSWORD_SENTINEL",
    ] {
        assert!(!output.contains(sentinel));
    }
    assert_eq!(report.counts.unknown_relation_tokens_inspected, 1);
    assert_eq!(report.reasons, vec![Reason::UnsupportedTarget]);
    assert_invariants(
        &[RetainedHeader {
            name: "Link",
            value,
        }],
        &report,
    );
}

#[test]
fn deterministic_ascii_and_utf8_mutations_never_panic_leak_or_break_spans() {
    let seeds = [
        "<https://example.com/a,b;c?q=%20#f>;rel=\"service-doc preconnect\";title=\"雪,é;🦀\"",
        "<https://example.com>;rel=unknown, <https://example.com:8443>;rel=describedby",
        "<https://[2001:db8::1]>/broken;rel=preconnect",
        "<https://example.com>;title=\"a\\\"b\";rel=service-desc;anchor=\"\"",
    ];
    let mut tested = 0;
    for seed in seeds {
        for index in 0..=seed.len() {
            if !seed.is_char_boundary(index) {
                continue;
            }
            for byte in 0u8..=127 {
                let mut mutated = seed[..index].to_string();
                mutated.push(byte as char);
                mutated.push_str(&seed[index..]);
                let headers = [RetainedHeader {
                    name: "LiNk",
                    value: &mutated,
                }];
                let report = inspect(&headers);
                assert_eq!(inspect(&headers), report);
                assert_invariants(&headers, &report);
                tested += 1;
            }
            for insert in [
                "é", "雪", "🦀", "\u{0085}", "\u{2028}", "\u{200b}", "\u{fffd}",
            ] {
                let mutated = format!("{}{insert}{}", &seed[..index], &seed[index..]);
                let headers = [RetainedHeader {
                    name: "link",
                    value: &mutated,
                }];
                let report = inspect(&headers);
                assert_eq!(inspect(&headers), report);
                assert_invariants(&headers, &report);
                tested += 1;
            }
            if index < seed.len() {
                let next = index + seed[index..].chars().next().unwrap().len_utf8();
                let mutated = format!("{}{}", &seed[..index], &seed[next..]);
                let headers = [RetainedHeader {
                    name: "link",
                    value: &mutated,
                }];
                let report = inspect(&headers);
                assert_eq!(inspect(&headers), report);
                assert_invariants(&headers, &report);
                tested += 1;
            }
        }
    }
    assert!(tested > 30_000);
}

#[test]
fn deterministic_arbitrary_utf8_and_boundary_lengths() {
    let mut state = 0x71d9_f42a_u64;
    for iteration in 0..1024 {
        let length = match iteration % 8 {
            0 => 0,
            1 => 1,
            2 => 4095,
            3 => 4096,
            4 => 4097,
            5 => 32768,
            _ => iteration % 512,
        };
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            bytes.push(state as u8);
        }
        let value = String::from_utf8_lossy(&bytes);
        let headers = [RetainedHeader {
            name: if iteration % 2 == 0 { "link" } else { "LINK" },
            value: &value,
        }];
        let report = inspect(&headers);
        assert_eq!(inspect(&headers), report);
        assert_invariants(&headers, &report);
    }
}

#[test]
fn readme_golden_metadata_example() {
    let value = "<https://EXAMPLE.com:443/docs?key=PRIVATE#frag>; rel=\"service-doc preconnect\"";
    let report = one(value);
    assert_eq!(value.len(), 77);
    assert_eq!(report.observations.len(), 2);
    assert_eq!(
        report.observations[0],
        Observation {
            relation: Relation::ServiceDoc,
            target_origin: "https://example.com".into(),
            source: "response_link_header",
            strength: "literal_declaration",
            header_name: "link",
            header_index: 0,
            header_value_bytes: 77,
            link_value_span: Span { start: 0, end: 77 },
            target_span: Span { start: 1, end: 46 },
            relation_span: Span { start: 54, end: 65 },
            rule_id: "response_link.service_doc.v1",
        }
    );
    assert_eq!(
        report.observations[1].relation_span,
        Span { start: 66, end: 76 }
    );
    assert_eq!(report.observations[1].relation, Relation::Preconnect);
}
