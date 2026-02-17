use super::maps::{
    ALKANO_EMERALD_FILE_MAP, ANTIDEN_FILE_MAP, ARCHIVE_FILE_MAP, BIBATA_FILE_MAP,
    BREEZE_NORD_FILE_MAP, BREEZE_PLASMA_FILE_MAP, COMIX_FILE_MAP, CONCEPT1_FILE_MAP, NERO_FILE_MAP,
    NO_FILE_MAP, POSY_BLACK_FILE_MAP, POSY_FILE_MAP, TAHOE_SHADOW_FILE_MAP, VISION_FILE_MAP,
};
use super::{CollectionSource, CursorCollectionSpec};

const MATERIAL_DEFAULT_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/SullensCR/Windows-Material-Design-Cursor-V2-Dark-Hdpi-by-jepriCreations/contents/cursor/default?ref=main",
];
const MATERIAL_PURE_BLACK_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/SullensCR/Windows-Material-Design-Cursor-V2-Dark-Hdpi-by-jepriCreations/contents/cursor/pure%20black?ref=main",
];
const TAIL_LIGHT_CANDY_API_URLS: [&str; 2] = [
    "https://api.github.com/repos/SullensCR/Windows-11-Hdpi-Tail-Cursor-Concept-by-jepriCreations/contents/light/base?ref=main",
    "https://api.github.com/repos/SullensCR/Windows-11-Hdpi-Tail-Cursor-Concept-by-jepriCreations/contents/light/09.%20candy?ref=main",
];
const TAIL_DARK_CANDY_API_URLS: [&str; 2] = [
    "https://api.github.com/repos/SullensCR/Windows-11-Hdpi-Tail-Cursor-Concept-by-jepriCreations/contents/dark/base?ref=main",
    "https://api.github.com/repos/SullensCR/Windows-11-Hdpi-Tail-Cursor-Concept-by-jepriCreations/contents/dark/09.%20candy?ref=main",
];
const CONCEPT1_DARK_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/SullensCR/Cursor-Concept-1-HDPI-By-jepriCreations/contents/dark?ref=main",
];
const CONCEPT1_LIGHT_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/SullensCR/Cursor-Concept-1-HDPI-By-jepriCreations/contents/light?ref=main",
];
const ARCHIVE_W11_CONCEPT_DARK_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/udaykamboj/jepriCreations-Unofficial-Archive-Repo/contents/windows_11_cursors_concept/dark?ref=main",
];
const ARCHIVE_W11_CONCEPT_LIGHT_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/udaykamboj/jepriCreations-Unofficial-Archive-Repo/contents/windows_11_cursors_concept/light?ref=main",
];
const ARCHIVE_W11_TAIL_DARK_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/udaykamboj/jepriCreations-Unofficial-Archive-Repo/contents/windows_11_tail_cursor_concept/dark?ref=main",
];
const ARCHIVE_W11_TAIL_LIGHT_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/udaykamboj/jepriCreations-Unofficial-Archive-Repo/contents/windows_11_tail_cursor_concept/light?ref=main",
];
const ANTIDEN_SIERRA_SHADOW_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/antiden/macOS-cursors-for-Windows/contents/1.%20Sierra%20and%20newer/2.%20With%20Shadow/1.%20Normal?ref=main",
];
const BREEZE_PLASMA6_API_URLS: [&str; 1] = [
    "https://api.github.com/repos/black7375/Breeze-Cursors-for-Windows/contents/plasma6/output?ref=master",
];

const TAHOE_ZIP_URL: &str =
    "https://drive.google.com/uc?export=download&id=1mO8rxRonMSn77sLypWypvmzV2eU6KfWP";
const VISION_BLACK_ZIP_URL: &str =
    "https://github.com/zDyant/Vision-Cursor/releases/download/v1.0/Vision-Black-Windows.zip";
const VISION_WHITE_ZIP_URL: &str =
    "https://github.com/zDyant/Vision-Cursor/releases/download/v1.0/Vision-White-Windows.zip";
