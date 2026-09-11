PRAGMA foreign_keys = ON;

CREATE TABLE translation_resource_versions (
    resource_version TEXT PRIMARY KEY NOT NULL
        CHECK (length(resource_version) BETWEEN 1 AND 128),
    schema_version INTEGER NOT NULL
        CHECK (schema_version > 0),
    created_at_unix_ms INTEGER NOT NULL
        CHECK (created_at_unix_ms >= 0),
    content_sha256 TEXT NOT NULL UNIQUE
        CHECK (
            length(content_sha256) = 64
            AND content_sha256 NOT GLOB '*[^0-9a-f]*'
        )
) STRICT, WITHOUT ROWID;

CREATE TABLE translation_screen_keys (
    screen_key TEXT PRIMARY KEY NOT NULL
        CHECK (
            length(screen_key) BETWEEN 1 AND 160
            AND screen_key NOT GLOB '*[^a-z0-9._-]*'
        ),
    screen_area TEXT NOT NULL
        CHECK (
            length(screen_area) BETWEEN 1 AND 80
            AND screen_area NOT GLOB '*[^a-z0-9._-]*'
        )
) STRICT, WITHOUT ROWID;

CREATE TABLE translation_messages (
    resource_version TEXT NOT NULL,
    locale TEXT NOT NULL
        CHECK (locale IN ('ko', 'en', 'ja', 'zh', 'vi', 'es', 'de', 'fr')),
    screen_key TEXT NOT NULL,
    text_value TEXT NOT NULL
        CHECK (length(text_value) > 0),
    PRIMARY KEY (resource_version, locale, screen_key),
    FOREIGN KEY (resource_version)
        REFERENCES translation_resource_versions(resource_version)
        ON UPDATE RESTRICT ON DELETE RESTRICT,
    FOREIGN KEY (screen_key)
        REFERENCES translation_screen_keys(screen_key)
        ON UPDATE RESTRICT ON DELETE RESTRICT
) STRICT, WITHOUT ROWID;
