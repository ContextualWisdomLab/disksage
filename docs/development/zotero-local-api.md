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

On 2026-08-21, the loopback endpoint answered `GET /api/users/0/items?limit=1` with Zotero 9.0.6,
`Zotero-API-Version: 3`, and `Total-Results: 8312`. A bounded invalid `POST` probe returned
`400 Endpoint does not support method`, confirming that this installation is read-only; no library
item or attachment was changed. The manifest therefore remains a dry-run handoff until Zotero 10+
or another explicitly supported local write route is available.

## APA 7 references

Joint Task Force. (2020). *Security and privacy controls for information systems and organizations*
(NIST Special Publication 800-53, Revision 5). National Institute of Standards and Technology.
https://doi.org/10.6028/NIST.SP.800-53r5

Motik, B., Cuenca Grau, B., Horrocks, I., Wu, Z., Fokoue, A., & Lutz, C. (Eds.). (2012). *OWL 2
Web Ontology Language: Profiles* (2nd ed.). W3C Recommendation. https://www.w3.org/TR/owl2-profiles/

Zotero Documentation. (2026, July 29). *Zotero Local API*. https://www.zotero.org/support/dev/web_api/v3/local_api

Zotero Documentation. (2026, July 29). *Zotero Web API file uploads*.
https://www.zotero.org/support/dev/web_api/v3/file_upload

Lebo, T., Sahoo, S., & McGuinness, D. (Eds.). (2013). *PROV-O: The PROV ontology*.
W3C Recommendation. https://www.w3.org/TR/prov-o/

Albertoni, R., Browning, D., Cox, S. J., Gonzalez Beltran, A., Perego, A., & Winstanley, P.
(Eds.). (2024). *Data Catalog Vocabulary (DCAT) - Version 3*. W3C Recommendation.
https://www.w3.org/TR/vocab-dcat-3/

DCMI Usage Board. (2020). *DCMI Metadata Terms*. DCMI Recommendation.
https://www.dublincore.org/specifications/dublin-core/dcmi-terms/2020-01-20/

Alam, M. M., & Wang, W. (2021). A comprehensive survey on the state-of-the-art data provenance
approaches for security enforcement. *Journal of Computer Security, 29*(4), 423–446.
https://doi.org/10.3233/JCS-200108

Chandramouli, R., & Pinhas, D. (2020). *Security guidelines for storage infrastructure* (NIST SP
800-209). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-209

Buneman, P., Khanna, S., & Tan, W.-C. (2001). Why and where: A characterization of data
provenance. In A. D. Bossi (Ed.), *Database Theory — ICDT 2001* (pp. 316–330). Springer.
https://doi.org/10.1007/3-540-44503-X_20
