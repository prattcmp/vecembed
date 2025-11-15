# VecEmbed

This tool imports, stores, and retrieves search results for vector embeddings related to a SQL database.

### Adding support for a table

Every table is **_not_** automatically detected and supported. Follow these steps to add VecEmbed support for a table:

1. Go to `proto/vecembed.proto` and add the table name to the `EmbeddableModel` enum in ALL CAPS snake case (e.g. **TABLE_NAME**).
2. Go to `src/embed/collections.rs` and add the `embeddable_entity` macro for the table name, supplying the relevant `id` and `text/body` columns. For example:
```rust
embeddable_entity!(
    table_name::Entity,
    table_name::Column,
    table_name::Column::Id,
    table_name::Column::Id,
    table_name::Column::Text
);
```
3. Go to `src/entities/string_convert.rs` and add the relevant `match` item to enable command line imports for that table. For example:
```rust
"uploaded_files" => {
    crate::embed::import::import_embeddings::<
        super::uploaded_files::Entity,
        super::uploaded_files::Column,
    >()
    .await
},
```

That's it! Now VecEmbed supports this table.

### Importing a table's contents

Once you've added support for a table, importing its contents is easy. 

##### Local development environment

If you have a local Rust development environment with the entire source, just run:

```sh
cargo run -- --import=table_name
```

##### Binary executable

If you have a compiled binary/executable of VecEmbed, make sure you have permissions to run that executable, then execute it with the import argument. On Unix based systems, run:

```sh
./executable_name --import=table_name
```

If you don't have permissions to run the executable on a Unix based system, run the following before trying the import command:

```sh
chmod +x executable_name
```

© 2024. All rights reserved. Silatus, Inc.