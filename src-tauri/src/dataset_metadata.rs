use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

const MAX_PROFILE_BYTES: u64 = 1024 * 1024;
const MAX_PROFILE_ROWS: u64 = 100;
const MAX_PROFILE_COLUMNS: usize = 128;
const MAX_COLUMN_NAME_CHARS: usize = 128;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetColumnProfile {
    pub name: String,
    pub inferred_type: String,
    pub observed_values: u64,
    pub missing_values: u64,
    pub sensitive_name: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetProfile {
    pub format: String,
    pub sampled_rows: u64,
    pub profile_complete: bool,
    pub sample_truncated: bool,
    pub columns: Vec<DatasetColumnProfile>,
    pub quality_warnings: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ValueKind {
    Unknown,
    Boolean,
    Integer,
    Number,
    Date,
    DateTime,
    Text,
    Json,
    Mixed,
}

impl ValueKind {
    fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Boolean => "boolean",
            Self::Integer => "integer",
            Self::Number => "number",
            Self::Date => "date",
            Self::DateTime => "datetime",
            Self::Text => "text",
            Self::Json => "json",
            Self::Mixed => "mixed",
        }
    }

    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Unknown, value) | (value, Self::Unknown) => value,
            (left, right) if left == right => left,
            (Self::Integer, Self::Number) | (Self::Number, Self::Integer) => Self::Number,
            (Self::Date, Self::DateTime) | (Self::DateTime, Self::Date) => Self::DateTime,
            (Self::Mixed, _) | (_, Self::Mixed) => Self::Mixed,
            _ => Self::Mixed,
        }
    }
}

#[derive(Debug, Clone)]
struct ColumnAccumulator {
    name: String,
    kind: ValueKind,
    observed_values: u64,
    missing_values: u64,
    sensitive_name: bool,
}

impl ColumnAccumulator {
    fn new(name: String, missing_values: u64) -> Self {
        let sensitive_name = sensitive_column_name(&name);
        Self {
            name,
            kind: ValueKind::Unknown,
            observed_values: 0,
            missing_values,
            sensitive_name,
        }
    }

    fn observe_text(&mut self, value: &str) {
        let value = value.trim();
        if value.is_empty() {
            self.missing_values += 1;
            return;
        }
        self.observed_values += 1;
        self.kind = self.kind.merge(classify_text(value));
    }

    fn observe_json(&mut self, value: &serde_json::Value) {
        if value.is_null() {
            self.missing_values += 1;
            return;
        }
        self.observed_values += 1;
        let next = match value {
            serde_json::Value::Null => ValueKind::Unknown,
            serde_json::Value::Bool(_) => ValueKind::Boolean,
            serde_json::Value::Number(number) if number.is_i64() || number.is_u64() => {
                ValueKind::Integer
            }
            serde_json::Value::Number(_) => ValueKind::Number,
            serde_json::Value::String(value) => classify_text(value),
            serde_json::Value::Array(_) | serde_json::Value::Object(_) => ValueKind::Json,
        };
        self.kind = self.kind.merge(next);
    }

    fn finish(self) -> DatasetColumnProfile {
        DatasetColumnProfile {
            name: self.name,
            inferred_type: self.kind.label().into(),
            observed_values: self.observed_values,
            missing_values: self.missing_values,
            sensitive_name: self.sensitive_name,
        }
    }
}

fn bounded_column_name(value: &str, index: usize) -> String {
    let value = value.trim().trim_start_matches('\u{feff}');
    if value.is_empty() {
        format!("column_{}", index + 1)
    } else {
        value.chars().take(MAX_COLUMN_NAME_CHARS).collect()
    }
}

fn sensitive_column_name(value: &str) -> bool {
    let normalized = value.to_lowercase().replace(['-', ' ', '.'], "_");
    [
        "name",
        "email",
        "phone",
        "mobile",
        "address",
        "birth",
        "ssn",
        "resident",
        "passport",
        "patient",
        "employee",
        "customer",
        "password",
        "secret",
        "token",
        "card",
        "account",
        "latitude",
        "longitude",
        "gps",
        "이름",
        "성명",
        "이메일",
        "전화",
        "연락",
        "주소",
        "생년",
        "주민",
        "여권",
        "환자",
        "직원",
        "고객",
        "계좌",
    ]
    .iter()
    .any(|term| normalized.contains(term))
}

fn looks_like_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit())
    {
        return false;
    }
    let month = value[5..7].parse::<u8>().unwrap_or_default();
    let day = value[8..10].parse::<u8>().unwrap_or_default();
    (1..=12).contains(&month) && (1..=31).contains(&day)
}

