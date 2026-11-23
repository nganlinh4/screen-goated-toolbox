use super::maps::{
    ALKANO_EMERALD_FILE_MAP, ANTIDEN_FILE_MAP, ARCHIVE_FILE_MAP, BIBATA_FILE_MAP,
    BREEZE_NORD_FILE_MAP, BREEZE_PLASMA_FILE_MAP, COMIX_FILE_MAP, CONCEPT1_FILE_MAP, NERO_FILE_MAP,
    NO_FILE_MAP, POSY_BLACK_FILE_MAP, POSY_FILE_MAP, TAHOE_SHADOW_FILE_MAP, VISION_FILE_MAP,
};
use super::{CollectionSource, CursorCollectionSpec, PinnedBytes, github_pins};

const TAHOE_ZIP_URL: &str =
    "https://drive.google.com/uc?export=download&id=1mO8rxRonMSn77sLypWypvmzV2eU6KfWP";
const VISION_BLACK_ZIP_URL: &str =
    "https://github.com/zDyant/Vision-Cursor/releases/download/v1.0/Vision-Black-Windows.zip";
const VISION_WHITE_ZIP_URL: &str =
    "https://github.com/zDyant/Vision-Cursor/releases/download/v1.0/Vision-White-Windows.zip";
const BIBATA_MODERN_AMBER_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Amber-Windows.zip";
const BIBATA_MODERN_AMBER_RIGHT_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Amber-Right-Windows.zip";
const BIBATA_MODERN_CLASSIC_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Classic-Windows.zip";
const BIBATA_MODERN_CLASSIC_RIGHT_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Classic-Right-Windows.zip";
const BIBATA_MODERN_ICE_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Ice-Windows.zip";
const BIBATA_MODERN_ICE_RIGHT_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Modern-Ice-Right-Windows.zip";
const BIBATA_ORIGINAL_AMBER_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Amber-Windows.zip";
const BIBATA_ORIGINAL_AMBER_RIGHT_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Amber-Right-Windows.zip";
const BIBATA_ORIGINAL_CLASSIC_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Classic-Windows.zip";
const BIBATA_ORIGINAL_CLASSIC_RIGHT_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Classic-Right-Windows.zip";
const BIBATA_ORIGINAL_ICE_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Ice-Windows.zip";
const BIBATA_ORIGINAL_ICE_RIGHT_ZIP_URL: &str = "https://github.com/ful1e5/Bibata_Cursor/releases/download/v2.0.7/Bibata-Original-Ice-Right-Windows.zip";
const POSY_DEFAULT_ZIP_URL: &str = "https://www.michieldb.nl/other/cursors/Posy%27s%20Cursor.zip";
const POSY_BLACK_ZIP_URL: &str =
    "https://www.michieldb.nl/other/cursors/Posy%27s%20Cursor%20Black.zip";
const RW_NERO_ZIP_URL: &str = "https://www.rw-designer.com/cursor-downloadset/nero.zip";
const RW_BREEZE_NORD_ZIP_URL: &str =
    "https://www.rw-designer.com/cursor-downloadset/breeze-nord-nordik.zip";
const RW_ALKANO_ZIP_URL: &str = "https://www.rw-designer.com/cursor-downloadset/alkano.zip";
const COMIX_RAR_URL: &str =
    "https://dl.skinpacks.com/post/Cursor/Windows_Cursors/Comix_New_Cursor_Pack.rar";

