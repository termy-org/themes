# Termy Themes

Community theme registry for Termy.

## Layout

```text
themes/
  <slug>/
    metadata.json
    files/
      <version>.json
index.json
schemas/
  theme.schema.json
  theme-metadata.schema.json
  theme-index.schema.json
```

## Export a Theme

From a Termy checkout:

```sh
cargo run -p termy_cli -- -export-theme \
  --repo /path/to/themes \
  --slug my-theme \
  --name "My Theme" \
  --version 1.0.0 \
  --description "A short description."
```

Validate before opening a pull request:

```sh
cargo run -p termy_cli -- -validate-theme-repo --repo /path/to/themes
```

Theme versions use semver like `1.0.0`. Existing version files are immutable unless
you intentionally re-export with `--force` before opening the PR.