fn classify_text(value: &str) -> ValueKind {
    let lower = value.trim().to_ascii_lowercase();
    if matches!(lower.as_str(), "true" | "false") {
        ValueKind::Boolean
    } else if value.parse::<i128>().is_ok() {
        ValueKind::Integer
    } else if value.parse::<f64>().is_ok() {
        ValueKind::Number
    } else if value.len() >= 11
        && value.get(..10).is_some_and(looks_like_date)
        && (value.as_bytes()[10] == b'T' || value.as_bytes()[10] == b' ')
    {
        ValueKind::DateTime
    } else if looks_like_date(value) {
        ValueKind::Date
    } else {
        ValueKind::Text
    }
}

fn push_warning(profile: &mut DatasetProfile, warning: &str) {
    if !profile
        .quality_warnings
        .iter()
        .any(|existing| existing == warning)
    {
        profile.quality_warnings.push(warning.into());
    }
}

fn finalize_profile(
    mut profile: DatasetProfile,
    columns: Vec<ColumnAccumulator>,
) -> DatasetProfile {
    profile.columns = columns.into_iter().map(ColumnAccumulator::finish).collect();
    if profile.sampled_rows == 0 {
        profile.profile_complete = false;
        push_warning(&mut profile, "no-data-rows");
    }
    if profile.columns.iter().any(|column| column.sensitive_name) {
        push_warning(&mut profile, "sensitive-column-name-detected");
    }
    profile.quality_warnings.sort();
    profile.quality_warnings.dedup();
    profile
}

fn profile_delimited<R: Read>(reader: R, delimiter: u8, format: &str) -> DatasetProfile {
    let mut profile = DatasetProfile {
        format: format.into(),
        profile_complete: true,
        ..DatasetProfile::default()
    };
    let mut csv_reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(true)
        .flexible(true)
        .from_reader(reader);
    let raw_headers = match csv_reader.headers() {
        Ok(headers) => headers.clone(),
        Err(_) => {
            profile.profile_complete = false;
            push_warning(&mut profile, "header-parse-error");
            return finalize_profile(profile, Vec::new());
        }
    };
    let source_column_count = raw_headers.len();
    if source_column_count == 0 {
        profile.profile_complete = false;
        push_warning(&mut profile, "missing-header");
        return finalize_profile(profile, Vec::new());
    }
    if source_column_count > MAX_PROFILE_COLUMNS {
        profile.profile_complete = false;
        push_warning(&mut profile, "column-limit-exceeded");
    }

    let mut names_seen = BTreeSet::new();
    let mut columns = Vec::new();
    for (index, raw) in raw_headers.iter().take(MAX_PROFILE_COLUMNS).enumerate() {
        let name = bounded_column_name(raw, index);
        if raw.trim().trim_start_matches('\u{feff}').is_empty() {
            profile.profile_complete = false;
            push_warning(&mut profile, "empty-column-name");
        }
        if !names_seen.insert(name.to_lowercase()) {
            profile.profile_complete = false;
            push_warning(&mut profile, "duplicate-column-name");
        }
        columns.push(ColumnAccumulator::new(name, 0));
    }

    for result in csv_reader.records() {
        if profile.sampled_rows >= MAX_PROFILE_ROWS {
            profile.profile_complete = false;
            profile.sample_truncated = true;
            push_warning(&mut profile, "row-sample-limit-reached");
            break;
        }
        let record = match result {
            Ok(record) => record,
            Err(_) => {
                profile.profile_complete = false;
                push_warning(&mut profile, "record-parse-error");
                continue;
            }
        };
        if record.len() != source_column_count {
            profile.profile_complete = false;
            push_warning(&mut profile, "inconsistent-row-width");
        }
        for (index, column) in columns.iter_mut().enumerate() {
            match record.get(index) {
                Some(value) => column.observe_text(value),
                None => column.missing_values += 1,
            }
        }
        profile.sampled_rows += 1;
    }
    finalize_profile(profile, columns)
}

