// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use alloc::string::String;
use alloc::vec::Vec;

use crate::RgbaImage;

/// The platform a file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[repr(u32)]
pub enum Platform {
    /// Nintendo DS.
    Ds = 1,
    /// Nintendo DSi (a DSi-only or DSi-enhanced unit code in the header).
    Dsi = 2,
    /// Nintendo 3DS.
    ThreeDs = 3,
    /// Nintendo GameCube.
    GameCube = 4,
    /// Nintendo Wii.
    Wii = 5,
    /// Nintendo Wii U.
    WiiU = 6,
    /// Game Boy (a cartridge without the Game Boy Color flag).
    GameBoy = 7,
    /// Game Boy Color (a cartridge whose header sets the CGB flag).
    GameBoyColor = 8,
    /// Game Boy Advance.
    GameBoyAdvance = 9,
    /// Nintendo 64.
    Nintendo64 = 10,
    /// Super Nintendo / Super Famicom.
    Snes = 11,
    /// Nintendo Entertainment System / Famicom.
    Nes = 12,
    /// Sega Genesis / Mega Drive.
    MegaDrive = 13,
    /// Sega 32X (a Mega Drive header whose system string says 32X).
    Sega32x = 14,
    /// Sega Master System.
    MasterSystem = 15,
    /// Sega Game Gear.
    GameGear = 16,
    /// Sony PSP.
    Psp = 17,
    /// Sony PlayStation 3.
    Ps3 = 18,
    /// Sony PlayStation Vita.
    Vita = 19,
}

/// Title languages, numbered as 3DS SMDH files store them (slots 12-15 are unused there and dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
#[allow(missing_docs)]
pub enum Language {
    Japanese = 0,
    English = 1,
    French = 2,
    German = 3,
    Italian = 4,
    Spanish = 5,
    ChineseSimplified = 6,
    Korean = 7,
    Dutch = 8,
    Portuguese = 9,
    Russian = 10,
    ChineseTraditional = 11,
}

impl Language {
    /// Every language, in slot order.
    pub const ALL: [Language; 12] = [
        Language::Japanese,
        Language::English,
        Language::French,
        Language::German,
        Language::Italian,
        Language::Spanish,
        Language::ChineseSimplified,
        Language::Korean,
        Language::Dutch,
        Language::Portuguese,
        Language::Russian,
        Language::ChineseTraditional,
    ];

    /// The language in `slot`, if there is one.
    pub fn from_slot(slot: usize) -> Option<Language> {
        Self::ALL.get(slot).copied()
    }
}

/// Regions a header names, numbered as the 3DS SMDH region-lockout bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
#[allow(missing_docs)]
pub enum Region {
    Japan = 0,
    NorthAmerica = 1,
    Europe = 2,
    Australia = 3,
    China = 4,
    Korea = 5,
    Taiwan = 6,
}

impl Region {
    /// Every region, in bit order.
    pub const ALL: [Region; 7] = [
        Region::Japan,
        Region::NorthAmerica,
        Region::Europe,
        Region::Australia,
        Region::China,
        Region::Korea,
        Region::Taiwan,
    ];
}

/// Age-rating boards, numbered by their slot in the 3DS SMDH ratings table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
#[allow(missing_docs)]
pub enum RatingBoard {
    Cero = 0,
    Esrb = 1,
    Usk = 3,
    Pegi = 4,
    PegiPrt = 6,
    Bbfc = 7,
    Cob = 8,
    Grb = 9,
    Cgsrr = 10,
}

impl RatingBoard {
    /// Every board, in slot order.
    pub const ALL: [RatingBoard; 9] = [
        RatingBoard::Cero,
        RatingBoard::Esrb,
        RatingBoard::Usk,
        RatingBoard::Pegi,
        RatingBoard::PegiPrt,
        RatingBoard::Bbfc,
        RatingBoard::Cob,
        RatingBoard::Grb,
        RatingBoard::Cgsrr,
    ];
}

/// One language's title.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Title {
    /// What a list shows: the long title (else the short one), lines joined with a space.
    pub name: String,
    /// The short title, lines joined with a space.
    pub short_name: String,
    /// The publisher, when the file names one.
    pub publisher: Option<String>,
}

/// The ids a header carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct GameIds {
    /// 3DS, Wii U, Wii WAD: the 64-bit title id as 16 upper-case hex digits.
    pub title_id: Option<String>,
    /// 3DS: the product code (`CTR-P-XXXX`); Wii U: `WUP-P-XXXX`.
    pub product_code: Option<String>,
    /// DS: the 4-character game code (header 0x0C); GameCube and Wii discs: the 6-character game id (game code then
    /// maker code); a Wii WAD: the 4 ASCII characters inside its title id.
    pub game_id: Option<String>,
    /// DS: the 2-character maker code (header 0x10); GameCube and Wii discs: the last two characters of the game id.
    pub maker_code: Option<String>,
}

