use disksage_lib::zotero_local::{validate_references, ZoteroCreator, ZoteroReference};

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
fn validation_accepts_http_and_absent_source_url() {
    let mut http = reference();
    http.url = Some("http://127.0.0.1/reference".into());
    assert!(validate_references(&[http]).is_ok());

    let mut without_url = reference();
    without_url.url = None;
    assert!(validate_references(&[without_url]).is_ok());
}

#[test]
fn validation_accepts_each_optional_bibliographic_text_field() {
    let mut item = reference();
    item.publication_title = Some("Journal of Storage Safety".into());
    item.publisher = Some("ContextualWisdomLab".into());
    item.volume = Some("12".into());
    item.issue = Some("3".into());
    item.pages = Some("10-22".into());

    assert!(validate_references(&[item]).is_ok());
}

#[test]
fn validation_rejects_an_existing_relative_regular_file() {
    let current_dir = std::env::current_dir().unwrap();
    let root = tempfile::Builder::new()
        .prefix("disksage-zotero-relative-")
        .tempdir_in(&current_dir)
        .unwrap();
    let file = tempfile::NamedTempFile::new_in(root.path()).unwrap();
    let relative_path = file.path().strip_prefix(&current_dir).unwrap().to_path_buf();
    assert!(!relative_path.is_absolute());
    assert!(relative_path.is_file());

    let mut item = reference();
    item.full_text_path = Some(relative_path);
    assert_eq!(
        validate_references(&[item]).unwrap_err(),
        "zotero-full-text-must-be-regular-file"
    );
}

#[test]
fn validation_accepts_exact_public_length_boundaries() {
    let mut item = reference();
    item.item_type = "x".repeat(64);
    item.title = "x".repeat(512);
    item.creators[0].creator_type = "x".repeat(64);

    assert!(validate_references(&[item]).is_ok());
}