const TAHOE_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 1777653,
    sha256: "2c1cee187609b74f7549ba8d4b00252c92f94b79deb00f2afe13bdd0d56c3d40",
};
const VISION_BLACK_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 125819,
    sha256: "d308b37ac55b0a3e60b8fd95e00cfa5a7de3e001ed6ab9470ec9544fe5ed58cc",
};
const VISION_WHITE_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 124535,
    sha256: "35a7cb0c687fa96d2049e7b94749aa1c1671f737ebbefcf19214c6ee23c681f2",
};
const BIBATA_MODERN_AMBER_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 11979435,
    sha256: "5151e0f8648d00fb734fea501def12b5cc1fbc65ba12a6e28db2e1295ebf8961",
};
const BIBATA_MODERN_AMBER_RIGHT_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 11993526,
    sha256: "6a68cfefaa082445dcc2cb647ee4f4d17d8f4c858fc3d5d0165cd44561ac78cc",
};
const BIBATA_MODERN_CLASSIC_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 11131753,
    sha256: "b98eca4077d6e7796ff7e710e4d220ca13439f1118160d1e7b3db9e91fdec2be",
};
const BIBATA_MODERN_CLASSIC_RIGHT_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 11104412,
    sha256: "0949d0857d614bbbea79350b823a5cc7203b4220a716350918ef40f647d450bc",
};
const BIBATA_MODERN_ICE_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 10277542,
    sha256: "0045e40324da5b540b3bee260f53d0792df62be9cdef91655a024ae9f151bd04",
};
const BIBATA_MODERN_ICE_RIGHT_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 10232040,
    sha256: "6adec5b9db0e3c5ec218821a1c1f62ccd83f43fd2257a0e31b1a99d815f7e9c3",
};
const BIBATA_ORIGINAL_AMBER_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 11894568,
    sha256: "cdae96d74c55e11142a72a1d8cc9a2a2c4a9efd3e792623dd1033043f319e4e5",
};
const BIBATA_ORIGINAL_AMBER_RIGHT_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 11903951,
    sha256: "32acb00c4181be3851be3302630eaf42a4aae5f3a1c1e11df06b7380777dd2d0",
};
const BIBATA_ORIGINAL_CLASSIC_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 11075566,
    sha256: "5b1d47004039f5c844f0e21fa3498f6396790e9133ea7f8396e6487a3000a34e",
};
const BIBATA_ORIGINAL_CLASSIC_RIGHT_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 11035869,
    sha256: "35ebc194729fea454d98fbe40d8a550a38d4434f6037f72f7fdb39cd28be9234",
};
const BIBATA_ORIGINAL_ICE_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 10184464,
    sha256: "868f2afc0fd91d334ec71962c339606defa5d85ce58fccc04485d54a4ac71aa2",
};
const BIBATA_ORIGINAL_ICE_RIGHT_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 10124679,
    sha256: "caa802eda19eb6c704530ea918c9b4b6edd336268f5c8e89bd0a58e26f556b1d",
};
const POSY_DEFAULT_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 900985,
    sha256: "e82372041fd29b803914280bf6207586a5c3b4e96cab63c313e40eb371d0506c",
};
const POSY_BLACK_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 911067,
    sha256: "d5ce2a4c1e9e9480737397411a5d9e2ae129e434c7d2e97466395af848f8d49f",
};
const RW_NERO_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 423509,
    sha256: "74a7ab86369d101b9a0076a3d57e4eea06eef532f8639f3b807d5f478300b22a",
};
const RW_BREEZE_NORD_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 35027,
    sha256: "9a2ac4dba5b179f5e07f3df8c8c9672679299499dfefd2c4e1edf2a7457a9371",
};
const RW_ALKANO_ZIP_PIN: PinnedBytes = PinnedBytes {
    size: 676532,
    sha256: "04f165a134605416a315e8a96bea2c25bfdb4617e2cfd76bf22105dce5b3c80f",
};
const COMIX_RAR_PIN: PinnedBytes = PinnedBytes {
    size: 290775,
    sha256: "592c17544c9c07f8e21b0a6b0a81d4993264bd418e309009f9135ed5f7724822",
};

