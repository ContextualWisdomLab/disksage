//! Real-workbook coverage for bounded spreadsheet metadata profiling.
//!
//! The fixture is a minimal standards-shaped XLSX package created entirely in a temporary
//! directory. It exercises workbook/worksheet admission and heterogeneous cell handling while
//! proving sampled cell values are never retained in the returned profile.

use disksage_lib::profile_dataset;
use std::io::Write;
use zip::write::SimpleFileOptions;

fn add_xml(
    archive: &mut zip::ZipWriter<std::fs::File>,
    path: &str,
    xml: &str,
) {
    archive
        .start_file(path, SimpleFileOptions::default())
        .expect("start XLSX package member");
    archive
        .write_all(xml.as_bytes())
        .expect("write XLSX package member");
}

fn write_minimal_xlsx(path: &std::path::Path) {
    let file = std::fs::File::create(path).expect("create XLSX fixture");
    let mut archive = zip::ZipWriter::new(file);

    add_xml(
        &mut archive,
        "[Content_Types].xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="xml" ContentType="application/xml"/>
  <Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
  <Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
  <Override PartName="/xl/worksheets/sheet2.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#,
    );
    add_xml(
        &mut archive,
        "_rels/.rels",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#,
    );
    add_xml(
        &mut archive,
        "xl/workbook.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">
  <sheets>
    <sheet name="Primary" sheetId="1" r:id="rId1"/>
    <sheet name="Secondary" sheetId="2" r:id="rId2"/>
  </sheets>
</workbook>"#,
    );
    add_xml(
        &mut archive,
        "xl/_rels/workbook.xml.rels",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
  <Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet2.xml"/>
</Relationships>"#,
    );
    add_xml(
        &mut archive,
        "xl/worksheets/sheet1.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1">
      <c r="A1" t="inlineStr"><is><t>email</t></is></c>
      <c r="B1" t="inlineStr"><is><t>status</t></is></c>
      <c r="C1" t="n"><v>7</v></c>
      <c r="D1" t="inlineStr"><is><t>email</t></is></c>
    </row>
    <row r="2">
      <c r="A2" t="inlineStr"><is><t>person@example.com</t></is></c>
      <c r="B2" t="b"><v>1</v></c>
      <c r="C2" t="n"><v>1.25</v></c>
      <c r="D2" t="e"><v>#DIV/0!</v></c>
    </row>
    <row r="3">
      <c r="A3"/>
      <c r="B3" t="inlineStr"><is><t>private-status</t></is></c>
    </row>
  </sheetData>
</worksheet>"#,
    );
    add_xml(
        &mut archive,
        "xl/worksheets/sheet2.xml",
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
  <sheetData>
    <row r="1"><c r="A1" t="inlineStr"><is><t>amount</t></is></c></row>
    <row r="2"><c r="A2" t="n"><v>42</v></c></row>
  </sheetData>
</worksheet>"#,
    );

    archive.finish().expect("finish XLSX fixture");
}

#[test]
fn valid_xlsx_profiles_schema_and_cell_failures_without_retaining_values() {
    let temp = tempfile::tempdir().expect("create XLSX fixture directory");
    let path = temp.path().join("privacy-boundary.XLSX");
    write_minimal_xlsx(&path);

    let profile = profile_dataset(&path);

    assert_eq!(profile.format, "xlsx");
    assert_eq!(profile.sampled_worksheets, 2);
    assert_eq!(profile.worksheet_names, ["Primary", "Secondary"]);
    assert_eq!(profile.sampled_rows, 3);
    assert!(!profile.profile_complete);
    assert!(!profile.sample_truncated);
    assert!(profile
        .quality_warnings
        .contains(&"non-text-or-empty-column-name".to_string()));
    assert!(profile
        .quality_warnings
        .contains(&"duplicate-column-name".to_string()));
    assert!(profile
        .quality_warnings
        .contains(&"cell-error-present".to_string()));
    assert!(profile
        .quality_warnings
        .contains(&"sensitive-column-name-detected".to_string()));

    let email = profile
        .columns
        .iter()
        .find(|column| column.name == "Primary!email")
        .expect("prefixed email column");
    assert!(email.sensitive_name);
    assert_eq!(email.observed_values, 1);
    assert_eq!(email.missing_values, 1);
    assert_eq!(email.inferred_type, "text");

    let numeric_header = profile
        .columns
        .iter()
        .find(|column| column.name == "Primary!column_3")
        .expect("non-text header must receive a bounded synthetic name");
    assert_eq!(numeric_header.inferred_type, "number");

    let amount = profile
        .columns
        .iter()
        .find(|column| column.name == "Secondary!amount")
        .expect("second worksheet column must be namespaced");
    assert_eq!(amount.inferred_type, "number");

    let serialized = serde_json::to_string(&profile).expect("serialize bounded profile");
    assert!(!serialized.contains("person@example.com"));
    assert!(!serialized.contains("private-status"));
    assert!(!serialized.contains("#DIV/0!"));
}
