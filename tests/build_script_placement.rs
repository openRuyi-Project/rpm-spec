//! Integration coverage for RPM 4.20 build-script placement.

use std::fmt::Write as _;

use rpm_spec::ast::{
    BuildScriptKind, BuildScriptPlacement, Section, ShellBody, SpecFile, SpecItem, TextSegment,
};
use rpm_spec::parse_result::codes;
use rpm_spec::parser::{parse_str, parse_str_with_spans};
use rpm_spec::printer::print;

fn build_scripts<T>(
    spec: &SpecFile<T>,
) -> Vec<(BuildScriptKind, BuildScriptPlacement, &ShellBody<T>, &T)> {
    spec.items
        .iter()
        .filter_map(|item| match item {
            SpecItem::Section(section) => match section.as_ref() {
                Section::BuildScript {
                    kind,
                    placement,
                    body,
                    data,
                } => Some((*kind, *placement, body, data)),
                _ => None,
            },
            _ => None,
        })
        .collect()
}

#[test]
fn every_build_script_kind_accepts_prepend_and_append() {
    let kinds = [
        ("prep", BuildScriptKind::Prep),
        ("conf", BuildScriptKind::Conf),
        (
            "generate_buildrequires",
            BuildScriptKind::GenerateBuildRequires,
        ),
        ("build", BuildScriptKind::Build),
        ("install", BuildScriptKind::Install),
        ("check", BuildScriptKind::Check),
        ("clean", BuildScriptKind::Clean),
    ];
    let placements = [
        ("-p", BuildScriptPlacement::Prepend, "before"),
        ("-a", BuildScriptPlacement::Append, "after"),
    ];
    let mut source = String::new();
    let mut expected = Vec::new();

    for (keyword, kind) in kinds {
        for (flag, placement, label) in placements {
            writeln!(source, "%{keyword} {flag}\necho {label}-{keyword}").unwrap();
            expected.push((kind, placement, format!("echo {label}-{keyword}")));
        }
    }

    let parsed = parse_str(&source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let actual: Vec<_> = build_scripts(&parsed.spec)
        .into_iter()
        .map(|(kind, placement, body, _)| {
            assert_eq!(body.lines.len(), 1);
            let line = body.lines[0]
                .literal_str()
                .expect("generated body line is literal")
                .to_owned();
            (kind, placement, line)
        })
        .collect();
    assert_eq!(actual, expected);

    let printed = print(&parsed.spec);
    let reparsed = parse_str(&printed);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
    assert_eq!(parsed.spec, reparsed.spec);
}

#[test]
fn build_script_spans_cover_their_header_and_body() {
    let source = "\
%prep -p
echo prep-before
%build
echo build-main
%install -a
echo install-after
";
    let parsed = parse_str_with_spans(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let slices: Vec<_> = build_scripts(&parsed.spec)
        .into_iter()
        .map(|(_, _, _, span)| {
            source
                .get(span.start_byte..span.end_byte)
                .expect("parser span must slice its source")
        })
        .collect();

    assert_eq!(
        slices,
        vec![
            "%prep -p\necho prep-before\n",
            "%build\necho build-main\n",
            "%install -a\necho install-after\n",
        ]
    );
}

#[test]
fn repeated_fragments_keep_source_order_without_a_main_section() {
    let source = "\
%install -a
echo append-one
%install -p
echo prepend
%install -a
echo append-two
";
    let parsed = parse_str(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let actual: Vec<_> = build_scripts(&parsed.spec)
        .into_iter()
        .map(|(_, placement, body, _)| {
            (
                placement,
                body.lines[0]
                    .literal_str()
                    .expect("test body line is literal"),
            )
        })
        .collect();
    assert_eq!(
        actual,
        vec![
            (BuildScriptPlacement::Append, "echo append-one"),
            (BuildScriptPlacement::Prepend, "echo prepend"),
            (BuildScriptPlacement::Append, "echo append-two"),
        ]
    );
}

#[test]
fn invalid_header_arguments_do_not_create_build_scripts() {
    for source in [
        "%build -x\n",
        "%build -pfoo\n",
        "%build -a -p\n",
        "%build %{?placement:-a}\n",
        "%build-p\n",
    ] {
        let parsed = parse_str(source);
        assert!(
            parsed.diagnostics.iter().any(|diagnostic| {
                diagnostic.code.as_deref() == Some(codes::W_LINE_NOT_RECOGNIZED)
            }),
            "missing recovery diagnostic for {source:?}"
        );
        assert!(
            build_scripts(&parsed.spec).is_empty(),
            "invalid header produced a build script for {source:?}"
        );
    }
}

#[cfg(feature = "serde")]
#[test]
fn serde_defaults_a_missing_placement_to_main() {
    let encoded = r#"{
        "BuildScript": {
            "kind": "Build",
            "body": { "lines": [], "conditionals": [] },
            "data": null
        }
    }"#;
    let decoded: Section<()> = serde_json::from_str(encoded).expect("old section deserializes");
    assert!(matches!(
        decoded,
        Section::BuildScript {
            placement: BuildScriptPlacement::Main,
            ..
        }
    ));
}

#[test]
fn heredoc_tags_remain_inside_the_install_body() {
    let source = "\
Name: libaio
Version: 0.3.113

%install -a
cat > libaio.pc <<'EOF'
Name: libaio
Version: %{version}
EOF
";
    let parsed = parse_str(source);
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);

    let preamble_count = parsed
        .spec
        .items
        .iter()
        .filter(|item| matches!(item, SpecItem::Preamble(_)))
        .count();
    assert_eq!(preamble_count, 2);

    let scripts = build_scripts(&parsed.spec);
    assert_eq!(scripts.len(), 1);
    let (kind, placement, body, _) = scripts[0];
    assert_eq!(kind, BuildScriptKind::Install);
    assert_eq!(placement, BuildScriptPlacement::Append);
    assert_eq!(body.lines.len(), 4);
    assert_eq!(body.lines[1].literal_str(), Some("Name: libaio"));
    assert!(body.lines[2].segments.iter().any(
        |segment| matches!(segment, TextSegment::Macro(reference) if reference.name == "version")
    ));
}