/// One step of a DSi animated icon: show `frame` (flips and palette already applied) for `duration_frames` sixtieths
/// of a second.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IconStep {
    /// The frame (32x32).
    pub frame: RgbaImage,
    /// How long it shows, in 1/60 s.
    pub duration_frames: u32,
}

/// The pictures a GameCube, Wii or Wii U file can hold, numbered as the C API's `GmImage` bits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum ImageKind {
    /// The Wii U `iconTex.tga` (128x128).
    Icon = 0,
    /// The GameCube `opening.bnr` banner (96x32).
    Banner = 1,
    /// A Wii disc's title logo: the wide picture with transparency in its `icon.bin`.
    Logo = 2,
    /// The Wii U TV boot splash `bootTvTex.tga` (1280x720).
    TvSplash = 3,
    /// The Wii U GamePad boot splash `bootDrcTex.tga` (854x480).
    GamePadSplash = 4,
    /// The Wii U `bootLogoTex.tga`.
    BootLogo = 5,
    /// A PSP game's `PIC1.PNG` background (480x272), decoded on demand.
    Background = 6,
}

/// A byte range of the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Extent {
    /// Absolute offset from the start of the file.
    pub offset: u64,
    /// Length in bytes.
    pub size: u64,
}

/// What a reader found in one file. Every field but [`platform`](Self::platform) may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameInfo {
    /// The platform the file belongs to.
    pub platform: Platform,
    /// Titles by language, in language order, only the languages the file fills.
    pub titles: Vec<(Language, Title)>,
    /// The main icon (3DS: 48x48).
    pub icon: Option<RgbaImage>,
    /// The small icon (3DS: 24x24).
    pub small_icon: Option<RgbaImage>,
    /// A DSi animated icon's steps; empty for a still icon.
    pub animated_icon: Vec<IconStep>,
    /// The ids the header carries.
    pub ids: GameIds,
    /// The regions the file is locked to, in region order; empty when it names none or is region free.
    pub regions: Vec<Region>,
    /// The file allows every region.
    pub region_free: bool,
    /// Active age ratings, in board order: board to minimum age (0 = no age restriction).
    pub ratings: Vec<(RatingBoard, u8)>,
    /// 3DS: where the ExeFS `banner` (the HOME-menu banner model) sits; it is located, never read.
    pub banner: Option<Extent>,
    /// GameCube, Wii, Wii U: the pictures the file holds. [`icon`](Self::icon) is the small one read with the titles
    /// (the Wii U icon, the GameCube banner, the Wii title logo); the large Wii U splashes are decoded on demand.
    pub images: Vec<ImageKind>,
    /// Wii U: the size of `bootMovie.h264`, when the title has one (presence and size only).
    pub boot_movie: Option<u64>,
    /// Wii U: the size of `bootSound.btsnd`, when the title has one.
    pub boot_sound: Option<u64>,
    /// GameCube and Wii: the disc header's own title (64 characters at 0x20), for a disc with no banner text.
    pub header_title: Option<String>,
    /// Cartridge consoles (Game Boy to Game Gear): the game's version byte or number, when the header has one.
    pub version: Option<u32>,
    /// Cartridge consoles: the header as typed fields (every field the reader decodes, raw codes included).
    pub cart: Option<crate::carts::CartHeader>,
}

impl GameInfo {
    /// An empty result for `platform`: every field but the platform blank.
    pub fn new(platform: Platform) -> Self {
        GameInfo {
            platform,
            titles: Vec::new(),
            icon: None,
            small_icon: None,
            animated_icon: Vec::new(),
            ids: GameIds::default(),
            regions: Vec::new(),
            region_free: false,
            ratings: Vec::new(),
            banner: None,
            images: Vec::new(),
            boot_movie: None,
            boot_sound: None,
            header_title: None,
            version: None,
            cart: None,
        }
    }

    /// The title in `language`, if the file has one.
    pub fn title_in(&self, language: Language) -> Option<&Title> {
        self.titles.iter().find(|(l, _)| *l == language).map(|(_, t)| t)
    }

    /// English if present, else the first language that has one.
    pub fn preferred_title(&self) -> Option<(Language, &Title)> {
        self.titles.iter().find(|(l, _)| *l == Language::English).or_else(|| self.titles.first()).map(|(l, t)| (*l, t))
    }

    /// The name a list shows: [`preferred_title`](Self::preferred_title)'s name, `None` when blank.
    pub fn title(&self) -> Option<&str> {
        self.preferred_title().map(|(_, t)| t.name.as_str()).filter(|n| !crate::text::is_blank(n))
    }
}
