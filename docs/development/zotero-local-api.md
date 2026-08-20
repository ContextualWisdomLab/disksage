# Zotero Local API metadata handoff

DiskSage keeps Zotero integration local and explicit. The CLI accepts a bounded JSON array of
bibliographic records, validates the source URL and metadata fields in Rust, and defaults to a
read-only preview:

```sh
cargo run --locked --manifest-path src-tauri/Cargo.toml --bin disksage-zotero-local -- \
  --input "$PWD/docs/development/zotero-reference-manifest.json"
```

The preview contains titles and original source URLs but no file contents or credentials. A write
requires a local API key granted by Zotero and an explicit `--execute`; the key is read only from
`ZOTERO_LOCAL_API_KEY` and is never placed in the manifest or command line:

```sh
ZOTERO_LOCAL_API_KEY='provided-by-zotero' cargo run --locked \
  --manifest-path src-tauri/Cargo.toml --bin disksage-zotero-local -- \
  --input "$PWD/docs/development/zotero-reference-manifest.json" --execute
```

This updates bibliographic metadata and the original source URL through `127.0.0.1:23119`.
To attach an original document, add an absolute `fullTextPath` to an individual manifest record.
The field is never sent as metadata; with `--execute`, DiskSage hashes and size-checks the regular
file, then performs Zotero's local three-phase upload (bounded to 4 GiB). No attachment is copied
in preview mode, and an upload failure stops before any source eviction.

The current installed Zotero 9 endpoint reports `Endpoint does not support method` for writes.
DiskSage maps that response to `zotero-local-api-write-unsupported`; it does not upgrade Zotero or
fall back to OAuth. Zotero 10+ is required for local writes and local file uploads.

## APA 7 references

Joint Task Force. (2020). *Security and privacy controls for information systems and organizations*
(NIST Special Publication 800-53, Revision 5). National Institute of Standards and Technology.
https://doi.org/10.6028/NIST.SP.800-53r5

Motik, B., Cuenca Grau, B., Horrocks, I., Wu, Z., Fokoue, A., & Lutz, C. (Eds.). (2012). *OWL 2
Web Ontology Language: Profiles* (2nd ed.). W3C Recommendation. https://www.w3.org/TR/owl2-profiles/

Zotero Documentation. (2026, July 29). *Zotero Local API*. https://www.zotero.org/support/dev/web_api/v3/local_api

Zotero Documentation. (2026, July 29). *Zotero Web API file uploads*.
https://www.zotero.org/support/dev/web_api/v3/file_upload