const BIBATA_MODERN_AMBER_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Amber-Windows.zip";
const BIBATA_MODERN_AMBER_RIGHT_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Amber-Right-Windows.zip";
const BIBATA_MODERN_CLASSIC_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Classic-Windows.zip";
const BIBATA_MODERN_CLASSIC_RIGHT_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Classic-Right-Windows.zip";
const BIBATA_MODERN_ICE_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Ice-Windows.zip";
const BIBATA_MODERN_ICE_RIGHT_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Ice-Right-Windows.zip";
const BIBATA_ORIGINAL_AMBER_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Amber-Windows.zip";
const BIBATA_ORIGINAL_AMBER_RIGHT_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Amber-Right-Windows.zip";
const BIBATA_ORIGINAL_CLASSIC_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Classic-Windows.zip";
const BIBATA_ORIGINAL_CLASSIC_RIGHT_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Classic-Right-Windows.zip";
const BIBATA_ORIGINAL_ICE_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Ice-Windows.zip";
const BIBATA_ORIGINAL_ICE_RIGHT_ZIP_URL: &str =
    "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Ice-Right-Windows.zip";
const POSY_DEFAULT_ZIP_URL: &str = "https://www.michieldb.nl/other/cursors/Posy%27s%20Cursor.zip";
const POSY_BLACK_ZIP_URL: &str =
    "https://www.michieldb.nl/other/cursors/Posy%27s%20Cursor%20Black.zip";
const RW_NERO_ZIP_URL: &str = "https://www.rw-designer.com/cursor-downloadset/nero.zip";
const RW_BREEZE_NORD_ZIP_URL: &str =
    "https://www.rw-designer.com/cursor-downloadset/breeze-nord-nordik.zip";
const RW_ALKANO_ZIP_URL: &str = "https://www.rw-designer.com/cursor-downloadset/alkano.zip";
const COMIX_RAR_URL: &str =
    "https://dl.skinpacks.com/post/Cursor/Windows_Cursors/Comix_New_Cursor_Pack.rar";

