# fontdb for WaOS

This fork adds WaOS to fontdb 0.23.0: on a WaOS app (a WASI component, `target_os = "wasi"`), `Database::load_system_fonts` lists the system's fonts through `wasdk::fonts`, where upstream has no backend for that target. The change is `src/waos.rs`, its wiring and `Source`'s `Debug` arm in `src/lib.rs`, and the `wasdk` dependency for that target; everything else is upstream's. It is kept for whoever ports an app to WaOS, and is not offered upstream.

## Using it

```toml
[patch.crates-io]
fontdb = { git = "https://github.com/waos-dev/fontdb", branch = "waos" }
```

The fork asks for the published `wasdk` (0.8.0). An app that takes `wasdk` from WaOS's repository names it in the same `[patch.crates-io]`, so the fork builds with that one too.

## What it does

`Database::load_system_fonts` (behind the `fs` feature, as on other systems) lists every installed family with `wasdk::fonts::families` and each family's faces with `wasdk::fonts::family`, pushing one `FaceInfo` per face without reading any font file. Each face gets its family name, face index, slant, weight, nearest `Stretch` from its CSS width percentage, and `monospaced` from the `isFixedPitch` field of its `post` table. All faces that share a `file_id` share one lazy `Source::Binary` backed by a `OnceLock<Vec<u8>>`, so a font collection is read once. The first time fontdb asks for a face's bytes, it opens the file with `Typeface::open_stream` and waits on the app's thread with `wasdk::block_on` until the whole file is read; a failed or short read logs a warning and yields an empty slice. On WaOS, `Debug` for `Source::Binary` prints `Binary(..)` so formatting a database or face does not trigger a read.

## What it leaves out

`FaceInfo::post_script_name` is left empty, because WaOS names a face by its family and style and the `name` table is not read for it. Localized family names from the `name` table are likewise not read; each face carries the single name WaOS files it under, tagged `Language::English_UnitedStates`. Fonts installed or removed while the app runs (`wasdk::fonts::changes`) are not watched or reloaded automatically. Default generic families (`serif`, `sans-serif`, `cursive`, `fantasy`, `monospace`) stay at `Database::new`'s defaults rather than being mapped from `wasdk::fonts::system_font`. An app running without the fonts capability sees no system fonts.
