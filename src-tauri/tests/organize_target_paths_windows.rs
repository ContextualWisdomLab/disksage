//! Windows-only public-boundary tests for ontology organize destinations.

#[cfg(windows)]
mod windows_tests {
    use disksage_lib::dupes::FileEntry;
    use disksage_lib::ontology::parse_ttl;
    use disksage_lib::organize::plan_moves;
    use std::path::PathBuf;

    fn ontology(target: &str) -> disksage_lib::ontology::Ontology {
        let ttl = format!(
            r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "{target}/{{class}}" .
"#
        );
        parse_ttl(&ttl).expect("Windows ontology fixture must parse")
    }

    #[test]
    fn home_relative_target_uses_native_absolute_path() {
        let home = PathBuf::from(r"C:\Users\u");
        let files = [FileEntry {
            path: PathBuf::from(r"C:\downloads\pic.png"),
            size: 100,
            mtime_ms: 0,
        }];
        let plans = plan_moves(&files, &ontology("~/Media/{class}"), &home);
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].dst, r"C:\Users\u\Media\Image\pic.png");
    }

    #[test]
    fn relative_target_fails_closed() {
        let home = PathBuf::from(r"C:\Users\u");
        let files = [FileEntry {
            path: PathBuf::from(r"C:\downloads\pic.png"),
            size: 100,
            mtime_ms: 0,
        }];
        assert!(plan_moves(&files, &ontology("relative/{class}"), &home).is_empty());
    }
}