pub(super) const COLLECTIONS: [CursorCollectionSpec; 35] = [
    CursorCollectionSpec {
        id: "material-dark-default",
        title: "Material Design Dark v2 (Default)",
        scheme_name: "Material Design Dark v2 by Jepri Creations",
        source: CollectionSource::GithubApi(&MATERIAL_DEFAULT_API_URLS),
        file_name_map: &NO_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "material-dark-pure-black",
        title: "Material Design Dark v2 (Pure Black)",
        scheme_name: "Material Design Pure Dark v2 by Jepri Creations",
        source: CollectionSource::GithubApi(&MATERIAL_PURE_BLACK_API_URLS),
        file_name_map: &NO_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-tail-light-candy",
        title: "W11 Tail HDPI Light (Candy)",
        scheme_name: "W11 Tail HDPI Light Candy by Jepri Creations",
        source: CollectionSource::GithubApi(&TAIL_LIGHT_CANDY_API_URLS),
        file_name_map: &NO_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-tail-dark-candy",
        title: "W11 Tail HDPI Dark (Candy)",
        scheme_name: "W11 Tail HDPI Dark Candy by Jepri Creations",
        source: CollectionSource::GithubApi(&TAIL_DARK_CANDY_API_URLS),
        file_name_map: &NO_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "cursor-concept-1-dark",
        title: "Cursor Concept 1 HDPI (Dark)",
        scheme_name: "Cursor Concept 1 HDPI Dark by Jepri Creations",
        source: CollectionSource::GithubApi(&CONCEPT1_DARK_API_URLS),
        file_name_map: &CONCEPT1_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "cursor-concept-1-light",
        title: "Cursor Concept 1 HDPI (Light)",
        scheme_name: "Cursor Concept 1 HDPI Light by Jepri Creations",
        source: CollectionSource::GithubApi(&CONCEPT1_LIGHT_API_URLS),
        file_name_map: &CONCEPT1_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-concept-dark-archive",
        title: "W11 Cursors Concept (Dark, Archive)",
        scheme_name: "W11 Cursor Concept Dark Free by Jepri Creations",
        source: CollectionSource::GithubApi(&ARCHIVE_W11_CONCEPT_DARK_API_URLS),
        file_name_map: &ARCHIVE_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-concept-light-archive",
        title: "W11 Cursors Concept (Light, Archive)",
        scheme_name: "W11 Cursor Concept Light Free by Jepri Creations",
        source: CollectionSource::GithubApi(&ARCHIVE_W11_CONCEPT_LIGHT_API_URLS),
        file_name_map: &ARCHIVE_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-tail-dark-archive",
        title: "W11 Tail Cursor Concept (Dark, Archive)",
        scheme_name: "W11 Tail Cursor Concept Dark Free by Jepri Creations",
        source: CollectionSource::GithubApi(&ARCHIVE_W11_TAIL_DARK_API_URLS),
        file_name_map: &ARCHIVE_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-tail-light-archive",
        title: "W11 Tail Cursor Concept (Light, Archive)",
        scheme_name: "W11 Tail Cursor Concept Light Free by Jepri Creations",
        source: CollectionSource::GithubApi(&ARCHIVE_W11_TAIL_LIGHT_API_URLS),
        file_name_map: &ARCHIVE_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "antiden-sierra-shadow",
        title: "macOS Sierra+ (With Shadow)",
        scheme_name: "macOS Cursors With Shadow Newer",
        source: CollectionSource::GithubApi(&ANTIDEN_SIERRA_SHADOW_API_URLS),
        file_name_map: &ANTIDEN_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "breeze-plasma6",
        title: "KDE Breeze 6.1",
        scheme_name: "KDE Breeze 6.1",
        source: CollectionSource::GithubApi(&BREEZE_PLASMA6_API_URLS),
        file_name_map: &BREEZE_PLASMA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "tahoe-tail-shadow",
        title: "MacOS Tahoe Tail (With Shadow)",
        scheme_name: "MacOS Tahoe Cursor (With Shadow)",
        source: CollectionSource::ZipArchive {
            url: TAHOE_ZIP_URL,
            subdir: "MacOS Tahoe Cursors/Tail/With Shadow",
        },
        file_name_map: &TAHOE_SHADOW_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "tahoe-tailless-shadow",
        title: "MacOS Tahoe Tailless (With Shadow)",
        scheme_name: "MacOS Tahoe Tailless Cursor (With Shadow)",
        source: CollectionSource::ZipArchive {
            url: TAHOE_ZIP_URL,
            subdir: "MacOS Tahoe Cursors/Tailless/With Shadow",
        },
        file_name_map: &TAHOE_SHADOW_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "vision-black",
        title: "Vision Cursor Black",
        scheme_name: "Vision Cursor Black",
        source: CollectionSource::ZipArchive {
            url: VISION_BLACK_ZIP_URL,
            subdir: "Vision-Black",
        },
        file_name_map: &VISION_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "vision-white",
        title: "Vision Cursor White",
        scheme_name: "Vision Cursor White",
        source: CollectionSource::ZipArchive {
            url: VISION_WHITE_ZIP_URL,
            subdir: "Vision-White",
        },
        file_name_map: &VISION_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-modern-amber-regular",
        title: "Bibata Modern Amber (Regular)",
        scheme_name: "Bibata-Modern-Amber-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_MODERN_AMBER_ZIP_URL,
            subdir: "Bibata-Modern-Amber-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-modern-amber-right-regular",
        title: "Bibata Modern Amber Right (Regular)",
        scheme_name: "Bibata-Modern-Amber-Right-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_MODERN_AMBER_RIGHT_ZIP_URL,
            subdir: "Bibata-Modern-Amber-Right-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-modern-classic-regular",
        title: "Bibata Modern Classic (Regular)",
        scheme_name: "Bibata-Modern-Classic-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_MODERN_CLASSIC_ZIP_URL,
            subdir: "Bibata-Modern-Classic-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-modern-classic-right-regular",
        title: "Bibata Modern Classic Right (Regular)",
        scheme_name: "Bibata-Modern-Classic-Right-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_MODERN_CLASSIC_RIGHT_ZIP_URL,
            subdir: "Bibata-Modern-Classic-Right-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-modern-ice-regular",
        title: "Bibata Modern Ice (Regular)",
        scheme_name: "Bibata-Modern-Ice-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_MODERN_ICE_ZIP_URL,
            subdir: "Bibata-Modern-Ice-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-modern-ice-right-regular",
        title: "Bibata Modern Ice Right (Regular)",
        scheme_name: "Bibata-Modern-Ice-Right-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_MODERN_ICE_RIGHT_ZIP_URL,
            subdir: "Bibata-Modern-Ice-Right-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-original-amber-regular",
        title: "Bibata Original Amber (Regular)",
        scheme_name: "Bibata-Original-Amber-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_ORIGINAL_AMBER_ZIP_URL,
            subdir: "Bibata-Original-Amber-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-original-amber-right-regular",
        title: "Bibata Original Amber Right (Regular)",
        scheme_name: "Bibata-Original-Amber-Right-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_ORIGINAL_AMBER_RIGHT_ZIP_URL,
            subdir: "Bibata-Original-Amber-Right-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-original-classic-regular",
        title: "Bibata Original Classic (Regular)",
        scheme_name: "Bibata-Original-Classic-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_ORIGINAL_CLASSIC_ZIP_URL,
            subdir: "Bibata-Original-Classic-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-original-classic-right-regular",
        title: "Bibata Original Classic Right (Regular)",
        scheme_name: "Bibata-Original-Classic-Right-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_ORIGINAL_CLASSIC_RIGHT_ZIP_URL,
            subdir: "Bibata-Original-Classic-Right-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-original-ice-regular",
        title: "Bibata Original Ice (Regular)",
        scheme_name: "Bibata-Original-Ice-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_ORIGINAL_ICE_ZIP_URL,
            subdir: "Bibata-Original-Ice-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "bibata-original-ice-right-regular",
        title: "Bibata Original Ice Right (Regular)",
        scheme_name: "Bibata-Original-Ice-Right-Regular Cursors",
        source: CollectionSource::ZipArchive {
            url: BIBATA_ORIGINAL_ICE_RIGHT_ZIP_URL,
            subdir: "Bibata-Original-Ice-Right-Regular-Windows",
        },
        file_name_map: &BIBATA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "posy-default",
        title: "Posy's Cursor",
        scheme_name: "Posy's Cursor",
        source: CollectionSource::ZipArchive {
            url: POSY_DEFAULT_ZIP_URL,
            subdir: "Posy's Cursor",
        },
        file_name_map: &POSY_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "posy-black",
        title: "Posy's Cursor Black",
        scheme_name: "Posy's Cursor Black",
        source: CollectionSource::ZipArchive {
            url: POSY_BLACK_ZIP_URL,
            subdir: "Posy's Cursor Black",
        },
        file_name_map: &POSY_BLACK_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "rw-nero",
        title: "Nero",
        scheme_name: "Nero Cursor",
        source: CollectionSource::ZipArchive {
            url: RW_NERO_ZIP_URL,
            subdir: "",
        },
        file_name_map: &NERO_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "rw-breeze-nord-nordik",
        title: "Breeze Nord Nordik",
        scheme_name: "Breeze Nord Nordik",
        source: CollectionSource::ZipArchive {
            url: RW_BREEZE_NORD_ZIP_URL,
            subdir: "",
        },
        file_name_map: &BREEZE_NORD_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "rw-alkano-emerald",
        title: "Alkano (Emerald)",
        scheme_name: "Alkano Emerald",
        source: CollectionSource::ZipArchive {
            url: RW_ALKANO_ZIP_URL,
            subdir: "",
        },
        file_name_map: &ALKANO_EMERALD_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "comix-new-black",
        title: "Comix New (Black)",
        scheme_name: "Comix New Black",
        source: CollectionSource::RarArchive {
            url: COMIX_RAR_URL,
            subdir: "Comix New Cursors/Comix New Black",
        },
        file_name_map: &COMIX_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "comix-new-white",
        title: "Comix New (White)",
        scheme_name: "Comix New White",
        source: CollectionSource::RarArchive {
            url: COMIX_RAR_URL,
            subdir: "Comix New Cursors/Comix New White",
        },
        file_name_map: &COMIX_FILE_MAP,
    },
];