pub(super) const COLLECTIONS: [CursorCollectionSpec; 35] = [
    CursorCollectionSpec {
        id: "material-dark-default",
        title: "Material Design Dark v2 (Default)",
        scheme_name: "Material Design Dark v2 by Jepri Creations",
        source: CollectionSource::Github(&github_pins::MATERIAL_DEFAULT),
        file_name_map: &NO_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "material-dark-pure-black",
        title: "Material Design Dark v2 (Pure Black)",
        scheme_name: "Material Design Pure Dark v2 by Jepri Creations",
        source: CollectionSource::Github(&github_pins::MATERIAL_PURE_BLACK),
        file_name_map: &NO_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-tail-light-candy",
        title: "W11 Tail HDPI Light (Candy)",
        scheme_name: "W11 Tail HDPI Light Candy by Jepri Creations",
        source: CollectionSource::Github(&github_pins::TAIL_LIGHT_CANDY),
        file_name_map: &NO_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-tail-dark-candy",
        title: "W11 Tail HDPI Dark (Candy)",
        scheme_name: "W11 Tail HDPI Dark Candy by Jepri Creations",
        source: CollectionSource::Github(&github_pins::TAIL_DARK_CANDY),
        file_name_map: &NO_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "cursor-concept-1-dark",
        title: "Cursor Concept 1 HDPI (Dark)",
        scheme_name: "Cursor Concept 1 HDPI Dark by Jepri Creations",
        source: CollectionSource::Github(&github_pins::CONCEPT1_DARK),
        file_name_map: &CONCEPT1_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "cursor-concept-1-light",
        title: "Cursor Concept 1 HDPI (Light)",
        scheme_name: "Cursor Concept 1 HDPI Light by Jepri Creations",
        source: CollectionSource::Github(&github_pins::CONCEPT1_LIGHT),
        file_name_map: &CONCEPT1_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-concept-dark-archive",
        title: "W11 Cursors Concept (Dark, Archive)",
        scheme_name: "W11 Cursor Concept Dark Free by Jepri Creations",
        source: CollectionSource::Github(&github_pins::ARCHIVE_W11_CONCEPT_DARK),
        file_name_map: &ARCHIVE_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-concept-light-archive",
        title: "W11 Cursors Concept (Light, Archive)",
        scheme_name: "W11 Cursor Concept Light Free by Jepri Creations",
        source: CollectionSource::Github(&github_pins::ARCHIVE_W11_CONCEPT_LIGHT),
        file_name_map: &ARCHIVE_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-tail-dark-archive",
        title: "W11 Tail Cursor Concept (Dark, Archive)",
        scheme_name: "W11 Tail Cursor Concept Dark Free by Jepri Creations",
        source: CollectionSource::Github(&github_pins::ARCHIVE_W11_TAIL_DARK),
        file_name_map: &ARCHIVE_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "w11-tail-light-archive",
        title: "W11 Tail Cursor Concept (Light, Archive)",
        scheme_name: "W11 Tail Cursor Concept Light Free by Jepri Creations",
        source: CollectionSource::Github(&github_pins::ARCHIVE_W11_TAIL_LIGHT),
        file_name_map: &ARCHIVE_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "antiden-sierra-shadow",
        title: "macOS Sierra+ (With Shadow)",
        scheme_name: "macOS Cursors With Shadow Newer",
        source: CollectionSource::Github(&github_pins::ANTIDEN_SIERRA_SHADOW),
        file_name_map: &ANTIDEN_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "breeze-plasma6",
        title: "KDE Breeze 6.1",
        scheme_name: "KDE Breeze 6.1",
        source: CollectionSource::Github(&github_pins::BREEZE_PLASMA6),
        file_name_map: &BREEZE_PLASMA_FILE_MAP,
    },
    CursorCollectionSpec {
        id: "tahoe-tail-shadow",
        title: "MacOS Tahoe Tail (With Shadow)",
        scheme_name: "MacOS Tahoe Cursor (With Shadow)",
        source: CollectionSource::ZipArchive {
            url: TAHOE_ZIP_URL,
            subdir: "MacOS Tahoe Cursors/Tail/With Shadow",
            pin: TAHOE_ZIP_PIN,
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
            pin: TAHOE_ZIP_PIN,
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
            pin: VISION_BLACK_ZIP_PIN,
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
            pin: VISION_WHITE_ZIP_PIN,
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
            pin: BIBATA_MODERN_AMBER_ZIP_PIN,
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
            pin: BIBATA_MODERN_AMBER_RIGHT_ZIP_PIN,
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
            pin: BIBATA_MODERN_CLASSIC_ZIP_PIN,
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
            pin: BIBATA_MODERN_CLASSIC_RIGHT_ZIP_PIN,
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
            pin: BIBATA_MODERN_ICE_ZIP_PIN,
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
            pin: BIBATA_MODERN_ICE_RIGHT_ZIP_PIN,
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
            pin: BIBATA_ORIGINAL_AMBER_ZIP_PIN,
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
            pin: BIBATA_ORIGINAL_AMBER_RIGHT_ZIP_PIN,
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
            pin: BIBATA_ORIGINAL_CLASSIC_ZIP_PIN,
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
            pin: BIBATA_ORIGINAL_CLASSIC_RIGHT_ZIP_PIN,
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
            pin: BIBATA_ORIGINAL_ICE_ZIP_PIN,
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
            pin: BIBATA_ORIGINAL_ICE_RIGHT_ZIP_PIN,
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
            pin: POSY_DEFAULT_ZIP_PIN,
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
            pin: POSY_BLACK_ZIP_PIN,
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
            pin: RW_NERO_ZIP_PIN,
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
            pin: RW_BREEZE_NORD_ZIP_PIN,
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
            pin: RW_ALKANO_ZIP_PIN,
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
            pin: COMIX_RAR_PIN,
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
            pin: COMIX_RAR_PIN,
        },
        file_name_map: &COMIX_FILE_MAP,
    },
];
