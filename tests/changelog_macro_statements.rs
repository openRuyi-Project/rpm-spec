//! Integration coverage for standalone macro statements in `%changelog`.

use rpm_spec::ast::{ChangelogItem, ConditionalMacro, MacroKind, Section, SpecFile, SpecItem};
use rpm_spec::parse_result::codes;
use rpm_spec::parser::{parse_str, parse_str_with_spans};
use rpm_spec::printer::print;

fn changelog_items<T>(spec: &SpecFile<T>) -> &[ChangelogItem<T>] {
    spec.items
        .iter()
        .find_map(|item| match item {
            SpecItem::Section(section) => match section.as_ref() {
                Section::Changelog { items, .. } => Some(items.as_slice()),
                _ => None,
            },
            _ => None,
        })
        .expect("changelog section")
}

#[test]
fn canonical_autochangelog_has_macro_shape_and_source_span() {
    let source = "%changelog\n%autochangelog\n";
    let parsed = parse_str_with_spans(source);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let [ChangelogItem::Statement { macro_ref, data }] = changelog_items(&parsed.spec) else {
        panic!("expected one changelog macro statement")
    };
    assert_eq!(macro_ref.kind, MacroKind::Plain);
    assert_eq!(macro_ref.name, "autochangelog");
    assert_eq!(macro_ref.conditional, ConditionalMacro::None);
    assert_eq!(&source[data.start_byte..data.end_byte], "%autochangelog\n");
}

#[test]
fn other_macro_names_use_the_same_statement_model() {
    let source = "%changelog\n%project_history\n%{?vendor_history}\n";
    let parsed = parse_str(source);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let [
        ChangelogItem::Statement {
            macro_ref: plain, ..
        },
        ChangelogItem::Statement {
            macro_ref: conditional,
            ..
        },
    ] = changelog_items(&parsed.spec)
    else {
        panic!("expected two changelog macro statements")
    };
    assert_eq!(plain.kind, MacroKind::Plain);
    assert_eq!(plain.name, "project_history");
    assert_eq!(plain.conditional, ConditionalMacro::None);
    assert_eq!(conditional.kind, MacroKind::Braced);
    assert_eq!(conditional.name, "vendor_history");
    assert_eq!(conditional.conditional, ConditionalMacro::IfDefined);
    assert_eq!(print(&parsed.spec), source);
}

#[test]
fn conditional_autochangelog_keeps_order_and_roundtrips() {
    let source = "\
%changelog
* Mon Jan 01 2024 A - 1-1
- first
%{?autochangelog}
* Sun Dec 31 2023 B - 0-1
- older
";
    let parsed = parse_str(source);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let [
        ChangelogItem::Entry(_),
        ChangelogItem::Statement { macro_ref, .. },
        ChangelogItem::Entry(_),
    ] = changelog_items(&parsed.spec)
    else {
        panic!("expected a macro statement between two dated entries")
    };
    assert_eq!(macro_ref.kind, MacroKind::Braced);
    assert_eq!(macro_ref.name, "autochangelog");
    assert_eq!(macro_ref.conditional, ConditionalMacro::IfDefined);

    let printed = print(&parsed.spec);
    assert_eq!(printed, source);
    let reparsed = parse_str(&printed);
    assert!(
        reparsed.diagnostics.is_empty(),
        "{:?}",
        reparsed.diagnostics
    );
    assert_eq!(reparsed.spec, parsed.spec);
}

#[test]
fn trailing_text_stays_an_unexpected_changelog_line() {
    let source = "%changelog\n%autochangelog trailing\n";
    let parsed = parse_str(source);

    assert!(changelog_items(&parsed.spec).is_empty());
    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(
        parsed.diagnostics[0].code.as_deref(),
        Some(codes::W_UNEXPECTED_LINE_IN_CHANGELOG)
    );
}

#[test]
fn unterminated_macro_does_not_consume_the_next_entry() {
    let source = "\
%changelog
%{?foo:
* Fri Sep 04 2026 A <a@example.org> - 1-1
- valid entry
";
    let parsed = parse_str_with_spans(source);

    let [
        ChangelogItem::Statement { data, .. },
        ChangelogItem::Entry(entry),
    ] = changelog_items(&parsed.spec)
    else {
        panic!("expected a recovered macro statement followed by a dated entry")
    };
    assert_eq!(&source[data.start_byte..data.end_byte], "%{?foo:\n");
    assert_eq!(entry.author.literal_str(), Some("A"));

    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(
        parsed.diagnostics[0].code.as_deref(),
        Some(codes::W_UNTERMINATED_MACRO)
    );
}

#[test]
fn top_level_multiline_macro_remains_supported() {
    let source = "\
%{lua:
print('hello')
}
Name: demo
";
    let parsed = parse_str(source);

    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let [SpecItem::Statement(macro_ref), SpecItem::Preamble(_)] = parsed.spec.items.as_slice()
    else {
        panic!("expected a top-level macro statement followed by a preamble")
    };
    assert_eq!(macro_ref.kind, MacroKind::Lua);
}
