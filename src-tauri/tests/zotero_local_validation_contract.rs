use disksage_lib::zotero_local::{
    dry_run_summary, parse_manifest, validate_references, ZoteroCreator, ZoteroReference,
    DEFAULT_LOCAL_API_BASE, MAX_FULL_TEXT_BYTES, MAX_REFERENCE_COUNT,
    MAX_REFERENCE_MANIFEST_BYTES,
};
use std::path::PathBuf;

fn reference() -> ZoteroReference {
    ZoteroReference {
        item_type: "journalArticle".into(),
        title: "Metadata-first storage planning".into(),
        creators: vec![ZoteroCreator {
            creator_type: "author".into(),
            first_name: Some("Ada".into()),
            last_name: Some("Lovelace".into()),
            name: None,
        }],
        date: Some("2026".into()),
        doi: Some("10.0000/example".into()),
        url: Some("https://example.org/paper".into()),
        abstract_note: Some("Bounded evidence.".into()),
        publication_title: None,
        publisher: None,
        volume: None,
        issue: None,
        pages: None,
        extra: Some("source=DiskSage".into()),
        full_text_path: None,
    }
}

#[test]
fn manifest_rejects_size_schema_and_empty_collection_failures() {
    let oversized = vec![b' '; MAX_REFERENCE_MANIFEST_BYTES + 1];
    assert_eq!(
        parse_manifest(&oversized).unwrap_err(),
        "zotero-reference-manifest-too-large"
    );
    assert_eq!(
        parse_manifest(br#"{"#).unwrap_err(),
        "zotero-reference-manifest-invalid"
    );
    assert_eq!(
        parse_manifest(b"[]").unwrap_err(),
        "zotero-reference-manifest-empty"
    );

    let unknown_field = serde_json::json!([{
        "itemType": "journalArticle",
        "title": "Title",
        "creators": [],
        "futureSemantic": true
    }]);
    assert_eq!(
        parse_manifest(&serde_json::to_vec(&unknown_field).unwrap()).unwrap_err(),
        "zotero-reference-manifest-invalid"
    );
}

#[test]
fn validation_enforces_collection_item_title_and_creator_bounds() {
    assert_eq!(
        validate_references(&vec![reference(); MAX_REFERENCE_COUNT + 1]).unwrap_err(),
        "zotero-reference-count-exceeded"
    );

    let mut item = reference();
    item.item_type.clear();
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-item-type-invalid"
    );

    let mut item = reference();
    item.item_type = "x".repeat(65);
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-item-type-invalid"
    );

    let mut item = reference();
    item.title = "   ".into();
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-title-invalid"
    );

    let mut item = reference();
    item.title = "x".repeat(513);
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-title-invalid"
    );

    let mut item = reference();
    item.creators = vec![item.creators[0].clone(); 51];
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-creator-count-exceeded"
    );

    let mut item = reference();
    item.creators[0].creator_type = " ".into();
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-creator-invalid"
    );

    let mut item = reference();
    item.creators[0].first_name = None;
    item.creators[0].last_name = None;
    item.creators[0].name = Some("   ".into());
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-creator-invalid"
    );

    let mut item = reference();
    item.creators[0].first_name = None;
    item.creators[0].last_name = None;
    item.creators[0].name = Some("Ada Lovelace".into());
    assert!(validate_references(&[item]).is_ok());
}

#[test]
fn validation_rejects_unsafe_urls_files_and_text_fields() {
    for url in [
        "file:///private/source.pdf".to_string(),
        "https://example.org/\nheader".to_string(),
        format!("https://example.org/{}", "x".repeat(4 * 1024)),
    ] {
        let mut item = reference();
        item.url = Some(url);
        assert_eq!(
            validate_references(&[item]).unwrap_err(),
            "zotero-source-url-invalid"
        );
    }

    let directory = tempfile::tempdir().unwrap();
    let mut item = reference();
    item.full_text_path = Some(directory.path().to_path_buf());
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-full-text-must-be-regular-file"
    );

    let oversized = tempfile::NamedTempFile::new().unwrap();
    oversized
        .as_file()
        .set_len(MAX_FULL_TEXT_BYTES + 1)
        .unwrap();
    let mut item = reference();
    item.full_text_path = Some(oversized.path().to_path_buf());
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-full-text-too-large"
    );

    let mut item = reference();
    item.abstract_note = Some("x".repeat(32 * 1024 + 1));
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-field-invalid"
    );

    let mut item = reference();
    item.extra = Some("visible\u{0001}control".into());
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-field-invalid"
    );

    let mut item = reference();
    item.full_text_path = Some(PathBuf::from("definitely-missing-relative-zotero-file"));
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-full-text-unavailable"
    );
}

#[test]
fn dry_run_summary_reports_only_bounded_non_mutating_evidence() {
    let first = reference();
    let mut second = reference();
    second.title = "Second reference".into();
    second.url = None;

    let summary = dry_run_summary(&[first, second]);
    assert_eq!(summary["executed"], false);
    assert_eq!(summary["local_api"], DEFAULT_LOCAL_API_BASE);
    assert_eq!(summary["item_count"], 2);
    assert_eq!(summary["titles"][0], "Metadata-first storage planning");
    assert_eq!(summary["titles"][1], "Second reference");
    assert_eq!(summary["original_urls"], serde_json::json!(["https://example.org/paper"]));
    assert!(summary["notice"]
        .as_str()
        .unwrap()
        .contains("pass --execute with ZOTERO_LOCAL_API_KEY to write"));
}
