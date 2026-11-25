//! The system's fonts on WaOS, listed through `wasdk::fonts` and read on first use.
//!
//! `load_system_fonts` asks WaOS for every installed family and its faces and records their
//! metadata without opening any font file. Each face's `Source::Binary` holds a `FontFile`
//! shared by every face with the same `file_id`, so a font collection is read once, the first
//! time fontdb asks for its bytes.
//!
//! Invariants: nothing is read while listing, and the first read waits synchronously on the
//! app's thread with `wasdk::block_on` (so no `wasdk::spawn` is used here).

use std::collections::HashMap;
use std::convert::TryFrom;
use std::sync::{Arc, OnceLock};

use wasdk::fonts::{self, Slant, Typeface};
use wasdk::StreamResult;

use crate::{Database, FaceInfo, Language, Source, Stretch, Style, Weight, ID};

const POST_TAG: u32 = u32::from_be_bytes(*b"post");

pub(crate) fn load_system_fonts(db: &mut Database) {
    let mut files: HashMap<u64, Arc<dyn AsRef<[u8]> + Sync + Send>> = HashMap::new();

    for family in fonts::families() {
        for typeface in fonts::family(&family) {
            db.push_face_info(face_info(typeface, &mut files));
        }
    }
}

fn face_info(
    typeface: Typeface,
    files: &mut HashMap<u64, Arc<dyn AsRef<[u8]> + Sync + Send>>,
) -> FaceInfo {
    let index = typeface.index();
    let style = typeface.style();
    let family = typeface.family();
    let monospaced = is_monospaced(&typeface);
    let file_id = typeface.file_id();

    let file = files
        .entry(file_id)
        .or_insert_with(|| Arc::new(FontFile::new(typeface)))
        .clone();

    FaceInfo {
        id: ID::dummy(),
        source: Source::Binary(file),
        index,
        families: vec![(family, Language::English_UnitedStates)],
        // WaOS names a face by its family and style, and the name table is not read for it.
        post_script_name: String::new(),
        style: style_from_slant(style.slant),
        weight: Weight(style.weight),
        stretch: stretch_from_width(style.width),
        monospaced,
    }
}

struct FontFile {
    typeface: Typeface,
    bytes: OnceLock<Vec<u8>>,
}

impl FontFile {
    fn new(typeface: Typeface) -> Self {
        Self {
            typeface,
            bytes: OnceLock::new(),
        }
    }
}

impl AsRef<[u8]> for FontFile {
    fn as_ref(&self) -> &[u8] {
        self.bytes.get_or_init(|| read_font_file(&self.typeface))
    }
}

fn read_font_file(typeface: &Typeface) -> Vec<u8> {
    let fonts::FontFile {
        mut data, length, ..
    } = typeface.open_stream();

    let length = match usize::try_from(length) {
        Ok(length) => length,
        Err(_) => {
            log::warn!(
                "Failed to read WaOS font '{}' because its length ({}) exceeds addressable memory.",
                typeface.family(),
                length
            );
            return Vec::new();
        }
    };

    let mut bytes = Vec::new();
    if let Err(err) = bytes.try_reserve_exact(length) {
        log::warn!(
            "Failed to reserve {} bytes for WaOS font '{}': {}.",
            length,
            typeface.family(),
            err
        );
        return Vec::new();
    }

    wasdk::block_on(async move {
        while bytes.len() < length {
            let (result, read) = data.read(bytes).await;
            bytes = read;
            match result {
                StreamResult::Complete(_) => {}
                StreamResult::Dropped | StreamResult::Cancelled => {
                    log::warn!(
                        "Short read for WaOS font '{}': got {} of {} bytes.",
                        typeface.family(),
                        bytes.len(),
                        length
                    );
                    return Vec::new();
                }
            }
        }
        if data.next().await.is_some() {
            log::warn!(
                "WaOS font '{}' stream ran past its declared {} bytes.",
                typeface.family(),
                length
            );
            return Vec::new();
        }
        bytes
    })
}

fn is_monospaced(typeface: &Typeface) -> bool {
    let post = match typeface.table(POST_TAG) {
        Some(post) => post,
        None => return false,
    };
    let bytes = match post.get(12..16) {
        Some(bytes) => bytes,
        None => return false,
    };
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) != 0
}

fn style_from_slant(slant: Slant) -> Style {
    match slant {
        Slant::Upright => Style::Normal,
        Slant::Italic => Style::Italic,
        Slant::Oblique => Style::Oblique,
    }
}

fn stretch_from_width(width: f32) -> Stretch {
    if !width.is_finite() {
        return Stretch::Normal;
    }
    const STOPS: [(f32, Stretch); 9] = [
        (50.0, Stretch::UltraCondensed),
        (62.5, Stretch::ExtraCondensed),
        (75.0, Stretch::Condensed),
        (87.5, Stretch::SemiCondensed),
        (100.0, Stretch::Normal),
        (112.5, Stretch::SemiExpanded),
        (125.0, Stretch::Expanded),
        (150.0, Stretch::ExtraExpanded),
        (200.0, Stretch::UltraExpanded),
    ];
    STOPS
        .iter()
        .copied()
        .min_by(|(a, _), (b, _)| (width - a).abs().total_cmp(&(width - b).abs()))
        .map(|(_, stretch)| stretch)
        .unwrap_or(Stretch::Normal)
}
