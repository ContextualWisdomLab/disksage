use disksage_lib::zotero_local::{
    validate_references, ZoteroCreator, ZoteroReference, MAX_REFERENCE_COUNT,
};

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

#[test]
fn validation_accepts_exact_collection_creator_url_and_text_limits() {
    assert!(validate_references(&vec![reference(); MAX_REFERENCE_COUNT]).is_ok());

    let mut creators_at_limit = reference();
    creators_at_limit.creators = vec![creators_at_limit.creators[0].clone(); 50];
    assert!(validate_references(&[creators_at_limit]).is_ok());

    let mut url_at_limit = reference();
    url_at_limit.url = Some(format!("https://{}", "x".repeat(4 * 1024 - "https://".len())));
    assert_eq!(url_at_limit.url.as_ref().unwrap().len(), 4 * 1024);
    assert!(validate_references(&[url_at_limit]).is_ok());

    let mut text_at_limit = reference();
    text_at_limit.abstract_note = Some("x".repeat(32 * 1024));
    assert!(validate_references(&[text_at_limit]).is_ok());
}