fn profile_json_lines<R: BufRead>(reader: R) -> DatasetProfile {
    let mut profile = DatasetProfile {
        format: "jsonl".into(),
        profile_complete: true,
        ..DatasetProfile::default()
    };
    let mut columns = Vec::<ColumnAccumulator>::new();
    let mut indexes = BTreeMap::<String, usize>::new();

    for line in reader.lines() {
        if profile.sampled_rows >= MAX_PROFILE_ROWS {
            profile.profile_complete = false;
            profile.sample_truncated = true;
            push_warning(&mut profile, "row-sample-limit-reached");
            break;
        }
        let line = match line {
            Ok(line) => line,
            Err(_) => {
                profile.profile_complete = false;
                push_warning(&mut profile, "record-read-error");
                continue;
            }
        };
        if line.trim().is_empty() {
            profile.profile_complete = false;
            push_warning(&mut profile, "blank-jsonl-line");
            continue;
        }
        let value: serde_json::Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                profile.profile_complete = false;
                push_warning(&mut profile, "record-parse-error");
                continue;
            }
        };
        let Some(object) = value.as_object() else {
            profile.profile_complete = false;
            profile.sampled_rows += 1;
            push_warning(&mut profile, "jsonl-record-not-object");
            for column in &mut columns {
                column.missing_values += 1;
            }
            continue;
        };

        for (name, index) in &indexes {
            if !object.contains_key(name) {
                columns[*index].missing_values += 1;
            }
        }
        for (raw_name, value) in object {
            let name = bounded_column_name(raw_name, indexes.len());
            let index = if let Some(index) = indexes.get(&name).copied() {
                index
            } else if columns.len() >= MAX_PROFILE_COLUMNS {
                profile.profile_complete = false;
                push_warning(&mut profile, "column-limit-exceeded");
                continue;
            } else {
                let index = columns.len();
                columns.push(ColumnAccumulator::new(name.clone(), profile.sampled_rows));
                indexes.insert(name, index);
                index
            };
            columns[index].observe_json(value);
        }
        profile.sampled_rows += 1;
    }
    finalize_profile(profile, columns)
}

#[cfg(not(coverage))]
pub fn profile_dataset(path: &Path) -> DatasetProfile {
    let format = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_else(|| "unknown".into());
    if !matches!(format.as_str(), "csv" | "tsv" | "jsonl") {
        return DatasetProfile {
            format,
            profile_complete: false,
            quality_warnings: vec!["unsupported-dataset-format".into()],
            ..DatasetProfile::default()
        };
    }

    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => {
            return DatasetProfile {
                format,
                profile_complete: false,
                quality_warnings: vec!["dataset-open-error".into()],
                ..DatasetProfile::default()
            };
        }
    };
    let byte_limited = file
        .metadata()
        .map(|metadata| metadata.len() > MAX_PROFILE_BYTES)
        .unwrap_or(true);
    let limited = file.take(MAX_PROFILE_BYTES);
    let mut profile = match format.as_str() {
        "csv" => profile_delimited(limited, b',', "csv"),
        "tsv" => profile_delimited(limited, b'\t', "tsv"),
        "jsonl" => profile_json_lines(BufReader::new(limited)),
        _ => unreachable!(),
    };
    if byte_limited {
        profile.profile_complete = false;
        profile.sample_truncated = true;
        push_warning(&mut profile, "byte-sample-limit-reached");
        profile.quality_warnings.sort();
    }
    profile
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::io::Write;

    #[test]
    fn csv_profile_records_schema_without_cell_values() {
        let input = b"\xef\xbb\xbfemail,age,active,note,created_at\nfirst@example.com,42,true,,2026-07-01\nsecond@example.com,3.5,false,hello,2026-07-02T10:00:00Z\n";
        let profile = profile_delimited(Cursor::new(input), b',', "csv");

        assert!(profile.profile_complete);
        assert_eq!(profile.sampled_rows, 2);
        assert_eq!(profile.columns[0].name, "email");
        assert!(profile.columns[0].sensitive_name);
        assert_eq!(profile.columns[1].inferred_type, "number");
        assert_eq!(profile.columns[2].inferred_type, "boolean");
        assert_eq!(profile.columns[3].missing_values, 1);
        assert_eq!(profile.columns[4].inferred_type, "datetime");
        assert!(profile
            .quality_warnings
            .contains(&"sensitive-column-name-detected".to_string()));
        let serialized = serde_json::to_string(&profile).unwrap();
        assert!(!serialized.contains("first@example.com"));
        assert!(!serialized.contains("hello"));
    }

    #[test]
    fn delimited_profile_flags_ambiguous_schema_and_empty_data() {
        let profile = profile_delimited(Cursor::new(b"name,name,\nAlice\n"), b',', "csv");
        assert!(!profile.profile_complete);
        assert_eq!(profile.columns[2].name, "column_3");
        for warning in [
            "duplicate-column-name",
            "empty-column-name",
            "inconsistent-row-width",
        ] {
            assert!(profile.quality_warnings.contains(&warning.to_string()));
        }

        let empty = profile_delimited(Cursor::new(b"a,b\n"), b',', "csv");
        assert!(empty.quality_warnings.contains(&"no-data-rows".to_string()));
    }

    #[test]
    fn jsonl_profile_unions_keys_and_counts_missing_without_values() {
        let input = b"{\"patient_id\":1,\"when\":\"2026-07-01\",\"payload\":{}}\n{\"patient_id\":null,\"when\":\"later\"}\n[]\n\nnot-json\n";
        let profile = profile_json_lines(Cursor::new(input));

        assert!(!profile.profile_complete);
        assert_eq!(profile.sampled_rows, 3);
        let patient = profile
            .columns
            .iter()
            .find(|column| column.name == "patient_id")
            .unwrap();
        assert!(patient.sensitive_name);
        assert_eq!(patient.observed_values, 1);
        assert_eq!(patient.missing_values, 2);
        assert_eq!(
            profile
                .columns
                .iter()
                .find(|column| column.name == "when")
                .unwrap()
                .inferred_type,
            "mixed"
        );
        for warning in [
            "blank-jsonl-line",
            "jsonl-record-not-object",
            "record-parse-error",
        ] {
            assert!(profile.quality_warnings.contains(&warning.to_string()));
        }
    }

    #[test]
    fn row_and_format_limits_fail_closed() {
        let mut input = String::from("id\n");
        for index in 0..=MAX_PROFILE_ROWS {
            input.push_str(&format!("{index}\n"));
        }
        let limited = profile_delimited(Cursor::new(input), b',', "csv");
        assert!(limited.sample_truncated);
        assert_eq!(limited.sampled_rows, MAX_PROFILE_ROWS);

        let tmp = tempfile::tempdir().unwrap();
        let unsupported = tmp.path().join("data.parquet");
        std::fs::File::create(&unsupported).unwrap();
        let profile = profile_dataset(&unsupported);
        assert_eq!(profile.format, "parquet");
        assert!(profile
            .quality_warnings
            .contains(&"unsupported-dataset-format".to_string()));

        let missing = profile_dataset(&tmp.path().join("missing.csv"));
        assert!(missing
            .quality_warnings
            .contains(&"dataset-open-error".to_string()));

        let tsv = tmp.path().join("small.tsv");
        let mut file = std::fs::File::create(&tsv).unwrap();
        file.write_all(b"a\tb\n1\ttrue\n").unwrap();
        let profile = profile_dataset(&tsv);
        assert_eq!(profile.format, "tsv");
        assert!(profile.profile_complete);

        let jsonl = tmp.path().join("small.jsonl");
        let mut file = std::fs::File::create(&jsonl).unwrap();
        file.write_all(b"{\"id\":1}\n").unwrap();
        let profile = profile_dataset(&jsonl);
        assert_eq!(profile.format, "jsonl");
        assert!(profile.profile_complete);

        let oversized = tmp.path().join("oversized.csv");
        let mut file = std::fs::File::create(&oversized).unwrap();
        file.write_all(b"payload\n").unwrap();
        file.write_all(&vec![b'x'; MAX_PROFILE_BYTES as usize + 1])
            .unwrap();
        let profile = profile_dataset(&oversized);
        assert!(profile.sample_truncated);
        assert!(profile
            .quality_warnings
            .contains(&"byte-sample-limit-reached".to_string()));
    }

    #[test]
    fn classifier_and_column_limits_cover_edge_cases() {
        assert_eq!(classify_text("false"), ValueKind::Boolean);
        assert_eq!(classify_text("-4"), ValueKind::Integer);
        assert_eq!(classify_text("1e3"), ValueKind::Number);
        assert_eq!(classify_text("2026-13-01"), ValueKind::Text);
        assert_eq!(classify_text("2026-01-01"), ValueKind::Date);
        assert_eq!(classify_text("2026-01-01 10:00"), ValueKind::DateTime);
        assert_eq!(classify_text("plain"), ValueKind::Text);
        assert_eq!(ValueKind::Json.merge(ValueKind::Text), ValueKind::Mixed);

        let headers = (0..=MAX_PROFILE_COLUMNS)
            .map(|index| format!("c{index}"))
            .collect::<Vec<_>>()
            .join(",");
        let profile = profile_delimited(Cursor::new(format!("{headers}\n")), b',', "csv");
        assert_eq!(profile.columns.len(), MAX_PROFILE_COLUMNS);
        assert!(profile
            .quality_warnings
            .contains(&"column-limit-exceeded".to_string()));
    }
}
