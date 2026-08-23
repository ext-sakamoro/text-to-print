use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use text_to_print_core::db::{Database, GenerationRow};
use text_to_print_core::pipeline::MeshStats;
use text_to_print_core::tier::Tier;
use text_to_print_llm::backend::LlmConfig;
use text_to_print_llm::backend_kind::{BackendKind, EmbeddedStatus, ExecutionMode};
use text_to_print_llm::embedded_backend::EmbeddedBackend;
use text_to_print_llm::sidecar::SidecarStatus;
use text_to_print_network::presets_client::{PresetCategory, PresetsResponse, parse_presets_json};

/// Bundled default presets JSON — 起動時 Cloudflare / cache いずれも空なら
/// 本 blob を parse して初期 preset とする (β 初期でも offline でも app が動く)
/// worker crate 側の同名 file と手動 sync (schema 一致)
pub const BUNDLED_DEFAULT_PRESETS_JSON: &str = include_str!("default_presets.json");

/// Ordered pipeline phases surfaced to the UI progress indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenerationPhase {
    Llm,
    Parse,
    Mesh,
    Safety,
    Export,
}

impl GenerationPhase {
    pub const ALL: [Self; 5] = [
        Self::Llm,
        Self::Parse,
        Self::Mesh,
        Self::Safety,
        Self::Export,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Llm => "LLM",
            Self::Parse => "parse",
            Self::Mesh => "mesh",
            Self::Safety => "safety",
            Self::Export => "export",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PhaseProgress {
    pub current: Option<GenerationPhase>,
    /// Latency of each completed phase, in the order they completed.
    pub completed: Vec<(GenerationPhase, Duration)>,
    /// Retry count for the LLM phase (surfaced by Stage 8 backend when wired).
    pub retry_count: u32,
    /// Wall-clock instant when the current generation started
    ///
    /// Populated by `prompt.rs` right before the LLM inference request is
    /// dispatched, cleared on `reset()` The UI reads this to show a running
    /// elapsed-time counter — otherwise the progress bar sits at 0% for the
    /// entire LLM phase (which dominates the wall-clock time) and users
    /// cannot tell whether the app is stuck or working
    pub generation_start: Option<std::time::Instant>,
}

impl PhaseProgress {
    pub fn reset(&mut self) {
        self.current = None;
        self.completed.clear();
        self.retry_count = 0;
        self.generation_start = None;
    }

    pub fn is_done(&self, phase: GenerationPhase) -> bool {
        self.completed.iter().any(|(p, _)| *p == phase)
    }

    pub fn latency_of(&self, phase: GenerationPhase) -> Option<Duration> {
        self.completed
            .iter()
            .find(|(p, _)| *p == phase)
            .map(|(_, d)| *d)
    }

    /// Elapsed time since `generation_start` was set — returns `None` when
    /// no generation is currently in flight
    pub fn elapsed(&self) -> Option<Duration> {
        self.generation_start.map(|t| t.elapsed())
    }
}

/// パラメータ入力可能な template の UI 状態 (in-memory only、DB 永続化なし)
///
/// 経路 A (固定 preset button) と経路 B (自然言語 LLM) の中間に位置する
/// **customizer 経路**を提供する archetype 毎に UI state を保持する
///
/// Sprint 6-9 の追加分 (filament_spool_holder / nozzle_holder /
/// build_plate_rack / cable_clip / led_channel / card_tray /
/// token_well / spice_rack / egg_tray / utensil_caddy / etc.) は
/// UI 側の呼出 site が未完成な段階があり、dead_code lint を許容する
/// (Wire 完了時に allow を削除する予定)
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct CustomizerState {
    /// Gridfinity bin customizer (organizer-gridfinity-desk PART 1)
    pub gridfinity: GridfinityUiState,
    /// Sticky note holder customizer (organizer-gridfinity-desk § 2.7)
    pub sticky_note: StickyNoteUiState,
    /// Business card holder customizer (§ 2.6)
    pub business_card: BusinessCardUiState,
    /// Pen cup customizer (§ 2.2)
    pub pen_cup: PenCupUiState,
    /// Phone stand customizer (§ 2.9)
    pub phone_stand: PhoneStandUiState,
    /// Headphone holder customizer (§ 2.5、Phase B2)
    pub headphone_holder: HeadphoneHolderUiState,
    /// Under-desk clamp mount customizer (§ 2.4、Phase B2)
    pub under_desk_mount: UnderDeskMountUiState,
    /// Desk shelf customizer (§ 2.3、Phase B2)
    pub desk_shelf: DeskShelfUiState,
    /// Monitor riser customizer (§ 2.1、Phase B2、簡易版 単一プリント)
    pub monitor_riser: MonitorRiserUiState,
    /// Coaster customizer (household § 7、Sprint 4)
    pub coaster: CoasterUiState,
    /// Tissue box cover customizer (household § 1、Sprint 4)
    pub tissue_box_cover: TissueBoxCoverUiState,
    /// Storage box customizer (household § 3、Sprint 4、基本形 lid なし)
    pub storage_box: StorageBoxUiState,
    /// Cable clip customizer (hobby-diy § 2、Sprint 5)
    pub cable_clip: CableClipUiState,
    /// LED strip channel customizer (hobby-diy § 3、Sprint 5)
    pub led_channel: LedChannelUiState,
    /// Card tray customizer (hobby-diy § 6、Sprint 5)
    pub card_tray: CardTrayUiState,
    /// Token well customizer (hobby-diy § 6、Sprint 5)
    pub token_well: TokenWellUiState,
    /// Wrench holder customizer (tools § 1、Sprint 6)
    pub wrench_holder: WrenchHolderUiState,
    /// Socket rail customizer (tools § 2、Sprint 6)
    pub socket_rail: SocketRailUiState,
    /// Hex bit holder customizer (tools § 3、Sprint 6)
    pub hex_bit_holder: HexBitHolderUiState,
    /// Raspberry Pi case customizer (electronics-enclosure § 1、Sprint 7)
    pub raspi_case: RaspiCaseUiState,
    /// ESP32/Arduino enclosure customizer (electronics-enclosure § 2、Sprint 7)
    pub esp32_enclosure: Esp32EnclosureUiState,
    /// 18650 battery holder customizer (electronics-enclosure § 3、Sprint 7)
    pub battery_18650_holder: Battery18650HolderUiState,
    /// Toothbrush holder customizer (bathroom § 7.1、Sprint 8)
    pub toothbrush_holder: ToothbrushHolderUiState,
    /// Drill bit holder customizer (garage § 8.1、Sprint 8)
    pub drill_bit_holder: DrillBitHolderUiState,
    /// Pliers rack customizer (garage § 8.4、Sprint 8)
    pub pliers_rack: PliersRackUiState,
    /// Spice rack customizer (kitchen § 6.1、Sprint 9)
    pub spice_rack: SpiceRackUiState,
    /// Egg tray customizer (kitchen § 6.5、Sprint 9)
    pub egg_tray: EggTrayUiState,
    /// Utensil caddy customizer (kitchen § 6.8、Sprint 9)
    pub utensil_caddy: UtensilCaddyUiState,
    /// Filament spool holder customizer (printer § 9.1、Sprint 10)
    pub filament_spool_holder: FilamentSpoolHolderUiState,
    /// Nozzle holder customizer (printer § 9.5、Sprint 10)
    pub nozzle_holder: NozzleHolderUiState,
    /// Build plate rack customizer (printer § 9.6、Sprint 10)
    pub build_plate_rack: BuildPlateRackUiState,
    /// Cutlery tray customizer (drawer § 3.2、Sprint 11)
    pub cutlery_tray: CutleryTrayUiState,
    /// Pill organizer customizer (drawer § 3.6、Sprint 11)
    pub pill_organizer: PillOrganizerUiState,
    /// Magnetic strip customizer (wall § 4.6、Sprint 11)
    pub magnetic_strip: MagneticStripUiState,
    /// Hairdryer holder customizer (bathroom § 7.7、Sprint 12)
    pub hairdryer_holder: HairdryerHolderUiState,
    /// K-Cup holder customizer (kitchen § 6.7、Sprint 12)
    pub kcup_holder: KcupHolderUiState,
    /// Hex key holder customizer (garage § 8.2、Sprint 12)
    pub hex_key_holder: HexKeyHolderUiState,
    /// Wrap/foil holder customizer (kitchen § 6.2、Sprint 13)
    pub wrap_holder: WrapHolderUiState,
    /// Sock divider customizer (drawer § 3.7、Sprint 13)
    pub sock_divider: SockDividerUiState,
    /// Soap tray customizer (bathroom § 7.3、Sprint 13)
    pub soap_tray: SoapTrayUiState,
    /// Razor holder customizer (bathroom § 7.2、Sprint 14)
    pub razor_holder: RazorHolderUiState,
    /// Chopstick holder customizer (drawer § 3.3、Sprint 14)
    pub chopstick_holder: ChopstickHolderUiState,
    /// Filament swatch holder customizer (printer § 9.7、Sprint 14)
    pub swatch_holder: SwatchHolderUiState,
    /// Toilet paper holder customizer (bathroom § 7.6、Sprint 15)
    pub tp_holder: TpHolderUiState,
    /// SD card holder customizer (printer § 9.4、Sprint 15)
    pub sd_card_holder: SdCardHolderUiState,
    /// Screwdriver rack customizer (garage § 8.5、Sprint 15)
    pub driver_rack: DriverRackUiState,
    /// Cotton/swab dispenser customizer (bathroom § 7.4、Sprint 16)
    pub cotton_dispenser: CottonDispenserUiState,
    /// Sink sponge caddy customizer (kitchen § 6.9、Sprint 16)
    pub sink_caddy: SinkCaddyUiState,
    /// Clamp wall rack customizer (garage § 8.8、Sprint 16)
    pub clamp_rack: ClampRackUiState,
}

/// Gridfinity bin customizer UI state (basic 3 param + advanced 5 field)
///
/// Basic: units_x, units_y, height_u = `gridfinity_bin(ux, uy, hu)`
/// Advanced: 上記 + dividers (use/x/y) + wall/floor thickness = `gridfinity_bin_ex(...)`
#[derive(Debug, Clone, Copy)]
pub struct GridfinityUiState {
    /// units X 方向 (1 unit = 42mm、range 1-6)
    pub units_x: u32,
    /// units Y 方向 (range 1-6)
    pub units_y: u32,
    /// 高さ U 数 (1U = 7mm、range 2-10)
    pub height_u: u32,
    /// advanced: dividers 有効化 (true → 内部仕切りあり)
    pub use_dividers: bool,
    /// advanced: X 方向 dividers (2-6、use_dividers=true 時のみ有効)
    pub dividers_x: u32,
    /// advanced: Y 方向 dividers (2-6、use_dividers=true 時のみ有効)
    pub dividers_y: u32,
    /// advanced: 壁厚 (mm、range 0.8-3.0、default 1.2)
    pub wall_thickness: f32,
    /// advanced: 底厚 (mm、range 1.0-4.0、default 1.5)
    pub floor_thickness: f32,
}

impl Default for GridfinityUiState {
    fn default() -> Self {
        // Gridfinity 2×2 × 6U (~84×84×46mm) = 最典型 default
        Self {
            units_x: 2,
            units_y: 2,
            height_u: 6,
            use_dividers: false,
            dividers_x: 2,
            dividers_y: 2,
            wall_thickness: 1.2,
            floor_thickness: 1.5,
        }
    }
}

impl GridfinityUiState {
    /// LOL DSL string 組立
    ///
    /// use_dividers=true or wall/floor が default 以外なら `gridfinity_bin_ex`、
    /// それ以外は basic `gridfinity_bin` を使う
    pub fn to_lol(self) -> String {
        let default_wall = (self.wall_thickness - 1.2).abs() < 0.01;
        let default_floor = (self.floor_thickness - 1.5).abs() < 0.01;
        if self.use_dividers || !default_wall || !default_floor {
            let (dx, dy) = if self.use_dividers {
                (self.dividers_x, self.dividers_y)
            } else {
                (0, 0)
            };
            format!(
                "gridfinity_bin_ex({}, {}, {}, {}, {}, {}, {})",
                self.units_x,
                self.units_y,
                self.height_u,
                dx,
                dy,
                self.wall_thickness,
                self.floor_thickness
            )
        } else {
            format!(
                "gridfinity_bin({}, {}, {})",
                self.units_x, self.units_y, self.height_u
            )
        }
    }
}

/// 付箋ホルダー customizer UI state (`sticky_note_holder(pad_w, pad_d, height)`)
#[derive(Debug, Clone, Copy)]
pub struct StickyNoteUiState {
    /// pad 幅 (mm、default 76 = Post-it 3 inch)
    pub pad_width: f32,
    /// pad 深さ (mm、default 76 or 127)
    pub pad_depth: f32,
    /// 全高 (mm、default 30 = 3-8 枚分)
    pub height: f32,
}

impl Default for StickyNoteUiState {
    fn default() -> Self {
        Self {
            pad_width: 76.0,
            pad_depth: 76.0,
            height: 30.0,
        }
    }
}

impl StickyNoteUiState {
    pub fn to_lol(self) -> String {
        format!(
            "sticky_note_holder({}, {}, {})",
            self.pad_width, self.pad_depth, self.height
        )
    }
}

/// 名刺ホルダー customizer UI state (`business_card_holder(card_w, card_h, slot_thickness)`)
#[derive(Debug, Clone, Copy)]
pub struct BusinessCardUiState {
    /// card 幅 (mm、default 91 = JP meishi)
    pub card_width: f32,
    /// card 高さ (mm、default 55 = JP meishi)
    pub card_height: f32,
    /// slot 厚 (mm、default 22 = 30-50 枚分)
    pub slot_thickness: f32,
}

impl Default for BusinessCardUiState {
    fn default() -> Self {
        Self {
            card_width: 91.0,
            card_height: 55.0,
            slot_thickness: 22.0,
        }
    }
}

impl BusinessCardUiState {
    pub fn to_lol(self) -> String {
        format!(
            "business_card_holder({}, {}, {})",
            self.card_width, self.card_height, self.slot_thickness
        )
    }
}

/// ペン立て customizer UI state (`pen_cup(inner_dia, height)`)
#[derive(Debug, Clone, Copy)]
pub struct PenCupUiState {
    /// cup 内径 (mm、default 75)
    pub inner_diameter: f32,
    /// cup 全高 (mm、default 100)
    pub height: f32,
}

impl Default for PenCupUiState {
    fn default() -> Self {
        Self {
            inner_diameter: 75.0,
            height: 100.0,
        }
    }
}

impl PenCupUiState {
    pub fn to_lol(self) -> String {
        format!("pen_cup({}, {})", self.inner_diameter, self.height)
    }
}

/// スマホスタンド customizer UI state (`phone_stand(slot_w, back_h, cable_dia)`)
#[derive(Debug, Clone, Copy)]
pub struct PhoneStandUiState {
    /// slot 幅 (mm、default 14 = phone、ケース対応時 +2-3)
    pub slot_width: f32,
    /// back plate 高さ (mm、default 100 phone / 150-190 tablet)
    pub back_height: f32,
    /// cable 通し穴径 (mm、default 18、0 で穴なし)
    pub cable_hole_dia: f32,
}

impl Default for PhoneStandUiState {
    fn default() -> Self {
        Self {
            slot_width: 14.0,
            back_height: 100.0,
            cable_hole_dia: 18.0,
        }
    }
}

impl PhoneStandUiState {
    pub fn to_lol(self) -> String {
        format!(
            "phone_stand({}, {}, {})",
            self.slot_width, self.back_height, self.cable_hole_dia
        )
    }
}

/// ヘッドホンホルダー customizer UI state (`headphone_holder(arm_len, headband_w, mount_w)`)
#[derive(Debug, Clone, Copy)]
pub struct HeadphoneHolderUiState {
    /// hook arm 長さ (mm、default 80、range 60-120)
    pub arm_length: f32,
    /// headband 幅 = arm width (mm、default 50、range 30-70)
    pub headband_width: f32,
    /// mount plate 幅 (mm、default 100、range 60-150)
    pub mount_width: f32,
}

impl Default for HeadphoneHolderUiState {
    fn default() -> Self {
        Self {
            arm_length: 80.0,
            headband_width: 50.0,
            mount_width: 100.0,
        }
    }
}

impl HeadphoneHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "headphone_holder({}, {}, {})",
            self.arm_length, self.headband_width, self.mount_width
        )
    }
}

/// 机下 clamp mount customizer UI state (`under_desk_mount(desk_t, clamp_w, screw_dia)`)
#[derive(Debug, Clone, Copy)]
pub struct UnderDeskMountUiState {
    /// desk thickness = clamp gap (mm、default 25、range 15-60)
    pub desk_thickness: f32,
    /// clamp jaw 幅 (mm、default 40、range 20-80)
    pub clamp_width: f32,
    /// screw hole 径 (mm、default 4 = M4、0 で穴なし = 両面テープ)
    pub screw_hole_dia: f32,
}

impl Default for UnderDeskMountUiState {
    fn default() -> Self {
        Self {
            desk_thickness: 25.0,
            clamp_width: 40.0,
            screw_hole_dia: 4.0,
        }
    }
}

impl UnderDeskMountUiState {
    pub fn to_lol(self) -> String {
        format!(
            "under_desk_mount({}, {}, {})",
            self.desk_thickness, self.clamp_width, self.screw_hole_dia
        )
    }
}

/// 卓上シェルフ customizer UI state (`desk_shelf(shelf_w, shelf_d, leg_h)`)
#[derive(Debug, Clone, Copy)]
pub struct DeskShelfUiState {
    /// shelf 幅 (mm、default 400、range 200-500)
    pub shelf_width: f32,
    /// shelf 奥行 (mm、default 200、range 150-300)
    pub shelf_depth: f32,
    /// leg 高さ (mm、default 100、range 60-150)
    pub leg_height: f32,
}

impl Default for DeskShelfUiState {
    fn default() -> Self {
        Self {
            shelf_width: 400.0,
            shelf_depth: 200.0,
            leg_height: 100.0,
        }
    }
}

impl DeskShelfUiState {
    pub fn to_lol(self) -> String {
        format!(
            "desk_shelf({}, {}, {})",
            self.shelf_width, self.shelf_depth, self.leg_height
        )
    }
}

/// モニターライザー customizer UI state (`monitor_riser(width, depth, height)`)
///
/// 簡易版 = 単一プリント想定、UI slider max=280mm (Bambu H2D 315mm 内)
#[derive(Debug, Clone, Copy)]
pub struct MonitorRiserUiState {
    /// 全幅 (mm、default 250、range 200-280)
    pub width: f32,
    /// 全奥行 (mm、default 180、range 150-240)
    pub depth: f32,
    /// 全高 (mm、default 90、range 60-120)
    pub height: f32,
}

impl Default for MonitorRiserUiState {
    fn default() -> Self {
        Self {
            width: 250.0,
            depth: 180.0,
            height: 90.0,
        }
    }
}

impl MonitorRiserUiState {
    pub fn to_lol(self) -> String {
        format!(
            "monitor_riser({}, {}, {})",
            self.width, self.depth, self.height
        )
    }
}

/// コースター customizer UI state (`coaster(diameter, thickness)`)
#[derive(Debug, Clone, Copy)]
pub struct CoasterUiState {
    /// 直径 (mm、default 95、range 80-110)
    pub diameter: f32,
    /// 全厚 (mm、default 5、range 4-8)
    pub thickness: f32,
}

impl Default for CoasterUiState {
    fn default() -> Self {
        Self {
            diameter: 95.0,
            thickness: 5.0,
        }
    }
}

impl CoasterUiState {
    pub fn to_lol(self) -> String {
        format!("coaster({}, {})", self.diameter, self.thickness)
    }
}

/// ティッシュボックスカバー customizer UI state
/// (`tissue_box_cover(internal_l, internal_w, internal_h)`)
#[derive(Debug, Clone, Copy)]
pub struct TissueBoxCoverUiState {
    /// 内部 長さ (mm、default 231 = US rectangular)
    pub internal_length: f32,
    /// 内部 幅 (mm、default 116)
    pub internal_width: f32,
    /// 内部 高さ (mm、default 53)
    pub internal_height: f32,
}

impl Default for TissueBoxCoverUiState {
    fn default() -> Self {
        Self {
            internal_length: 231.0,
            internal_width: 116.0,
            internal_height: 53.0,
        }
    }
}

impl TissueBoxCoverUiState {
    pub fn to_lol(self) -> String {
        format!(
            "tissue_box_cover({}, {}, {})",
            self.internal_length, self.internal_width, self.internal_height
        )
    }
}

/// 収納 BOX customizer UI state (`storage_box(internal_l, internal_w, internal_h)`)
#[derive(Debug, Clone, Copy)]
pub struct StorageBoxUiState {
    /// 内部 長さ (mm、default 150 = medium)
    pub internal_length: f32,
    /// 内部 幅 (mm、default 100)
    pub internal_width: f32,
    /// 内部 高さ (mm、default 60)
    pub internal_height: f32,
}

impl Default for StorageBoxUiState {
    fn default() -> Self {
        Self {
            internal_length: 150.0,
            internal_width: 100.0,
            internal_height: 60.0,
        }
    }
}

impl StorageBoxUiState {
    pub fn to_lol(self) -> String {
        format!(
            "storage_box({}, {}, {})",
            self.internal_length, self.internal_width, self.internal_height
        )
    }
}

/// ケーブルクリップ customizer UI state (`cable_clip(cable_dia, length)`)
#[derive(Debug, Clone, Copy)]
pub struct CableClipUiState {
    /// ケーブル直径 (mm、default 7 = HDMI、range 3-12)
    pub cable_diameter: f32,
    /// クリップ長 (mm、default 28、range 15-60)
    pub clip_length: f32,
}

impl Default for CableClipUiState {
    fn default() -> Self {
        Self {
            cable_diameter: 7.0,
            clip_length: 28.0,
        }
    }
}

impl CableClipUiState {
    pub fn to_lol(self) -> String {
        format!("cable_clip({}, {})", self.cable_diameter, self.clip_length)
    }
}

/// LED strip channel customizer UI state (`led_channel(strip_width, length)`)
#[derive(Debug, Clone, Copy)]
pub struct LedChannelUiState {
    /// LED strip PCB 幅 (mm、default 10 = WS2812B、range 6-20)
    pub strip_width: f32,
    /// channel 全長 (mm、default 300、range 50-1000)
    pub channel_length: f32,
}

impl Default for LedChannelUiState {
    fn default() -> Self {
        Self {
            strip_width: 10.0,
            channel_length: 300.0,
        }
    }
}

impl LedChannelUiState {
    pub fn to_lol(self) -> String {
        format!("led_channel({}, {})", self.strip_width, self.channel_length)
    }
}

/// カードトレー customizer UI state (`card_tray(card_w, card_h, depth)`)
#[derive(Debug, Clone, Copy)]
pub struct CardTrayUiState {
    /// カード幅 (mm、default 63 = Poker、range 30-80)
    pub card_width: f32,
    /// カード高さ (mm、default 88 = Poker、range 50-130)
    pub card_height: f32,
    /// tray 内深さ (mm、default 30、range 10-60)
    pub tray_depth: f32,
}

impl Default for CardTrayUiState {
    fn default() -> Self {
        Self {
            card_width: 63.0,
            card_height: 88.0,
            tray_depth: 30.0,
        }
    }
}

impl CardTrayUiState {
    pub fn to_lol(self) -> String {
        format!(
            "card_tray({}, {}, {})",
            self.card_width, self.card_height, self.tray_depth
        )
    }
}

/// トークン井戸 customizer UI state (`token_well(dia, depth, count)`)
#[derive(Debug, Clone, Copy)]
pub struct TokenWellUiState {
    /// well 直径 (mm、default 20、range 8-40)
    pub well_diameter: f32,
    /// well 深さ (mm、default 20、range 5-50)
    pub well_depth: f32,
    /// well 個数 (row 方向、default 4、range 1-10)
    pub well_count: u32,
}

impl Default for TokenWellUiState {
    fn default() -> Self {
        Self {
            well_diameter: 20.0,
            well_depth: 20.0,
            well_count: 4,
        }
    }
}

impl TokenWellUiState {
    pub fn to_lol(self) -> String {
        format!(
            "token_well({}, {}, {})",
            self.well_diameter, self.well_depth, self.well_count
        )
    }
}

/// レンチホルダー customizer UI state (`wrench_holder(min_mm, max_mm, count)`)
#[derive(Debug, Clone, Copy)]
pub struct WrenchHolderUiState {
    /// 最小レンチ幅 (mm、default 8、range 6-22)
    pub min_size_mm: f32,
    /// 最大レンチ幅 (mm、default 19、range 8-32)
    pub max_size_mm: f32,
    /// slot 個数 (default 6、range 3-12)
    pub count: u32,
}

impl Default for WrenchHolderUiState {
    fn default() -> Self {
        Self {
            min_size_mm: 8.0,
            max_size_mm: 19.0,
            count: 6,
        }
    }
}

impl WrenchHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "wrench_holder({}, {}, {})",
            self.min_size_mm, self.max_size_mm, self.count
        )
    }
}

/// ソケットレール customizer UI state (`socket_rail(post_dia, post_height, count)`)
#[derive(Debug, Clone, Copy)]
pub struct SocketRailUiState {
    /// post 直径 (mm、1/4"=6.0 / 3/8"=9.2 / 1/2"=12.4 / 3/4"=18.7、default 12.4)
    pub post_diameter: f32,
    /// post 高さ (mm、default 22、range 12-30)
    pub post_height: f32,
    /// post 個数 (default 6、range 3-15)
    pub post_count: u32,
}

impl Default for SocketRailUiState {
    fn default() -> Self {
        Self {
            post_diameter: 12.4,
            post_height: 22.0,
            post_count: 6,
        }
    }
}

impl SocketRailUiState {
    pub fn to_lol(self) -> String {
        format!(
            "socket_rail({}, {}, {})",
            self.post_diameter, self.post_height, self.post_count
        )
    }
}

/// ヘックスビットホルダー customizer UI state (`hex_bit_holder(rows, cols, spacing)`)
///
/// 1/4" bit 想定、hex hole は 6.85mm across-flats × 14mm depth 固定
#[derive(Debug, Clone, Copy)]
pub struct HexBitHolderUiState {
    /// 行数 (Y 方向、default 5、range 1-10)
    pub rows: u32,
    /// 列数 (X 方向、default 4、range 1-10)
    pub cols: u32,
    /// hole 中心間 pitch (mm、default 12、range 10-20)
    pub spacing: f32,
}

impl Default for HexBitHolderUiState {
    fn default() -> Self {
        Self {
            rows: 5,
            cols: 4,
            spacing: 12.0,
        }
    }
}

impl HexBitHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "hex_bit_holder({}, {}, {})",
            self.rows, self.cols, self.spacing
        )
    }
}

/// Raspberry Pi ケース customizer UI state (`raspi_case(pcb_w, pcb_d, internal_h)`)
#[derive(Debug, Clone, Copy)]
pub struct RaspiCaseUiState {
    /// PCB 幅 (mm、RPi 5/4=85 / Zero 2W=65、default 85)
    pub pcb_width: f32,
    /// PCB 奥行 (mm、RPi 5/4=56 / Zero 2W=30、default 56)
    pub pcb_depth: f32,
    /// PCB 上の内部高さ (mm、Active Cooler=25 / bare=15、default 25)
    pub internal_height: f32,
}

impl Default for RaspiCaseUiState {
    fn default() -> Self {
        Self {
            pcb_width: 85.0,
            pcb_depth: 56.0,
            internal_height: 25.0,
        }
    }
}

impl RaspiCaseUiState {
    pub fn to_lol(self) -> String {
        format!(
            "raspi_case({}, {}, {})",
            self.pcb_width, self.pcb_depth, self.internal_height
        )
    }
}

/// ESP32/Arduino エンクロージャ customizer UI state (`esp32_enclosure(pcb_w, pcb_d, internal_h)`)
#[derive(Debug, Clone, Copy)]
pub struct Esp32EnclosureUiState {
    /// PCB 幅 (mm、ESP32=51.6 / Uno=68.6 / Nano=45.0、default 51.6)
    pub pcb_width: f32,
    /// PCB 奥行 (mm、ESP32=28.4 / Uno=53.4 / Nano=18.0、default 28.4)
    pub pcb_depth: f32,
    /// PCB 上の内部高さ (mm、default 15、header 露出時 20)
    pub internal_height: f32,
}

impl Default for Esp32EnclosureUiState {
    fn default() -> Self {
        Self {
            pcb_width: 51.6,
            pcb_depth: 28.4,
            internal_height: 15.0,
        }
    }
}

impl Esp32EnclosureUiState {
    pub fn to_lol(self) -> String {
        format!(
            "esp32_enclosure({}, {}, {})",
            self.pcb_width, self.pcb_depth, self.internal_height
        )
    }
}

/// 18650 バッテリーホルダー customizer UI state
/// (`battery_18650_holder(count, wall_thickness, floor_thickness)`)
///
/// cell Ø18.6 × L68mm 固定、count 個 row 配置
#[derive(Debug, Clone, Copy)]
pub struct Battery18650HolderUiState {
    /// cell 個数 (row 方向、default 4、range 1-10)
    pub cell_count: u32,
    /// inter-cell wall (mm、thermal safety、default 2.5、range 2.0-4.0)
    pub wall_thickness: f32,
    /// 端部 floor 厚 (mm、0 = 両端貫通 / >0 = 片端閉塞、default 0)
    pub floor_thickness: f32,
}

impl Default for Battery18650HolderUiState {
    fn default() -> Self {
        Self {
            cell_count: 4,
            wall_thickness: 2.5,
            floor_thickness: 0.0,
        }
    }
}

impl Battery18650HolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "battery_18650_holder({}, {}, {})",
            self.cell_count, self.wall_thickness, self.floor_thickness
        )
    }
}

/// 歯ブラシホルダー customizer UI state (`toothbrush_holder(count, hole_diameter, height)`)
#[derive(Debug, Clone, Copy)]
pub struct ToothbrushHolderUiState {
    /// hole 個数 (default 4、range 1-8)
    pub count: u32,
    /// hole 直径 (mm、manual=15 / electric=40、default 15、range 10-45)
    pub hole_diameter: f32,
    /// hole 深さ = 全体 height (mm、default 70、range 50-120)
    pub hole_depth: f32,
}

impl Default for ToothbrushHolderUiState {
    fn default() -> Self {
        Self {
            count: 4,
            hole_diameter: 15.0,
            hole_depth: 70.0,
        }
    }
}

impl ToothbrushHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "toothbrush_holder({}, {}, {})",
            self.count, self.hole_diameter, self.hole_depth
        )
    }
}

/// ドリルビットホルダー customizer UI state (`drill_bit_holder(min_mm, max_mm, count)`)
#[derive(Debug, Clone, Copy)]
pub struct DrillBitHolderUiState {
    /// 最小ビット径 (mm、default 3.0、range 1.0-8.0)
    pub min_size_mm: f32,
    /// 最大ビット径 (mm、default 13.0、range 5.0-20.0)
    pub max_size_mm: f32,
    /// hole 個数 (default 11、range 5-25)
    pub count: u32,
}

impl Default for DrillBitHolderUiState {
    fn default() -> Self {
        Self {
            min_size_mm: 3.0,
            max_size_mm: 13.0,
            count: 11,
        }
    }
}

impl DrillBitHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "drill_bit_holder({}, {}, {})",
            self.min_size_mm, self.max_size_mm, self.count
        )
    }
}

/// プライヤーラック customizer UI state (`pliers_rack(slot_count, slot_width, slot_depth)`)
#[derive(Debug, Clone, Copy)]
pub struct PliersRackUiState {
    /// slot 個数 (default 6、range 3-12)
    pub slot_count: u32,
    /// slot 幅 (mm、needle-nose=10 / combi=15 / tongue-groove=20-25、default 15、range 8-30)
    pub slot_width: f32,
    /// slot 深さ (mm、default 60、range 40-90)
    pub slot_depth: f32,
}

impl Default for PliersRackUiState {
    fn default() -> Self {
        Self {
            slot_count: 6,
            slot_width: 15.0,
            slot_depth: 60.0,
        }
    }
}

impl PliersRackUiState {
    pub fn to_lol(self) -> String {
        format!(
            "pliers_rack({}, {}, {})",
            self.slot_count, self.slot_width, self.slot_depth
        )
    }
}

/// スパイスラック customizer UI state (`spice_rack(count, jar_diameter, jar_height)`)
#[derive(Debug, Clone, Copy)]
pub struct SpiceRackUiState {
    /// jar 個数 (default 6、range 3-12)
    pub count: u32,
    /// jar 直径 (mm、small=42 / std=48 / large=52、default 48、range 40-55)
    pub jar_diameter: f32,
    /// jar 高さ (mm、default 100、range 70-130、lip 高さ計算に使用)
    pub jar_height: f32,
}

impl Default for SpiceRackUiState {
    fn default() -> Self {
        Self {
            count: 6,
            jar_diameter: 48.0,
            jar_height: 100.0,
        }
    }
}

impl SpiceRackUiState {
    pub fn to_lol(self) -> String {
        format!(
            "spice_rack({}, {}, {})",
            self.count, self.jar_diameter, self.jar_height
        )
    }
}

/// 卵トレー customizer UI state (`egg_tray(rows, cols, cup_depth)`)
///
/// egg cup diameter 40mm 固定、pitch 50mm 固定
#[derive(Debug, Clone, Copy)]
pub struct EggTrayUiState {
    /// 行数 (Z 方向、default 3、range 1-8)
    pub rows: u32,
    /// 列数 (X 方向、default 4、range 1-8)
    pub cols: u32,
    /// cup depth (mm、default 18、range 12-25)
    pub cup_depth: f32,
}

impl Default for EggTrayUiState {
    fn default() -> Self {
        Self {
            rows: 3,
            cols: 4,
            cup_depth: 18.0,
        }
    }
}

impl EggTrayUiState {
    pub fn to_lol(self) -> String {
        format!("egg_tray({}, {}, {})", self.rows, self.cols, self.cup_depth)
    }
}

/// キッチンツールキャディ customizer UI state
/// (`utensil_caddy(count, compartment_dia, height)`)
#[derive(Debug, Clone, Copy)]
pub struct UtensilCaddyUiState {
    /// compartment 個数 (default 4、range 1-6)
    pub count: u32,
    /// compartment 内径 (mm、default 65、range 45-80)
    pub compartment_diameter: f32,
    /// compartment 高さ (mm、default 130、range 100-180)
    pub height: f32,
}

impl Default for UtensilCaddyUiState {
    fn default() -> Self {
        Self {
            count: 4,
            compartment_diameter: 65.0,
            height: 130.0,
        }
    }
}

impl UtensilCaddyUiState {
    pub fn to_lol(self) -> String {
        format!(
            "utensil_caddy({}, {}, {})",
            self.count, self.compartment_diameter, self.height
        )
    }
}

/// フィラメントスプールホルダー customizer UI state
/// (`filament_spool_holder(spool_od, spool_width, bore_diameter)`)
#[derive(Debug, Clone, Copy)]
pub struct FilamentSpoolHolderUiState {
    /// spool 外径 (mm、1kg=200 / 250g=125 / 2kg=250、default 200、range 100-300)
    pub spool_outer_diameter: f32,
    /// spool 幅 (mm、1kg=68 / 250g=45 / 2kg=80、default 68、range 30-120)
    pub spool_width: f32,
    /// spool bore 内径 (mm、std=52 / 2kg=70、default 52、range 30-100)
    pub bore_diameter: f32,
}

impl Default for FilamentSpoolHolderUiState {
    fn default() -> Self {
        Self {
            spool_outer_diameter: 200.0,
            spool_width: 68.0,
            bore_diameter: 52.0,
        }
    }
}

impl FilamentSpoolHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "filament_spool_holder({}, {}, {})",
            self.spool_outer_diameter, self.spool_width, self.bore_diameter
        )
    }
}

/// ノズルホルダー customizer UI state (`nozzle_holder(count, hole_diameter, depth)`)
#[derive(Debug, Clone, Copy)]
pub struct NozzleHolderUiState {
    /// hole 個数 (default 8、range 3-15)
    pub count: u32,
    /// hole 直径 (mm、E3D V6/Bambu M6=8、default 8、range 6-15)
    pub hole_diameter: f32,
    /// hole 深さ (mm、default 6、range 4-15)
    pub hole_depth: f32,
}

impl Default for NozzleHolderUiState {
    fn default() -> Self {
        Self {
            count: 8,
            hole_diameter: 8.0,
            hole_depth: 6.0,
        }
    }
}

impl NozzleHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "nozzle_holder({}, {}, {})",
            self.count, self.hole_diameter, self.hole_depth
        )
    }
}

/// ビルドプレートラック customizer UI state
/// (`build_plate_rack(slot_count, slot_spacing, height)`)
#[derive(Debug, Clone, Copy)]
pub struct BuildPlateRackUiState {
    /// slot 個数 (default 5、range 2-10)
    pub slot_count: u32,
    /// slot spacing (mm、center-to-center、default 15、range 12-25)
    pub slot_spacing: f32,
    /// rack 全高 = plate 接触幅 (mm、default 200、range 150-350)
    pub height: f32,
}

impl Default for BuildPlateRackUiState {
    fn default() -> Self {
        Self {
            slot_count: 5,
            slot_spacing: 15.0,
            height: 200.0,
        }
    }
}

impl BuildPlateRackUiState {
    pub fn to_lol(self) -> String {
        format!(
            "build_plate_rack({}, {}, {})",
            self.slot_count, self.slot_spacing, self.height
        )
    }
}

/// カトラリートレー customizer UI state (`cutlery_tray(slot_count, slot_width, slot_length)`)
#[derive(Debug, Clone, Copy)]
pub struct CutleryTrayUiState {
    /// slot 個数 (default 3、range 2-8)
    pub slot_count: u32,
    /// slot 幅 (mm、default 35、range 20-60)
    pub slot_width: f32,
    /// slot 長 (mm、default 220、range 150-350)
    pub slot_length: f32,
}

impl Default for CutleryTrayUiState {
    fn default() -> Self {
        Self {
            slot_count: 3,
            slot_width: 35.0,
            slot_length: 220.0,
        }
    }
}

impl CutleryTrayUiState {
    pub fn to_lol(self) -> String {
        format!(
            "cutlery_tray({}, {}, {})",
            self.slot_count, self.slot_width, self.slot_length
        )
    }
}

/// 薬箱 customizer UI state (`pill_organizer(rows, cols, cell_size)`)
#[derive(Debug, Clone, Copy)]
pub struct PillOrganizerUiState {
    /// 行数 (default 7、weekly = 7 days、range 1-14)
    pub rows: u32,
    /// 列数 (default 2、AM/PM、range 1-8)
    pub cols: u32,
    /// cell 内寸 (mm 正方形、default 20、range 15-30)
    pub cell_size: f32,
}

impl Default for PillOrganizerUiState {
    fn default() -> Self {
        Self {
            rows: 7,
            cols: 2,
            cell_size: 20.0,
        }
    }
}

impl PillOrganizerUiState {
    pub fn to_lol(self) -> String {
        format!(
            "pill_organizer({}, {}, {})",
            self.rows, self.cols, self.cell_size
        )
    }
}

/// マグネットストリップ customizer UI state
/// (`magnetic_strip(magnet_count, magnet_diameter, spacing)`)
#[derive(Debug, Clone, Copy)]
pub struct MagneticStripUiState {
    /// magnet 個数 (default 8、range 3-15)
    pub magnet_count: u32,
    /// magnet 直径 (mm、6mm or 8mm neodymium、default 6.0、range 4-15)
    pub magnet_diameter: f32,
    /// magnet spacing (中心間距離 mm、default 30、range 20-60)
    pub magnet_spacing: f32,
}

impl Default for MagneticStripUiState {
    fn default() -> Self {
        Self {
            magnet_count: 8,
            magnet_diameter: 6.0,
            magnet_spacing: 30.0,
        }
    }
}

impl MagneticStripUiState {
    pub fn to_lol(self) -> String {
        format!(
            "magnetic_strip({}, {}, {})",
            self.magnet_count, self.magnet_diameter, self.magnet_spacing
        )
    }
}

/// ヘアドライヤーホルダー customizer UI state
/// (`hairdryer_holder(barrel_diameter, holster_depth, wall_thickness)`)
#[derive(Debug, Clone, Copy)]
pub struct HairdryerHolderUiState {
    /// barrel 内径 (mm、Dyson=85 / 汎用=45-90、default 85、range 40-120)
    pub barrel_diameter: f32,
    /// holster 深さ (mm、default 110、range 80-150)
    pub holster_depth: f32,
    /// 壁厚 (mm、default 3.0、range 2-6)
    pub wall_thickness: f32,
}

impl Default for HairdryerHolderUiState {
    fn default() -> Self {
        Self {
            barrel_diameter: 85.0,
            holster_depth: 110.0,
            wall_thickness: 3.0,
        }
    }
}

impl HairdryerHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "hairdryer_holder({}, {}, {})",
            self.barrel_diameter, self.holster_depth, self.wall_thickness
        )
    }
}

/// K-Cup ホルダー customizer UI state (`kcup_holder(rows, cols, capsule_diameter)`)
#[derive(Debug, Clone, Copy)]
pub struct KcupHolderUiState {
    /// 行数 (default 3、range 1-6)
    pub rows: u32,
    /// 列数 (default 4、range 1-6)
    pub cols: u32,
    /// capsule 直径 (mm、K-Cup=53 / Nespresso=39 / Dolce Gusto=55、default 53、range 35-60)
    pub capsule_diameter: f32,
}

impl Default for KcupHolderUiState {
    fn default() -> Self {
        Self {
            rows: 3,
            cols: 4,
            capsule_diameter: 53.0,
        }
    }
}

impl KcupHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "kcup_holder({}, {}, {})",
            self.rows, self.cols, self.capsule_diameter
        )
    }
}

/// ヘックスキーホルダー customizer UI state
/// (`hex_key_holder(count, min_key_mm, max_key_mm)`)
#[derive(Debug, Clone, Copy)]
pub struct HexKeyHolderUiState {
    /// key 個数 (default 9、Metric standard、range 5-15)
    pub count: u32,
    /// 最小 key 幅 (mm、default 1.5、range 1.0-4.0)
    pub min_key_mm: f32,
    /// 最大 key 幅 (mm、default 10.0、range 6.0-15.0)
    pub max_key_mm: f32,
}

impl Default for HexKeyHolderUiState {
    fn default() -> Self {
        Self {
            count: 9,
            min_key_mm: 1.5,
            max_key_mm: 10.0,
        }
    }
}

impl HexKeyHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "hex_key_holder({}, {}, {})",
            self.count, self.min_key_mm, self.max_key_mm
        )
    }
}

/// wrap/foil ロールホルダー customizer UI state
/// (`wrap_holder(roll_diameter, roll_width, wall_thickness)`)
#[derive(Debug, Clone, Copy)]
pub struct WrapHolderUiState {
    /// roll 外径 (mm、standard=55、default 55、range 40-65)
    pub roll_diameter: f32,
    /// roll 幅 (mm、standard 12"=305、default 305、range 200-460)
    pub roll_width: f32,
    /// 壁厚 (mm、default 3.0、range 2-5)
    pub wall_thickness: f32,
}

impl Default for WrapHolderUiState {
    fn default() -> Self {
        Self {
            roll_diameter: 55.0,
            roll_width: 305.0,
            wall_thickness: 3.0,
        }
    }
}

impl WrapHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "wrap_holder({}, {}, {})",
            self.roll_diameter, self.roll_width, self.wall_thickness
        )
    }
}

/// 靴下 divider customizer UI state (`sock_divider(cell_count, cell_width, height)`)
#[derive(Debug, Clone, Copy)]
pub struct SockDividerUiState {
    /// cell 個数 (default 4、range 2-10)
    pub cell_count: u32,
    /// cell 幅 (mm、default 80、range 50-150)
    pub cell_width: f32,
    /// height (mm、drawer 高、default 89、range 50-120)
    pub height: f32,
}

impl Default for SockDividerUiState {
    fn default() -> Self {
        Self {
            cell_count: 4,
            cell_width: 80.0,
            height: 89.0,
        }
    }
}

impl SockDividerUiState {
    pub fn to_lol(self) -> String {
        format!(
            "sock_divider({}, {}, {})",
            self.cell_count, self.cell_width, self.height
        )
    }
}

/// 石鹸トレー customizer UI state
/// (`soap_tray(tray_length, tray_width, drain_slot_count)`)
#[derive(Debug, Clone, Copy)]
pub struct SoapTrayUiState {
    /// tray 内 長 (mm、default 200、range 100-300)
    pub tray_length: f32,
    /// tray 内 幅 (mm、default 90、range 60-150)
    pub tray_width: f32,
    /// drain slot 個数 (default 6、range 2-15)
    pub drain_slot_count: u32,
}

impl Default for SoapTrayUiState {
    fn default() -> Self {
        Self {
            tray_length: 200.0,
            tray_width: 90.0,
            drain_slot_count: 6,
        }
    }
}

impl SoapTrayUiState {
    pub fn to_lol(self) -> String {
        format!(
            "soap_tray({}, {}, {})",
            self.tray_length, self.tray_width, self.drain_slot_count
        )
    }
}

/// カミソリホルダー customizer UI state
/// (`razor_holder(slot_width, slot_depth, mount_hole_diameter)`)
#[derive(Debug, Clone, Copy)]
pub struct RazorHolderUiState {
    /// slot 幅 (mm、razor stem、default 12、range 8-16)
    pub slot_width: f32,
    /// slot 深さ (mm、default 22、range 15-30)
    pub slot_depth: f32,
    /// mount hole 直径 (mm、M4=4.5、default 4.5、range 3-6)
    pub mount_hole_diameter: f32,
}

impl Default for RazorHolderUiState {
    fn default() -> Self {
        Self {
            slot_width: 12.0,
            slot_depth: 22.0,
            mount_hole_diameter: 4.5,
        }
    }
}

impl RazorHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "razor_holder({}, {}, {})",
            self.slot_width, self.slot_depth, self.mount_hole_diameter
        )
    }
}

/// 箸ホルダー customizer UI state
/// (`chopstick_holder(pair_count, slot_width, slot_length)`)
#[derive(Debug, Clone, Copy)]
pub struct ChopstickHolderUiState {
    /// pair 個数 (default 4、range 2-10)
    pub pair_count: u32,
    /// slot 幅 (mm、pair 用 12-15、default 13、range 8-20)
    pub slot_width: f32,
    /// slot 長 (mm、default 260、range 200-330)
    pub slot_length: f32,
}

impl Default for ChopstickHolderUiState {
    fn default() -> Self {
        Self {
            pair_count: 4,
            slot_width: 13.0,
            slot_length: 260.0,
        }
    }
}

impl ChopstickHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "chopstick_holder({}, {}, {})",
            self.pair_count, self.slot_width, self.slot_length
        )
    }
}

/// フィラメントスウォッチホルダー customizer UI state
/// (`swatch_holder(rows, cols, swatch_width)`)
#[derive(Debug, Clone, Copy)]
pub struct SwatchHolderUiState {
    /// 行数 (default 8、range 2-20)
    pub rows: u32,
    /// 列数 (default 4、range 1-10)
    pub cols: u32,
    /// swatch 幅 (mm、standard=32 / small=24、default 32、range 20-60)
    pub swatch_width: f32,
}

impl Default for SwatchHolderUiState {
    fn default() -> Self {
        Self {
            rows: 8,
            cols: 4,
            swatch_width: 32.0,
        }
    }
}

impl SwatchHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "swatch_holder({}, {}, {})",
            self.rows, self.cols, self.swatch_width
        )
    }
}

// ── Sprint 15 ミックス 4 archetype UI state (tp / sd_card / driver) ──

/// トイレットペーパーホルダー UI state (bathroom § 7.6、wall-mount backplate + Z-axis axle)
/// (`tp_holder(inner_diameter, roll_width, wall_thickness)`)
#[derive(Debug, Clone, Copy)]
pub struct TpHolderUiState {
    /// ロール内径 (mm、standard=40、default 40、range 35-50)
    pub inner_diameter: f32,
    /// ロール幅 = 軸長 (mm、default 110、range 90-150)
    pub roll_width: f32,
    /// backplate 厚 (mm、default 5、range 3-10)
    pub wall_thickness: f32,
}

impl Default for TpHolderUiState {
    fn default() -> Self {
        Self {
            inner_diameter: 40.0,
            roll_width: 110.0,
            wall_thickness: 5.0,
        }
    }
}

impl TpHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "tp_holder({}, {}, {})",
            self.inner_diameter, self.roll_width, self.wall_thickness
        )
    }
}

/// SD カードホルダー UI state (printer § 9.4、2D grid narrow rect slots for SD cards)
/// (`sd_card_holder(rows, cols, card_width)`)
#[derive(Debug, Clone, Copy)]
pub struct SdCardHolderUiState {
    /// 行数 (default 4、range 2-8)
    pub rows: u32,
    /// 列数 (default 4、range 2-8)
    pub cols: u32,
    /// カード幅 (mm、SD full=24 / microSD=15、default 24、range 12-30)
    pub card_width: f32,
}

impl Default for SdCardHolderUiState {
    fn default() -> Self {
        Self {
            rows: 4,
            cols: 4,
            card_width: 24.0,
        }
    }
}

impl SdCardHolderUiState {
    pub fn to_lol(self) -> String {
        format!(
            "sd_card_holder({}, {}, {})",
            self.rows, self.cols, self.card_width
        )
    }
}

/// ドライバーラック UI state (garage § 8.5、row 状 large cyl hole for screwdriver handles)
/// (`driver_rack(slot_count, slot_diameter, height)`)
#[derive(Debug, Clone, Copy)]
pub struct DriverRackUiState {
    /// slot 個数 (default 8、range 4-16)
    pub slot_count: u32,
    /// slot 直径 (mm、handle 用、default 25、range 15-40)
    pub slot_diameter: f32,
    /// ラック高さ (mm、default 100、range 60-150)
    pub height: f32,
}

impl Default for DriverRackUiState {
    fn default() -> Self {
        Self {
            slot_count: 8,
            slot_diameter: 25.0,
            height: 100.0,
        }
    }
}

impl DriverRackUiState {
    pub fn to_lol(self) -> String {
        format!(
            "driver_rack({}, {}, {})",
            self.slot_count, self.slot_diameter, self.height
        )
    }
}

// ── Sprint 16 ミックス 5 archetype UI state (cotton / sink / clamp) ──

/// 綿棒/コットン ディスペンサー UI state (bathroom § 7.4、open top cyl + inner cavity)
/// (`cotton_dispenser(count, inner_diameter, height)`)
#[derive(Debug, Clone, Copy)]
pub struct CottonDispenserUiState {
    /// 収容目安個数 (informational、SDF に非反映、default 80、range 20-200)
    pub count: u32,
    /// cavity 内径 (mm、default 90、range 60-120)
    pub inner_diameter: f32,
    /// 全高 (mm、default 100、range 60-150)
    pub height: f32,
}

impl Default for CottonDispenserUiState {
    fn default() -> Self {
        Self {
            count: 80,
            inner_diameter: 90.0,
            height: 100.0,
        }
    }
}

impl CottonDispenserUiState {
    pub fn to_lol(self) -> String {
        format!(
            "cotton_dispenser({}, {}, {})",
            self.count, self.inner_diameter, self.height
        )
    }
}

/// スポンジホルダー UI state (kitchen § 6.9、drain hole 付き rect tray)
/// (`sink_caddy(tray_length, tray_width, drain_hole_count)`)
#[derive(Debug, Clone, Copy)]
pub struct SinkCaddyUiState {
    /// tray 内 長 (mm、default 200、range 150-300)
    pub tray_length: f32,
    /// tray 内 幅 (mm、default 100、range 80-150)
    pub tray_width: f32,
    /// drain hole 個数 (default 8、range 4-16)
    pub drain_hole_count: u32,
}

impl Default for SinkCaddyUiState {
    fn default() -> Self {
        Self {
            tray_length: 200.0,
            tray_width: 100.0,
            drain_hole_count: 8,
        }
    }
}

impl SinkCaddyUiState {
    pub fn to_lol(self) -> String {
        format!(
            "sink_caddy({}, {}, {})",
            self.tray_length, self.tray_width, self.drain_hole_count
        )
    }
}

/// クランプ壁掛けラック UI state (garage § 8.8、row 状 hook + backplate)
/// (`clamp_rack(hook_count, hook_width, height)`)
#[derive(Debug, Clone, Copy)]
pub struct ClampRackUiState {
    /// hook 個数 (default 5、range 2-10)
    pub hook_count: u32,
    /// 各 hook 幅 (mm、default 30、range 20-60)
    pub hook_width: f32,
    /// 全高 = backplate 高さ (mm、default 150、range 100-300)
    pub height: f32,
}

impl Default for ClampRackUiState {
    fn default() -> Self {
        Self {
            hook_count: 5,
            hook_width: 30.0,
            height: 150.0,
        }
    }
}

impl ClampRackUiState {
    pub fn to_lol(self) -> String {
        format!(
            "clamp_rack({}, {}, {})",
            self.hook_count, self.hook_width, self.height
        )
    }
}

pub struct AppState {
    #[allow(dead_code)]
    pub data_dir: PathBuf,
    pub tier: Tier,
    pub llm_config: LlmConfig,
    pub prompt_input: String,
    pub generation_status: GenerationStatus,
    pub current_lol: Option<String>,
    /// Latest mesh built by the pipeline, ready for the on-screen preview
    ///
    /// Populated in `prompt.rs` right after `pipeline::export_mesh` returns
    /// success The `Arc` lets the mesh cross the async task boundary
    /// without copying vertices
    pub viewer_mesh: Option<std::sync::Arc<alice_sdf::mesh::Mesh>>,
    /// Monotonic version counter that increments each time `viewer_mesh` is
    /// replaced The mesh preview UI compares this against its own
    /// last-uploaded version to decide whether to re-push vertices to the
    /// GPU
    pub mesh_version: u64,
    /// General tier: 公開待ちの SDF (id, lol_source, prompt)
    pub pending_publish: Option<(String, String, String)>,
    pub history: Vec<GenerationRow>,
    pub db: Database,
    pub profile_id: String,
    pub runtime: tokio::runtime::Runtime,
    pub result_rx: mpsc::Receiver<GenerationMessage>,
    pub result_tx: mpsc::Sender<GenerationMessage>,
    pub model_progress:
        tokio::sync::watch::Receiver<text_to_print_llm::downloader::DownloadProgress>,
    pub model_ready: bool,
    pub phase_progress: PhaseProgress,
    /// Set true once egui focus has been requested for the prompt field.
    pub prompt_focused_once: bool,
    /// `alice-llm-server` sidecar プロセスの状態
    ///
    /// - `Waiting`: model DL 完了待ち
    /// - `Starting`: `alice-llm-server` を spawn 発行済、`/health` 応答待ち
    /// - `Running`: 推論 request 受付可
    /// - `Error(msg)`: バイナリ不在 / model 不在 / health timeout 等
    ///
    /// spawn task 本体は `AppState::new` の runtime 上で動作し、
    /// `SidecarProcess` を task スコープに保持することで runtime drop 時に
    /// 自動 kill される
    pub sidecar_status: tokio::sync::watch::Receiver<SidecarStatus>,
    /// Stage 3-C.6: user-selected inference backend (Sidecar HTTP or
    /// in-process Embedded) Persisted in DB (`profiles.backend_kind`)
    /// The default is `Sidecar` so existing installs keep the pre-3-C
    /// behaviour without a migration step
    pub backend_kind: BackendKind,
    /// Stage 3-C.12: whether the Embedded backend runs on CPU or GPU
    /// Only meaningful when [`Self::backend_kind`] is
    /// [`BackendKind::Embedded`] Persisted in DB
    /// (`profiles.execution_mode`) Default `Cpu`
    pub execution_mode: ExecutionMode,
    /// Loaded [`EmbeddedBackend`] wrapped for shared access The slot is
    /// `None` until the user opts into Embedded and the background load
    /// task populates it via [`AppState::switch_backend_kind`]
    pub embedded: std::sync::Arc<std::sync::Mutex<Option<EmbeddedBackend>>>,
    /// UI-facing load status Read via [`AppState::embedded_status`] The
    /// underlying `Arc<Mutex<EmbeddedStatus>>` is written by the load
    /// task in [`AppState::switch_backend_kind`]
    pub embedded_status_cell: std::sync::Arc<std::sync::Mutex<EmbeddedStatus>>,
    /// BYO LLM (2026-08-23): remote OpenAI-compat provider slot The
    /// value is `None` until the user configures a provider in Settings
    /// and enters a valid API key (from OS Keychain via
    /// [`text_to_print_core::keychain`]) Selection is persisted in DB
    /// (`profiles.openai_compat_active_provider`) and full per-provider
    /// config lives in `llm_provider_configs` table
    pub openai_compat: std::sync::Arc<
        std::sync::Mutex<Option<text_to_print_llm::openai_compat_backend::OpenAiCompatBackend>>,
    >,
    /// BYO LLM (2026-08-23): user-supplied GGUF path that overrides the
    /// download path when Embedded is active `None` = use ModelChoice
    /// default filename in `models_dir` Persisted in DB
    /// (`profiles.custom_gguf_path`)
    pub custom_gguf_path: Option<PathBuf>,
    /// LoRA share opt-in flag (Stage 5 T5.2) When `true` (default) the
    /// LOL DSL + quality signals are queued for upload to the shared LoRA
    /// training set; when `false` the user has opted out
    pub share_lol_dsl: bool,
    /// Stage 3-C.14: when `true` (default), every generation forwards
    /// [`text_to_print_llm::grammar_lol::LOL_GBNF`] to the backend so
    /// output is guaranteed to be parseable by
    /// `alice_bamboo::parse_lol` Turn off for debugging free-form output
    pub enforce_lol_grammar: bool,
    /// Most recent share-payload dry-run path (GAP-12) Populated when the
    /// generation success path serialises a `SharePayload` to
    /// `data_dir/share_dry_run/{uuid}.json` The real Cloudflare Workers
    /// upload (Epic-Infra #35) will consume the same payload; the dry-run
    /// path keeps the opt-in gate honest even when the backend is offline
    pub pending_share_dry_run: Option<std::path::PathBuf>,
    /// Template customizer UI state (Gridfinity bin 等の param 入力保持)
    pub customizer_state: CustomizerState,
    /// Sprint X.1: archetype preset library snapshot (Layer 1 sync)
    ///
    /// 起動時に (1) local cache → (2) bundled default の順で初期化、その後
    /// background で Cloudflare Worker `GET /api/presets` を fetch し
    /// 成功時に上書き `prompt.rs::show_prompt_templates` は本 field を
    /// dynamic に読み出して TEMPLATE_CATEGORIES const の代替とする
    /// 詳細: memory/project_text_to_print_archetype_library_architecture.md
    pub presets: PresetsSnapshot,
}

/// Preset library の in-memory snapshot、by-startup / by-cloud-sync で更新
#[derive(Debug, Clone)]
pub struct PresetsSnapshot {
    pub source: PresetsSource,
    pub version: String,
    pub categories: Vec<PresetCategory>,
}

/// Snapshot がどこから来たか (debug + UI status 表示用)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetsSource {
    /// Compile-time bundled JSON (`BUNDLED_DEFAULT_PRESETS_JSON`)、初回起動 or offline
    Bundled,
    /// Local SQLite `presets_cache` (前回起動時に sync 済み)
    Cache,
    /// 起動直後の background sync で Cloudflare から fresh 取得
    Cloud,
}

impl PresetsSnapshot {
    /// 起動時初期化: cache → bundled の順に fallback
    ///
    /// 常に成功 (bundled 必ず parse 通る前提、`BUNDLED_DEFAULT_PRESETS_JSON` は
    /// compile-time embed + 単体 test で parse 保証済み)
    #[must_use]
    pub fn load_initial(db: &Database) -> Self {
        // 1. local cache 試行
        if let Ok(Some((json, _etag))) = db.get_presets_cache()
            && let Ok(parsed) = parse_presets_json(&json)
        {
            return Self::from_response(parsed, PresetsSource::Cache);
        }
        // 2. bundled fallback (parse 失敗は panic、bundled は compile-time verified)
        let parsed = parse_presets_json(BUNDLED_DEFAULT_PRESETS_JSON)
            .expect("bundled default presets must be valid JSON");
        Self::from_response(parsed, PresetsSource::Bundled)
    }

    fn from_response(resp: PresetsResponse, source: PresetsSource) -> Self {
        Self {
            source,
            version: resp.version,
            categories: resp.categories,
        }
    }
}

pub enum GenerationStatus {
    Idle,
    Generating,
    Done {
        lol_source: String,
        // Boxed because `MeshStats` is ~200 B and this variant is the largest
        // enum arm by an order of magnitude (`clippy::large_enum_variant`)
        mesh_stats: Option<Box<MeshStats>>,
    },
    Error(String),
}

pub enum GenerationMessage {
    PhaseStart(GenerationPhase),
    PhaseDone(GenerationPhase, Duration),
    Success {
        id: String,
        lol_source: String,
        /// Boxed for the same reason as [`GenerationStatus::Done::mesh_stats`]
        mesh_stats: Option<Box<MeshStats>>,
        /// Number of LLM retries performed during this generation Non-zero
        /// values mean the initial LOL DSL raised safety violations and the
        /// [`text_to_print_llm::backend::generate_with_retry`] loop
        /// requested a revised DSL via `fix_prompt` (Stage 8 T8.2)
        retry_count: u32,
        /// Path to the dry-run share payload JSON (GAP-12) Populated when
        /// `share_lol_dsl` is on and the generation produced a 3MF export;
        /// otherwise `None`
        share_dry_run: Option<std::path::PathBuf>,
    },
    Failure {
        id: String,
        error: String,
    },
    /// Sprint X.1: background preset sync 完了通知
    ///
    /// Cloudflare Worker `GET /api/presets` fetch 成功時に emit、UI 側は
    /// `poll_results` で受け取って `AppState::presets` を上書き 304 (未変更) or
    /// error は特に message emit しない (silent、次回 startup で cache 経由復元)
    PresetsUpdated(Box<PresetsSnapshot>),
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Self {
        let db_path = data_dir.join("text-to-print.db");
        let db = Database::open(&db_path).expect("failed to open database");

        let profile_id = load_or_create_profile_id(&data_dir);
        let tier = db.get_or_create_profile(&profile_id).unwrap_or(Tier::Free);
        let share_lol_dsl = db.get_share_lol_dsl(&profile_id).unwrap_or(true);
        // v0.1.0-beta.1 (2026-08-07): default を Sidecar → Embedded に変更
        // sidecar は alice-llm-server binary の別途 install を必要とする
        // (release.yml は bundle 済だが local `cargo run` では欠落) →
        // 初回起動 UX が壊れる Embedded は alice-llm を rlib 直リンクなので
        // binary 追加なしで動く GGUF DL は既存 downloader flow で自動化済
        let backend_kind = BackendKind::from_db_str(
            &db.get_backend_kind(&profile_id)
                .unwrap_or_else(|_| "Embedded".to_string()),
        );
        let execution_mode = ExecutionMode::from_db_str(
            &db.get_execution_mode(&profile_id)
                .unwrap_or_else(|_| "Cpu".to_string()),
        );
        // v0.1.0-beta.1 (2026-08-07): default を true → false に変更
        // (詳細は db.rs::get_enforce_lol_grammar コメント参照)
        let enforce_lol_grammar = db.get_enforce_lol_grammar(&profile_id).unwrap_or(false);
        // BYO LLM (2026-08-23): user-supplied GGUF override Empty string
        // from DB → None (falls back to ModelChoice default filename)
        let custom_gguf_path = db
            .get_custom_gguf_path(&profile_id)
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from);

        let history = db.list_generations(&profile_id, 50).unwrap_or_default();

        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("failed to create tokio runtime");

        let (result_tx, result_rx) = mpsc::channel();

        // モデルチェック + バックグラウンドダウンロード
        // Stage 3-C.9: model_choice に対応する GGUF を保持 起動時は
        // LlmConfig::default() (= Qwen 3.5-4B) を採用、UI で dropdown 変更
        // → Embedded 有効時は on_model_choice_changed が呼ばれ切替
        let models_dir = data_dir.join("models");
        let initial_choice = LlmConfig::default().model_choice;
        let model_ready = text_to_print_llm::downloader::model_exists(&models_dir, initial_choice);
        let initial_status = if model_ready {
            text_to_print_llm::downloader::DownloadStatus::Complete
        } else {
            text_to_print_llm::downloader::DownloadStatus::Pending
        };
        let (progress_tx, progress_rx) =
            tokio::sync::watch::channel(text_to_print_llm::downloader::DownloadProgress {
                downloaded_bytes: 0,
                total_bytes: None,
                status: initial_status,
            });

        if !model_ready {
            let md = models_dir.clone();
            runtime.spawn(async move {
                if let Err(e) =
                    text_to_print_llm::downloader::download_model(&md, initial_choice, progress_tx)
                        .await
                {
                    tracing::error!(error = %e, "model download failed");
                }
            });
        }

        // sidecar auto-spawn: model DL 完了を待ってから `alice-llm-server` を起動
        // 2026-08-07: preferred port 8000 は user 環境で Python HTTP server 等が
        // 良く塞ぐため、事前 free port scan 済 chosen port は LlmConfig の
        // endpoint も同期更新 client → sidecar 疎通が確実になる
        // 2026-08-23: preferred port は Settings → Network から user 変更可
        //   優先度: TTP_SIDECAR_PORT env → DB profiles.sidecar_port → default 8000
        let (sidecar_tx, sidecar_rx) = tokio::sync::watch::channel(SidecarStatus::Waiting);
        let default_llm_config = LlmConfig::default();
        let preferred_port = std::env::var("TTP_SIDECAR_PORT")
            .ok()
            .and_then(|s| s.parse::<u16>().ok())
            .or_else(|| db.get_sidecar_port(&profile_id).ok())
            .unwrap_or_else(|| default_sidecar_port(&default_llm_config.endpoint));
        let chosen_port =
            text_to_print_llm::sidecar::find_free_port_starting_at(preferred_port, 16)
                .unwrap_or(preferred_port);
        let llm_config = if chosen_port == preferred_port {
            default_llm_config
        } else {
            tracing::info!(
                preferred = preferred_port,
                chosen = chosen_port,
                "sidecar preferred port in use, endpoint updated"
            );
            LlmConfig {
                endpoint: format!("http://localhost:{chosen_port}/v1/chat/completions"),
                ..default_llm_config
            }
        };
        {
            let models_dir_for_sidecar = models_dir.clone();
            let mut model_progress_rx = progress_rx.clone();
            runtime.spawn(async move {
                // model DL の完了を待つ (model_ready なら DownloadStatus::Complete で初期化済)
                while !matches!(
                    model_progress_rx.borrow().status,
                    text_to_print_llm::downloader::DownloadStatus::Complete
                ) {
                    if model_progress_rx.changed().await.is_err() {
                        // sender drop = AppState 破棄、task 終了
                        return;
                    }
                }
                let model_path = text_to_print_llm::downloader::model_path(
                    &models_dir_for_sidecar,
                    initial_choice,
                );
                text_to_print_llm::sidecar::run_auto_spawn(model_path, chosen_port, sidecar_tx)
                    .await;
            });
        }
        // Sidecar warm-up: healthy 検知後に background で dummy 1-token
        // request 送出 Metal shader compile + buffer alloc が初回のみ
        // ~50-100s 走る問題を、user が prompt 打ち込む前に済ませておく
        // fire-and-forget、失敗は log のみ (実 request 時に retry される)
        {
            let mut sidecar_rx_for_warmup = sidecar_rx.clone();
            let endpoint = llm_config.endpoint.clone();
            let model_id = llm_config.model_choice.model_id().to_string();
            runtime.spawn(async move {
                // Running になるまで待つ
                loop {
                    if sidecar_rx_for_warmup.borrow().is_running() {
                        break;
                    }
                    if sidecar_rx_for_warmup.changed().await.is_err() {
                        return;
                    }
                }
                tracing::info!("sidecar warm-up starting (background dummy request)");
                let started = std::time::Instant::now();
                let client = reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(300))
                    .build()
                    .ok();
                if let Some(client) = client {
                    // 現実的な payload で warm-up: max_tokens=1 だと Metal
                    // shader の一部 pipeline が compile されず初回本番 request
                    // で遅延する (user 実測 warm-up 2s 後の本 request 155s)
                    // 対策: 実 LOL DSL 生成に近い payload (short prompt +
                    // max_tokens 50) で warm-up、shader の generation path
                    // まで compile 済にする
                    let body = serde_json::json!({
                        "model": model_id,
                        "messages": [{
                            "role": "user",
                            "content": "Output a 10mm sphere in LOL DSL: sphere(5)"
                        }],
                        "max_tokens": 50,
                        "temperature": 0.7,
                    });
                    match client.post(&endpoint).json(&body).send().await {
                        Ok(resp) => tracing::info!(
                            elapsed_ms = started.elapsed().as_millis() as u64,
                            status = %resp.status(),
                            "sidecar warm-up complete (realistic payload)"
                        ),
                        Err(e) => tracing::warn!("sidecar warm-up failed: {e}"),
                    }
                }
            });
        }

        // Stage 5: real share upload sweep — retry any queued payloads
        // from prior sessions Delayed 5s to let the sidecar + UI settle
        // before hitting the network Runs once per launch; the offline
        // queue keeps payloads for 24 h (`QUEUE_TTL`) so a subsequent
        // launch drains anything that didn't land this time
        {
            let queue_dir = data_dir.join("share_queue");
            runtime.spawn(async move {
                tokio::time::sleep(Duration::from_secs(5)).await;
                let cfg = text_to_print_network::share::ShareConfig::default();
                match text_to_print_network::share::retry_queued_uploads(&cfg, &queue_dir).await {
                    Ok(summary) => tracing::info!(
                        delivered = summary.delivered,
                        still_pending = summary.still_pending,
                        rejected = summary.rejected_permanently,
                        expired = summary.expired,
                        "share queue sweep complete"
                    ),
                    Err(e) => tracing::warn!(error = %e, "share queue sweep failed"),
                }
            });
        }

        // Stage 3-C.6: initial embedded slot Empty until the user opts
        // into Embedded via the settings UI Kicking off the load here
        // (even when backend_kind == Embedded from DB) would block app
        // startup for tens of seconds; instead we surface the toggle in
        // Settings and let the load begin only when the user asks
        let embedded = std::sync::Arc::new(std::sync::Mutex::new(None));
        let embedded_status_cell =
            std::sync::Arc::new(std::sync::Mutex::new(EmbeddedStatus::NotLoaded));

        // If the persisted preference is Embedded, prime the load in the
        // background so the first generation doesn't pay the ~30 s load
        // cost synchronously The sidecar remains available in the mean
        // time as a fallback
        if backend_kind == BackendKind::Embedded {
            spawn_embedded_load(
                &runtime,
                models_dir.clone(),
                initial_choice,
                execution_mode,
                custom_gguf_path.clone(),
                embedded.clone(),
                embedded_status_cell.clone(),
                progress_rx.clone(),
            );
        }

        // Sprint X.1: preset library の初期化 (cache → bundled fallback)
        // `db` を struct 化する前に load、以降 background で Cloudflare fetch
        let presets = PresetsSnapshot::load_initial(&db);

        // Startup background preset sync (Cloudflare Worker から latest fetch)
        // 失敗しても initial cache/bundled で動く、fetch 成功時に UI 更新
        //
        // 2026-08-23 Settings → Network 対応:
        //   - Sync 有効判定: TTP_PRESETS_SYNC_DISABLE env で 1 個 → skip 強制、
        //     env 未設定なら DB profiles.presets_sync_enabled 参照 (default true)
        //   - Endpoint 解決: TTP_PRESETS_ENDPOINT env > DB presets_endpoint > default
        //     (PresetsClient::resolve_endpoint に集約)
        //   - Sync disable 時は log で明示、bundled / cache だけで動作継続
        let sync_disabled_env = std::env::var("TTP_PRESETS_SYNC_DISABLE")
            .ok()
            .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true"));
        let sync_enabled_db = db.get_presets_sync_enabled(&profile_id).unwrap_or(true);
        if sync_disabled_env || !sync_enabled_db {
            tracing::info!(
                env_disabled = sync_disabled_env,
                db_enabled = sync_enabled_db,
                "presets sync disabled (settings), using bundled/cache only"
            );
        } else {
            let db_endpoint = db.get_presets_endpoint(&profile_id).unwrap_or_default();
            let endpoint = text_to_print_network::presets_client::PresetsClient::resolve_endpoint(
                Some(&db_endpoint),
            );
            spawn_presets_sync(&runtime, db_path.clone(), endpoint, result_tx.clone());
        }

        Self {
            data_dir,
            tier,
            llm_config,
            prompt_input: String::new(),
            generation_status: GenerationStatus::Idle,
            current_lol: None,
            viewer_mesh: None,
            mesh_version: 0,
            pending_publish: None,
            history,
            db,
            profile_id,
            runtime,
            result_rx,
            result_tx,
            model_progress: progress_rx,
            model_ready,
            phase_progress: PhaseProgress::default(),
            prompt_focused_once: false,
            sidecar_status: sidecar_rx,
            backend_kind,
            execution_mode,
            embedded,
            embedded_status_cell,
            // BYO LLM (2026-08-23): slot stays empty at startup until
            // the user configures a provider + API key in Settings The
            // Settings UI (step 3) populates it via a helper that reads
            // llm_provider_configs + Keychain and constructs
            // OpenAiCompatBackend
            openai_compat: std::sync::Arc::new(std::sync::Mutex::new(None)),
            custom_gguf_path,
            share_lol_dsl,
            enforce_lol_grammar,
            pending_share_dry_run: None,
            customizer_state: CustomizerState::default(),
            presets,
        }
    }

    pub fn refresh_history(&mut self) {
        self.history = self
            .db
            .list_generations(&self.profile_id, 50)
            .unwrap_or_default();
    }

    pub fn today(&self) -> String {
        chrono::Local::now().format("%Y-%m-%d").to_string()
    }

    pub fn daily_usage(&self) -> u32 {
        self.db
            .get_daily_usage(&self.profile_id, &self.today())
            .unwrap_or(0)
    }

    pub fn can_generate(&self) -> bool {
        self.daily_usage() < self.tier.limits().daily_generations
    }

    /// Directory used by the share dry-run gate (GAP-12) The generation
    /// success path writes `SharePayload` JSON blobs here whenever the
    /// user has opted in and produced a mesh; the Cloudflare Workers
    /// backend (Epic-Infra #35) will drain the queue later
    pub fn share_dry_run_dir(&self) -> std::path::PathBuf {
        self.data_dir.join("share_dry_run")
    }

    /// Directory used by the real upload queue (Stage 5) `enqueue` writes
    /// `SharePayload` JSON blobs here that `retry_queued_uploads` drains
    /// on app startup / periodic sweep against the Cloudflare Worker
    /// endpoint Distinct from `share_dry_run_dir` so operators can inspect
    /// the dry-run corpus without touching live queue state
    pub fn share_queue_dir(&self) -> std::path::PathBuf {
        self.data_dir.join("share_queue")
    }

    /// Effective share flag Combines the user's opt-in toggle with the
    /// tier gate: paid tiers (General / Pro / Enterprise) never share
    /// regardless of the toggle Free tier respects the toggle
    ///
    /// This is the single value the generation path should consult before
    /// dumping / enqueueing a `SharePayload` The settings UI can still
    /// surface the raw toggle to communicate opt-in state
    pub fn share_effective_enabled(&self) -> bool {
        match self.tier {
            Tier::Free => self.share_lol_dsl,
            Tier::General | Tier::Pro | Tier::Enterprise => false,
        }
    }

    /// Snapshot the current [`EmbeddedStatus`] for UI display Cheap `Clone`
    /// under a `std::sync::Mutex` so egui's sync render path can call it
    #[must_use]
    pub fn embedded_status(&self) -> EmbeddedStatus {
        self.embedded_status_cell
            .lock()
            .map(|g| g.clone())
            .unwrap_or(EmbeddedStatus::NotLoaded)
    }

    /// Persist a new [`BackendKind`] and, if switching to Embedded, kick
    /// off a background load task Idempotent on the same kind
    pub fn switch_backend_kind(&mut self, new_kind: BackendKind) {
        if self.backend_kind == new_kind {
            return;
        }
        self.backend_kind = new_kind;
        if let Err(e) = self
            .db
            .set_backend_kind(&self.profile_id, new_kind.to_db_str())
        {
            tracing::warn!(error = %e, "failed to persist backend_kind");
        }
        if new_kind == BackendKind::Embedded {
            let already_loaded = self.embedded.lock().map(|g| g.is_some()).unwrap_or(false);
            if already_loaded {
                return;
            }
            spawn_embedded_load(
                &self.runtime,
                self.data_dir.join("models"),
                self.llm_config.model_choice,
                self.execution_mode,
                self.custom_gguf_path.clone(),
                self.embedded.clone(),
                self.embedded_status_cell.clone(),
                self.model_progress.clone(),
            );
        }
    }

    /// Stage 3-C.12: change the Embedded execution mode (CPU ↔ GPU) at
    /// runtime If Embedded is currently active, drops the loaded model
    /// and re-loads under the new mode
    pub fn switch_execution_mode(&mut self, new_mode: ExecutionMode) {
        if self.execution_mode == new_mode {
            return;
        }
        self.execution_mode = new_mode;
        if let Err(e) = self
            .db
            .set_execution_mode(&self.profile_id, new_mode.to_db_str())
        {
            tracing::warn!(error = %e, "failed to persist execution_mode");
        }
        // Only reload if Embedded is the active backend Sidecar path
        // ignores execution_mode entirely (server subprocess decides its
        // own CPU / GPU via its own --hybrid flag)
        if self.backend_kind != BackendKind::Embedded {
            return;
        }
        if let Ok(mut slot) = self.embedded.lock() {
            *slot = None;
        }
        if let Ok(mut status) = self.embedded_status_cell.lock() {
            *status = EmbeddedStatus::NotLoaded;
        }
        spawn_embedded_load(
            &self.runtime,
            self.data_dir.join("models"),
            self.llm_config.model_choice,
            new_mode,
            self.custom_gguf_path.clone(),
            self.embedded.clone(),
            self.embedded_status_cell.clone(),
            self.model_progress.clone(),
        );
    }

    /// Stage 3-C.9: handle a `ModelChoice` change from the Settings UI
    /// Called after the ComboBox has already updated
    /// `self.llm_config.model_choice` to the newly selected value
    ///
    /// If Embedded is active, drop the currently loaded backend and kick
    /// off a load for the new choice The Sidecar path is unaffected — the
    /// alice-llm-server subprocess reads its `--model` arg once at spawn
    /// time and would need a full sidecar restart to swap models (out of
    /// scope for this hook)
    ///
    /// Stage 3-C.13: If the currently loaded backend already tracks the
    /// new choice (e.g. the user flipped through the dropdown and landed
    /// back on the loaded choice), skip the drop + reload cycle — a
    /// wasted ~30 s and duplicate memory pressure
    pub fn on_model_choice_changed(&mut self, prev_choice: text_to_print_llm::model::ModelChoice) {
        let new_choice = self.llm_config.model_choice;
        if prev_choice == new_choice {
            return;
        }
        if self.backend_kind != BackendKind::Embedded {
            return;
        }
        let already_loaded = self
            .embedded
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|b| b.loaded_choice()))
            == Some(new_choice);
        if already_loaded {
            tracing::info!(
                ?new_choice,
                "embedded backend already loaded with target choice, skipping swap"
            );
            return;
        }
        tracing::info!(
            prev = ?prev_choice,
            new = ?new_choice,
            "embedded backend swap on model choice change"
        );
        // Drop the currently loaded backend so the worker thread's stack
        // unwinds and mmap / gguf / model release their memory before we
        // begin loading the new file (avoids a 2× resident-set spike)
        if let Ok(mut slot) = self.embedded.lock() {
            *slot = None;
        }
        if let Ok(mut status) = self.embedded_status_cell.lock() {
            *status = EmbeddedStatus::NotLoaded;
        }
        spawn_embedded_load(
            &self.runtime,
            self.data_dir.join("models"),
            new_choice,
            self.execution_mode,
            self.custom_gguf_path.clone(),
            self.embedded.clone(),
            self.embedded_status_cell.clone(),
            self.model_progress.clone(),
        );
    }

    /// BYO LLM (2026-08-23): swap the custom GGUF override at runtime
    /// Persists the new path to DB, updates the in-memory field, and if
    /// Embedded is the active backend drops the currently loaded model
    /// and kicks off a fresh load from the new path Pass `None` to
    /// clear the override (revert to ModelChoice download path)
    pub fn set_custom_gguf_path(&mut self, new_path: Option<PathBuf>) {
        let db_value = new_path
            .as_ref()
            .and_then(|p| p.to_str())
            .unwrap_or_default();
        if let Err(e) = self.db.set_custom_gguf_path(&self.profile_id, db_value) {
            tracing::warn!(error = %e, "failed to persist custom_gguf_path");
        }
        self.custom_gguf_path = new_path;
        // Only reload if Embedded is the active backend — Sidecar /
        // OpenAiCompat backends don't consume GGUF files
        if self.backend_kind != BackendKind::Embedded {
            return;
        }
        if let Ok(mut slot) = self.embedded.lock() {
            *slot = None;
        }
        if let Ok(mut status) = self.embedded_status_cell.lock() {
            *status = EmbeddedStatus::NotLoaded;
        }
        spawn_embedded_load(
            &self.runtime,
            self.data_dir.join("models"),
            self.llm_config.model_choice,
            self.execution_mode,
            self.custom_gguf_path.clone(),
            self.embedded.clone(),
            self.embedded_status_cell.clone(),
            self.model_progress.clone(),
        );
    }

    /// Snapshot the currently-active backend for a generation request
    ///
    /// - `BackendKind::Sidecar` → wraps the current `LlmConfig`
    /// - `BackendKind::Embedded` → clones the loaded backend if `Ready`,
    ///   otherwise falls back to `Sidecar` (so a generation request
    ///   during Embedded load doesn't fail silently)
    /// - `BackendKind::OpenAiCompat` → clones the loaded remote backend
    ///   if configured, otherwise falls back to `Sidecar` so an
    ///   unconfigured provider doesn't hang generation State plumbing
    ///   for the OpenAI-compat slot lives in [`Self::openai_compat`]
    #[must_use]
    pub fn active_backend(&self) -> text_to_print_llm::backend_kind::LlmBackend {
        use text_to_print_llm::backend_kind::LlmBackend;
        match self.backend_kind {
            BackendKind::Sidecar => LlmBackend::Sidecar(self.llm_config.clone()),
            BackendKind::Embedded => {
                let backend = self.embedded.lock().ok().and_then(|g| g.clone());
                match backend {
                    Some(b) => LlmBackend::Embedded(b),
                    None => LlmBackend::Sidecar(self.llm_config.clone()),
                }
            }
            BackendKind::OpenAiCompat => {
                let backend = self.openai_compat.lock().ok().and_then(|g| g.clone());
                match backend {
                    Some(b) => LlmBackend::OpenAiCompat(b),
                    None => LlmBackend::Sidecar(self.llm_config.clone()),
                }
            }
        }
    }
}

/// `http://host:PORT/...` の PORT を抽出、失敗時は 8000
fn default_sidecar_port(endpoint: &str) -> u16 {
    endpoint
        .split("://")
        .nth(1)
        .and_then(|s| s.split('/').next())
        .and_then(|host_port| host_port.rsplit(':').next())
        .and_then(|p| p.parse().ok())
        .unwrap_or(8000)
}

fn load_or_create_profile_id(data_dir: &std::path::Path) -> String {
    let id_path = data_dir.join("profile_id");
    if let Ok(id) = std::fs::read_to_string(&id_path) {
        let id = id.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = std::fs::write(&id_path, &id);
    id
}

/// Stage 3-C.9: spawn a background task that waits for model DL to
/// finish, then loads the specified `ModelChoice` into `embedded_slot`
/// and reflects state through `status_slot`
///
/// Extracted so both the app startup path (when the persisted
/// `backend_kind` is Embedded) and the runtime switch paths
/// (`switch_backend_kind` / `on_model_choice_changed`) share the same
/// wait-for-DL-then-load flow
/// Sprint X.1: startup 時に Cloudflare Worker から preset を background fetch
///
/// 動作:
/// 1. `db.get_presets_cache()` から前回の etag を取り出す
/// 2. `PresetsClient::fetch(etag)` を呼び、200 なら DB cache 更新 + UI に PresetsUpdated 通知
/// 3. 304 なら silent no-op (cache 継続)
/// 4. network / server error なら silent no-op (bundled or cache で app は動く)
///
/// UI thread は blocking 一切なし、fetch 失敗しても launch 遅延 0
fn spawn_presets_sync(
    runtime: &tokio::runtime::Runtime,
    db_path: PathBuf,
    endpoint: String,
    tx: mpsc::Sender<GenerationMessage>,
) {
    runtime.spawn(async move {
        // fetch 前に current etag を DB から読む (別 thread から DB access するため再 open)
        let current_etag: Option<String> = match Database::open(&db_path) {
            Ok(db) => db
                .get_presets_cache()
                .ok()
                .flatten()
                .and_then(|(_json, etag)| etag),
            Err(_) => None,
        };

        tracing::info!(
            endpoint = %endpoint,
            etag = ?current_etag,
            "presets sync starting"
        );

        let start = std::time::Instant::now();
        let client = text_to_print_network::presets_client::PresetsClient::new(endpoint.clone());
        match client.fetch(current_etag.as_deref()).await {
            Ok(text_to_print_network::presets_client::FetchResult::Updated { body, etag }) => {
                // Serialize body back to JSON for DB storage (canonical round-trip)
                if let Ok(json) = serde_json::to_string(&body)
                    && let Ok(db) = Database::open(&db_path)
                {
                    let _ = db.set_presets_cache(&body.version, etag.as_deref(), &json);
                }
                let snapshot = PresetsSnapshot {
                    source: PresetsSource::Cloud,
                    version: body.version.clone(),
                    categories: body.categories,
                };
                let _ = tx.send(GenerationMessage::PresetsUpdated(Box::new(snapshot)));
                tracing::info!(
                    version = %body.version,
                    elapsed_ms = %start.elapsed().as_millis(),
                    "presets sync completed"
                );
            }
            Ok(text_to_print_network::presets_client::FetchResult::NotModified) => {
                tracing::info!(
                    elapsed_ms = %start.elapsed().as_millis(),
                    "presets sync: not modified (304, cache current)"
                );
            }
            Err(e) => {
                tracing::warn!(
                    error = ?e,
                    endpoint,
                    elapsed_ms = %start.elapsed().as_millis(),
                    "presets sync failed (silent fallback to cache/bundled)"
                );
            }
        }
    });
}

/// Spawn the background task that loads the Embedded backend
///
/// `custom_gguf_path` (BYO LLM, 2026-08-23) — when `Some`, the load
/// path skips the HF download wait entirely and points the backend
/// directly at the given file A missing custom file surfaces as
/// `EmbeddedStatus::Error` so the user gets an immediate signal
///
/// 8-arg signature is intentional: `custom_gguf_path` was added on
/// top of the existing 7-arg contract without repackaging into a
/// config struct, since 4 call sites already exist and packaging
/// would just shift the parameter count from function → constructor
#[allow(clippy::too_many_arguments)]
fn spawn_embedded_load(
    runtime: &tokio::runtime::Runtime,
    models_dir: PathBuf,
    choice: text_to_print_llm::model::ModelChoice,
    execution_mode: ExecutionMode,
    custom_gguf_path: Option<PathBuf>,
    embedded_slot: std::sync::Arc<std::sync::Mutex<Option<EmbeddedBackend>>>,
    status_slot: std::sync::Arc<std::sync::Mutex<EmbeddedStatus>>,
    mut model_progress_rx: tokio::sync::watch::Receiver<
        text_to_print_llm::downloader::DownloadProgress,
    >,
) {
    runtime.spawn(async move {
        // Resolve GGUF path: custom override wins over ModelChoice default
        let model_path = if let Some(p) = custom_gguf_path {
            if !p.exists() {
                *status_slot.lock().expect("embedded status lock") = EmbeddedStatus::Error(
                    format!("custom GGUF path does not exist: {}", p.display()),
                );
                return;
            }
            tracing::info!(
                path = %p.display(),
                ?choice,
                "loading embedded backend from custom GGUF (skipping HF download)"
            );
            p
        } else {
            // Wait for model DL to complete Choice-agnostic here — the DL
            // pipeline only knows about the initial choice; runtime-switched
            // choices are expected to be user-placed at
            // `models_dir/{choice.default_filename()}` If missing, load fails
            // fast with an IO error which surfaces as EmbeddedStatus::Error
            while !matches!(
                model_progress_rx.borrow().status,
                text_to_print_llm::downloader::DownloadStatus::Complete
            ) {
                if model_progress_rx.changed().await.is_err() {
                    return;
                }
                // Once initial DL completes we still proceed even if the user
                // switched to a different choice — the load path will
                // discover whether the file exists on disk
                if text_to_print_llm::downloader::model_exists(&models_dir, choice) {
                    break;
                }
            }
            text_to_print_llm::downloader::model_path(&models_dir, choice)
        };
        *status_slot.lock().expect("embedded status lock") = EmbeddedStatus::Loading;
        let mp_first = model_path.clone();
        let load_result = tokio::task::spawn_blocking(move || {
            EmbeddedBackend::load_full(&mp_first, choice, execution_mode)
        })
        .await;
        match load_result {
            Ok(Ok(backend)) => {
                *embedded_slot.lock().expect("embedded slot lock") = Some(backend);
                *status_slot.lock().expect("embedded status lock") = EmbeddedStatus::Ready;
                tracing::info!(?choice, ?execution_mode, "embedded backend loaded");
            }
            Ok(Err(e)) => {
                // Stage 3-C.15: GPU load failure → automatic CPU
                // fallback so users on hostless-GPU systems (or with
                // insufficient VRAM) still get a working Embedded path
                // Surfaces as `EmbeddedStatus::Ready` with a fallback
                // log line rather than `Error(...)` so the UI doesn't
                // look like the toggle was rejected
                if execution_mode == ExecutionMode::Gpu {
                    tracing::warn!(
                        error = %e,
                        ?choice,
                        "GPU load failed, falling back to CPU"
                    );
                    let mp_cpu = model_path.clone();
                    let cpu_result = tokio::task::spawn_blocking(move || {
                        EmbeddedBackend::load_full(&mp_cpu, choice, ExecutionMode::Cpu)
                    })
                    .await;
                    match cpu_result {
                        Ok(Ok(backend)) => {
                            *embedded_slot.lock().expect("embedded slot lock") = Some(backend);
                            *status_slot.lock().expect("embedded status lock") =
                                EmbeddedStatus::Ready;
                            tracing::info!(
                                ?choice,
                                "embedded backend loaded (CPU fallback after GPU failure)"
                            );
                            return;
                        }
                        Ok(Err(e_cpu)) => {
                            *status_slot.lock().expect("embedded status lock") =
                                EmbeddedStatus::Error(format!("GPU: {e} / CPU fallback: {e_cpu}"));
                            tracing::warn!(
                                error = %e_cpu,
                                ?choice,
                                "CPU fallback also failed"
                            );
                            return;
                        }
                        Err(join_err) => {
                            *status_slot.lock().expect("embedded status lock") =
                                EmbeddedStatus::Error(format!(
                                    "GPU: {e} / CPU fallback task panic: {join_err}"
                                ));
                            return;
                        }
                    }
                }
                *status_slot.lock().expect("embedded status lock") =
                    EmbeddedStatus::Error(e.to_string());
                tracing::warn!(
                    error = %e,
                    ?choice,
                    ?execution_mode,
                    "embedded backend load failed"
                );
            }
            Err(join_err) => {
                *status_slot.lock().expect("embedded status lock") =
                    EmbeddedStatus::Error(join_err.to_string());
                tracing::warn!(
                    error = %join_err,
                    ?choice,
                    ?execution_mode,
                    "embedded backend load task panicked"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{
        Battery18650HolderUiState, BuildPlateRackUiState, BusinessCardUiState, CableClipUiState,
        CardTrayUiState, ChopstickHolderUiState, ClampRackUiState, CoasterUiState,
        CottonDispenserUiState, CustomizerState, CutleryTrayUiState, DeskShelfUiState,
        DrillBitHolderUiState, DriverRackUiState, EggTrayUiState, Esp32EnclosureUiState,
        FilamentSpoolHolderUiState, GenerationPhase, GridfinityUiState, HairdryerHolderUiState,
        HeadphoneHolderUiState, HexBitHolderUiState, HexKeyHolderUiState, KcupHolderUiState,
        LedChannelUiState, MagneticStripUiState, MonitorRiserUiState, NozzleHolderUiState,
        PenCupUiState, PhaseProgress, PhoneStandUiState, PillOrganizerUiState, PliersRackUiState,
        RaspiCaseUiState, RazorHolderUiState, SdCardHolderUiState, SinkCaddyUiState,
        SoapTrayUiState, SockDividerUiState, SocketRailUiState, SpiceRackUiState,
        StickyNoteUiState, StorageBoxUiState, SwatchHolderUiState, TissueBoxCoverUiState,
        TokenWellUiState, ToothbrushHolderUiState, TpHolderUiState, UnderDeskMountUiState,
        UtensilCaddyUiState, WrapHolderUiState, WrenchHolderUiState, default_sidecar_port,
    };
    use std::time::Duration;

    #[test]
    fn port_from_localhost_endpoint() {
        assert_eq!(
            default_sidecar_port("http://localhost:8000/v1/chat/completions"),
            8000
        );
        assert_eq!(
            default_sidecar_port("http://127.0.0.1:12345/v1/chat/completions"),
            12345
        );
    }

    #[test]
    fn port_defaults_when_missing() {
        assert_eq!(default_sidecar_port("http://localhost/v1/x"), 8000);
        assert_eq!(default_sidecar_port(""), 8000);
        assert_eq!(default_sidecar_port("garbage"), 8000);
    }

    // ────────────────────────────────────────────────────────
    // Generation phase state-machine tests
    //
    // UI 側で表示する 5 phase の進捗 (Llm → Parse → Mesh → Safety → Export)
    // を pure state (egui / DB / tokio 非依存) で verify する 実 pipeline は
    // `crates/core/tests/e2e_pipeline.rs` で E2E 通過を確認しているため、
    // ここは phase transition の invariant (順序 / 冪等 / reset) のみ担保
    // ────────────────────────────────────────────────────────

    #[test]
    fn phase_all_covers_five_phases_in_order() {
        assert_eq!(GenerationPhase::ALL.len(), 5);
        assert_eq!(GenerationPhase::ALL[0], GenerationPhase::Llm);
        assert_eq!(GenerationPhase::ALL[1], GenerationPhase::Parse);
        assert_eq!(GenerationPhase::ALL[2], GenerationPhase::Mesh);
        assert_eq!(GenerationPhase::ALL[3], GenerationPhase::Safety);
        assert_eq!(GenerationPhase::ALL[4], GenerationPhase::Export);
    }

    #[test]
    fn phase_label_stable_for_ui() {
        assert_eq!(GenerationPhase::Llm.label(), "LLM");
        assert_eq!(GenerationPhase::Parse.label(), "parse");
        assert_eq!(GenerationPhase::Mesh.label(), "mesh");
        assert_eq!(GenerationPhase::Safety.label(), "safety");
        assert_eq!(GenerationPhase::Export.label(), "export");
    }

    #[test]
    fn phase_progress_default_is_empty() {
        let p = PhaseProgress::default();
        assert!(p.current.is_none());
        assert!(p.completed.is_empty());
        assert_eq!(p.retry_count, 0);
        for phase in GenerationPhase::ALL {
            assert!(
                !p.is_done(phase),
                "{} should not be done initially",
                phase.label()
            );
            assert!(p.latency_of(phase).is_none());
        }
    }

    #[test]
    fn phase_progress_marks_completed_phase() {
        let mut p = PhaseProgress {
            current: Some(GenerationPhase::Llm),
            completed: vec![(GenerationPhase::Llm, Duration::from_millis(120))],
            retry_count: 0,
            generation_start: None,
        };
        p.current = Some(GenerationPhase::Parse);
        assert!(p.is_done(GenerationPhase::Llm));
        assert!(!p.is_done(GenerationPhase::Parse));
        assert_eq!(
            p.latency_of(GenerationPhase::Llm),
            Some(Duration::from_millis(120))
        );
        assert!(p.latency_of(GenerationPhase::Parse).is_none());
    }

    #[test]
    fn phase_progress_full_pipeline_flow() {
        let latencies = [80, 5, 340, 15, 40];
        let completed: Vec<_> = GenerationPhase::ALL
            .iter()
            .zip(latencies)
            .map(|(phase, ms)| (*phase, Duration::from_millis(ms)))
            .collect();
        let p = PhaseProgress {
            current: Some(GenerationPhase::Export),
            completed,
            retry_count: 2,
            generation_start: None,
        };
        for (phase, ms) in GenerationPhase::ALL.iter().zip(latencies) {
            assert!(p.is_done(*phase));
            assert_eq!(p.latency_of(*phase), Some(Duration::from_millis(ms)));
        }
        assert_eq!(p.retry_count, 2);
    }

    #[test]
    fn phase_progress_reset_clears_all_state() {
        let mut p = PhaseProgress {
            current: Some(GenerationPhase::Mesh),
            completed: vec![
                (GenerationPhase::Llm, Duration::from_millis(1)),
                (GenerationPhase::Parse, Duration::from_millis(2)),
            ],
            retry_count: 3,
            generation_start: None,
        };
        p.reset();
        assert!(p.current.is_none());
        assert!(p.completed.is_empty());
        assert_eq!(p.retry_count, 0);
        for phase in GenerationPhase::ALL {
            assert!(!p.is_done(phase));
        }
    }

    #[test]
    fn phase_progress_idempotent_reset() {
        let mut p = PhaseProgress::default();
        p.reset();
        p.reset();
        assert!(p.current.is_none());
        assert!(p.completed.is_empty());
        assert_eq!(p.retry_count, 0);
    }

    // ── customizer_state tests ──

    #[test]
    fn gridfinity_default_is_2x2_6u() {
        let g = GridfinityUiState::default();
        assert_eq!(g.units_x, 2);
        assert_eq!(g.units_y, 2);
        assert_eq!(g.height_u, 6);
    }

    #[test]
    fn gridfinity_to_lol_matches_dsl_syntax() {
        let g = GridfinityUiState {
            units_x: 3,
            units_y: 4,
            height_u: 6,
            ..Default::default()
        };
        assert_eq!(g.to_lol(), "gridfinity_bin(3, 4, 6)");
    }

    #[test]
    fn gridfinity_to_lol_default_matches_2x2_6u() {
        let g = GridfinityUiState::default();
        assert_eq!(g.to_lol(), "gridfinity_bin(2, 2, 6)");
    }

    #[test]
    fn customizer_state_default_is_gridfinity_default() {
        let c = CustomizerState::default();
        assert_eq!(c.gridfinity.units_x, 2);
        assert_eq!(c.gridfinity.units_y, 2);
        assert_eq!(c.gridfinity.height_u, 6);
    }

    // ── Phase C: gridfinity_bin_ex advanced UI state tests ──

    #[test]
    fn gridfinity_advanced_default_is_basic_lol() {
        let g = GridfinityUiState::default();
        assert!(!g.use_dividers);
        assert_eq!(g.dividers_x, 2);
        assert_eq!(g.dividers_y, 2);
        assert!((g.wall_thickness - 1.2).abs() < 1e-6);
        assert!((g.floor_thickness - 1.5).abs() < 1e-6);
        // default state (no advanced use) → basic gridfinity_bin
        assert_eq!(g.to_lol(), "gridfinity_bin(2, 2, 6)");
    }

    #[test]
    fn gridfinity_with_dividers_switches_to_ex() {
        let g = GridfinityUiState {
            use_dividers: true,
            dividers_x: 3,
            dividers_y: 2,
            ..Default::default()
        };
        assert_eq!(g.to_lol(), "gridfinity_bin_ex(2, 2, 6, 3, 2, 1.2, 1.5)");
    }

    #[test]
    fn gridfinity_with_custom_wall_switches_to_ex() {
        let g = GridfinityUiState {
            wall_thickness: 1.8,
            ..Default::default()
        };
        assert_eq!(g.to_lol(), "gridfinity_bin_ex(2, 2, 6, 0, 0, 1.8, 1.5)");
    }

    // ── Phase B: PART 2 archetype UI state tests ──

    #[test]
    fn sticky_note_default_is_small_square() {
        let s = StickyNoteUiState::default();
        assert!((s.pad_width - 76.0).abs() < 1e-6);
        assert!((s.pad_depth - 76.0).abs() < 1e-6);
        assert!((s.height - 30.0).abs() < 1e-6);
        assert_eq!(s.to_lol(), "sticky_note_holder(76, 76, 30)");
    }

    #[test]
    fn business_card_default_is_jp_meishi() {
        let b = BusinessCardUiState::default();
        assert!((b.card_width - 91.0).abs() < 1e-6);
        assert!((b.card_height - 55.0).abs() < 1e-6);
        assert!((b.slot_thickness - 22.0).abs() < 1e-6);
        assert_eq!(b.to_lol(), "business_card_holder(91, 55, 22)");
    }

    #[test]
    fn pen_cup_default_is_standard_75x100() {
        let p = PenCupUiState::default();
        assert!((p.inner_diameter - 75.0).abs() < 1e-6);
        assert!((p.height - 100.0).abs() < 1e-6);
        assert_eq!(p.to_lol(), "pen_cup(75, 100)");
    }

    #[test]
    fn phone_stand_default_is_phone_with_cable_hole() {
        let ps = PhoneStandUiState::default();
        assert!((ps.slot_width - 14.0).abs() < 1e-6);
        assert!((ps.back_height - 100.0).abs() < 1e-6);
        assert!((ps.cable_hole_dia - 18.0).abs() < 1e-6);
        assert_eq!(ps.to_lol(), "phone_stand(14, 100, 18)");
    }

    #[test]
    fn customizer_state_default_includes_all_5_archetypes() {
        let c = CustomizerState::default();
        // 5 archetype (旧、PART 1 + PART 2 前半) default 全部 set されている
        assert_eq!(c.gridfinity.to_lol(), "gridfinity_bin(2, 2, 6)");
        assert_eq!(c.sticky_note.to_lol(), "sticky_note_holder(76, 76, 30)");
        assert_eq!(c.business_card.to_lol(), "business_card_holder(91, 55, 22)");
        assert_eq!(c.pen_cup.to_lol(), "pen_cup(75, 100)");
        assert_eq!(c.phone_stand.to_lol(), "phone_stand(14, 100, 18)");
    }

    // ── Phase B2: PART 2 残 4 archetype UI state tests ──

    #[test]
    fn headphone_holder_default_is_wall_mount() {
        let h = HeadphoneHolderUiState::default();
        assert!((h.arm_length - 80.0).abs() < 1e-6);
        assert!((h.headband_width - 50.0).abs() < 1e-6);
        assert!((h.mount_width - 100.0).abs() < 1e-6);
        assert_eq!(h.to_lol(), "headphone_holder(80, 50, 100)");
    }

    #[test]
    fn under_desk_mount_default_is_standard_desk() {
        let m = UnderDeskMountUiState::default();
        assert!((m.desk_thickness - 25.0).abs() < 1e-6);
        assert!((m.clamp_width - 40.0).abs() < 1e-6);
        assert!((m.screw_hole_dia - 4.0).abs() < 1e-6);
        assert_eq!(m.to_lol(), "under_desk_mount(25, 40, 4)");
    }

    #[test]
    fn desk_shelf_default_is_desktop_400x200() {
        let s = DeskShelfUiState::default();
        assert!((s.shelf_width - 400.0).abs() < 1e-6);
        assert!((s.shelf_depth - 200.0).abs() < 1e-6);
        assert!((s.leg_height - 100.0).abs() < 1e-6);
        assert_eq!(s.to_lol(), "desk_shelf(400, 200, 100)");
    }

    #[test]
    fn monitor_riser_default_is_compact_desk() {
        let r = MonitorRiserUiState::default();
        assert!((r.width - 250.0).abs() < 1e-6);
        assert!((r.depth - 180.0).abs() < 1e-6);
        assert!((r.height - 90.0).abs() < 1e-6);
        assert_eq!(r.to_lol(), "monitor_riser(250, 180, 90)");
    }

    #[test]
    fn customizer_state_default_includes_all_9_archetypes() {
        let c = CustomizerState::default();
        // Phase B2 追加後は 9 archetype 全部 (PART 1 + PART 2 全部)
        assert_eq!(c.gridfinity.to_lol(), "gridfinity_bin(2, 2, 6)");
        assert_eq!(c.sticky_note.to_lol(), "sticky_note_holder(76, 76, 30)");
        assert_eq!(c.business_card.to_lol(), "business_card_holder(91, 55, 22)");
        assert_eq!(c.pen_cup.to_lol(), "pen_cup(75, 100)");
        assert_eq!(c.phone_stand.to_lol(), "phone_stand(14, 100, 18)");
        assert_eq!(c.headphone_holder.to_lol(), "headphone_holder(80, 50, 100)");
        assert_eq!(c.under_desk_mount.to_lol(), "under_desk_mount(25, 40, 4)");
        assert_eq!(c.desk_shelf.to_lol(), "desk_shelf(400, 200, 100)");
        assert_eq!(c.monitor_riser.to_lol(), "monitor_riser(250, 180, 90)");
    }

    // ── Sprint 4: household.md 3 archetype UI state tests ──

    #[test]
    fn coaster_default_is_round_95x5() {
        let c = CoasterUiState::default();
        assert!((c.diameter - 95.0).abs() < 1e-6);
        assert!((c.thickness - 5.0).abs() < 1e-6);
        assert_eq!(c.to_lol(), "coaster(95, 5)");
    }

    #[test]
    fn tissue_box_cover_default_is_rectangular_us() {
        let t = TissueBoxCoverUiState::default();
        assert!((t.internal_length - 231.0).abs() < 1e-6);
        assert!((t.internal_width - 116.0).abs() < 1e-6);
        assert!((t.internal_height - 53.0).abs() < 1e-6);
        assert_eq!(t.to_lol(), "tissue_box_cover(231, 116, 53)");
    }

    #[test]
    fn storage_box_default_is_medium() {
        let s = StorageBoxUiState::default();
        assert!((s.internal_length - 150.0).abs() < 1e-6);
        assert!((s.internal_width - 100.0).abs() < 1e-6);
        assert!((s.internal_height - 60.0).abs() < 1e-6);
        assert_eq!(s.to_lol(), "storage_box(150, 100, 60)");
    }

    #[test]
    fn customizer_state_default_includes_all_12_archetypes() {
        let c = CustomizerState::default();
        // Sprint 4 追加後は 12 archetype (organizer PART 1+2 完全 + household 3)
        assert_eq!(c.coaster.to_lol(), "coaster(95, 5)");
        assert_eq!(
            c.tissue_box_cover.to_lol(),
            "tissue_box_cover(231, 116, 53)"
        );
        assert_eq!(c.storage_box.to_lol(), "storage_box(150, 100, 60)");
    }

    // ── Sprint 5: hobby-diy.md 4 archetype UI state tests ──

    #[test]
    fn cable_clip_default_is_hdmi() {
        let c = CableClipUiState::default();
        assert!((c.cable_diameter - 7.0).abs() < 1e-6);
        assert!((c.clip_length - 28.0).abs() < 1e-6);
        assert_eq!(c.to_lol(), "cable_clip(7, 28)");
    }

    #[test]
    fn led_channel_default_is_ws2812b_300mm() {
        let l = LedChannelUiState::default();
        assert!((l.strip_width - 10.0).abs() < 1e-6);
        assert!((l.channel_length - 300.0).abs() < 1e-6);
        assert_eq!(l.to_lol(), "led_channel(10, 300)");
    }

    #[test]
    fn card_tray_default_is_poker() {
        let t = CardTrayUiState::default();
        assert!((t.card_width - 63.0).abs() < 1e-6);
        assert!((t.card_height - 88.0).abs() < 1e-6);
        assert!((t.tray_depth - 30.0).abs() < 1e-6);
        assert_eq!(t.to_lol(), "card_tray(63, 88, 30)");
    }

    #[test]
    fn token_well_default_is_dice_4() {
        let t = TokenWellUiState::default();
        assert!((t.well_diameter - 20.0).abs() < 1e-6);
        assert!((t.well_depth - 20.0).abs() < 1e-6);
        assert_eq!(t.well_count, 4);
        assert_eq!(t.to_lol(), "token_well(20, 20, 4)");
    }

    #[test]
    fn customizer_state_default_includes_all_16_archetypes() {
        let c = CustomizerState::default();
        // Sprint 5 追加後は 16 archetype (organizer PART 1+2 完全 + household 3 + hobby-diy 4)
        assert_eq!(c.cable_clip.to_lol(), "cable_clip(7, 28)");
        assert_eq!(c.led_channel.to_lol(), "led_channel(10, 300)");
        assert_eq!(c.card_tray.to_lol(), "card_tray(63, 88, 30)");
        assert_eq!(c.token_well.to_lol(), "token_well(20, 20, 4)");
    }

    // ── Sprint 6: tools.md 3 archetype UI state tests ──

    #[test]
    fn wrench_holder_default_is_metric_6() {
        let w = WrenchHolderUiState::default();
        assert!((w.min_size_mm - 8.0).abs() < 1e-6);
        assert!((w.max_size_mm - 19.0).abs() < 1e-6);
        assert_eq!(w.count, 6);
        assert_eq!(w.to_lol(), "wrench_holder(8, 19, 6)");
    }

    #[test]
    fn socket_rail_default_is_half_inch_6() {
        let s = SocketRailUiState::default();
        assert!((s.post_diameter - 12.4).abs() < 1e-6);
        assert!((s.post_height - 22.0).abs() < 1e-6);
        assert_eq!(s.post_count, 6);
        assert_eq!(s.to_lol(), "socket_rail(12.4, 22, 6)");
    }

    #[test]
    fn hex_bit_holder_default_is_grid_4x5() {
        let h = HexBitHolderUiState::default();
        assert_eq!(h.rows, 5);
        assert_eq!(h.cols, 4);
        assert!((h.spacing - 12.0).abs() < 1e-6);
        assert_eq!(h.to_lol(), "hex_bit_holder(5, 4, 12)");
    }

    #[test]
    fn customizer_state_default_includes_all_19_archetypes() {
        let c = CustomizerState::default();
        // Sprint 6 追加後は 19 archetype (organizer 9 + household 3 + hobby-diy 4 + tools 3)
        assert_eq!(c.wrench_holder.to_lol(), "wrench_holder(8, 19, 6)");
        assert_eq!(c.socket_rail.to_lol(), "socket_rail(12.4, 22, 6)");
        assert_eq!(c.hex_bit_holder.to_lol(), "hex_bit_holder(5, 4, 12)");
    }

    // ── Sprint 7: electronics-enclosure.md 3 archetype UI state tests ──

    #[test]
    fn raspi_case_default_is_rpi5() {
        let c = RaspiCaseUiState::default();
        assert!((c.pcb_width - 85.0).abs() < 1e-6);
        assert!((c.pcb_depth - 56.0).abs() < 1e-6);
        assert!((c.internal_height - 25.0).abs() < 1e-6);
        assert_eq!(c.to_lol(), "raspi_case(85, 56, 25)");
    }

    #[test]
    fn esp32_enclosure_default_is_devkit_v1() {
        let e = Esp32EnclosureUiState::default();
        assert!((e.pcb_width - 51.6).abs() < 1e-6);
        assert!((e.pcb_depth - 28.4).abs() < 1e-6);
        assert!((e.internal_height - 15.0).abs() < 1e-6);
        assert_eq!(e.to_lol(), "esp32_enclosure(51.6, 28.4, 15)");
    }

    #[test]
    fn battery_18650_holder_default_is_row_4_through() {
        let b = Battery18650HolderUiState::default();
        assert_eq!(b.cell_count, 4);
        assert!((b.wall_thickness - 2.5).abs() < 1e-6);
        assert!((b.floor_thickness - 0.0).abs() < 1e-6);
        assert_eq!(b.to_lol(), "battery_18650_holder(4, 2.5, 0)");
    }

    #[test]
    fn customizer_state_default_includes_all_22_archetypes() {
        let c = CustomizerState::default();
        // Sprint 7 追加後は 22 archetype (organizer 9 + household 3 + hobby-diy 4 + tools 3 + electronics 3)
        assert_eq!(c.raspi_case.to_lol(), "raspi_case(85, 56, 25)");
        assert_eq!(
            c.esp32_enclosure.to_lol(),
            "esp32_enclosure(51.6, 28.4, 15)"
        );
        assert_eq!(
            c.battery_18650_holder.to_lol(),
            "battery_18650_holder(4, 2.5, 0)"
        );
    }

    // ── Sprint 8: organizer-bathroom-garage.md 3 archetype UI state tests ──

    #[test]
    fn toothbrush_holder_default_is_manual_4() {
        let t = ToothbrushHolderUiState::default();
        assert_eq!(t.count, 4);
        assert!((t.hole_diameter - 15.0).abs() < 1e-6);
        assert!((t.hole_depth - 70.0).abs() < 1e-6);
        assert_eq!(t.to_lol(), "toothbrush_holder(4, 15, 70)");
    }

    #[test]
    fn drill_bit_holder_default_is_metric_11() {
        let d = DrillBitHolderUiState::default();
        assert!((d.min_size_mm - 3.0).abs() < 1e-6);
        assert!((d.max_size_mm - 13.0).abs() < 1e-6);
        assert_eq!(d.count, 11);
        assert_eq!(d.to_lol(), "drill_bit_holder(3, 13, 11)");
    }

    #[test]
    fn pliers_rack_default_is_standard_6() {
        let p = PliersRackUiState::default();
        assert_eq!(p.slot_count, 6);
        assert!((p.slot_width - 15.0).abs() < 1e-6);
        assert!((p.slot_depth - 60.0).abs() < 1e-6);
        assert_eq!(p.to_lol(), "pliers_rack(6, 15, 60)");
    }

    #[test]
    fn customizer_state_default_includes_all_25_archetypes() {
        let c = CustomizerState::default();
        // Sprint 8 追加後は 25 archetype (organizer 9 + household 3 + hobby-diy 4 + tools 3 + electronics 3 + bathroom-garage 3)
        assert_eq!(c.toothbrush_holder.to_lol(), "toothbrush_holder(4, 15, 70)");
        assert_eq!(c.drill_bit_holder.to_lol(), "drill_bit_holder(3, 13, 11)");
        assert_eq!(c.pliers_rack.to_lol(), "pliers_rack(6, 15, 60)");
    }

    // ── Sprint 9: organizer-cable-kitchen.md 3 archetype UI state tests ──

    #[test]
    fn spice_rack_default_is_standard_6() {
        let s = SpiceRackUiState::default();
        assert_eq!(s.count, 6);
        assert!((s.jar_diameter - 48.0).abs() < 1e-6);
        assert!((s.jar_height - 100.0).abs() < 1e-6);
        assert_eq!(s.to_lol(), "spice_rack(6, 48, 100)");
    }

    #[test]
    fn egg_tray_default_is_4x3() {
        let e = EggTrayUiState::default();
        assert_eq!(e.rows, 3);
        assert_eq!(e.cols, 4);
        assert!((e.cup_depth - 18.0).abs() < 1e-6);
        assert_eq!(e.to_lol(), "egg_tray(3, 4, 18)");
    }

    #[test]
    fn utensil_caddy_default_is_standard_4() {
        let u = UtensilCaddyUiState::default();
        assert_eq!(u.count, 4);
        assert!((u.compartment_diameter - 65.0).abs() < 1e-6);
        assert!((u.height - 130.0).abs() < 1e-6);
        assert_eq!(u.to_lol(), "utensil_caddy(4, 65, 130)");
    }

    #[test]
    fn customizer_state_default_includes_all_28_archetypes() {
        let c = CustomizerState::default();
        // Sprint 9 追加後は 28 archetype (+3: spice_rack / egg_tray / utensil_caddy)
        assert_eq!(c.spice_rack.to_lol(), "spice_rack(6, 48, 100)");
        assert_eq!(c.egg_tray.to_lol(), "egg_tray(3, 4, 18)");
        assert_eq!(c.utensil_caddy.to_lol(), "utensil_caddy(4, 65, 130)");
    }

    // ── Sprint 10: organizer-printer-modular.md 3 archetype UI state tests ──

    #[test]
    fn filament_spool_holder_default_is_standard_1kg() {
        let f = FilamentSpoolHolderUiState::default();
        assert!((f.spool_outer_diameter - 200.0).abs() < 1e-6);
        assert!((f.spool_width - 68.0).abs() < 1e-6);
        assert!((f.bore_diameter - 52.0).abs() < 1e-6);
        assert_eq!(f.to_lol(), "filament_spool_holder(200, 68, 52)");
    }

    #[test]
    fn nozzle_holder_default_is_m6_row_8() {
        let n = NozzleHolderUiState::default();
        assert_eq!(n.count, 8);
        assert!((n.hole_diameter - 8.0).abs() < 1e-6);
        assert!((n.hole_depth - 6.0).abs() < 1e-6);
        assert_eq!(n.to_lol(), "nozzle_holder(8, 8, 6)");
    }

    #[test]
    fn build_plate_rack_default_is_standard_5() {
        let r = BuildPlateRackUiState::default();
        assert_eq!(r.slot_count, 5);
        assert!((r.slot_spacing - 15.0).abs() < 1e-6);
        assert!((r.height - 200.0).abs() < 1e-6);
        assert_eq!(r.to_lol(), "build_plate_rack(5, 15, 200)");
    }

    #[test]
    fn customizer_state_default_includes_all_31_archetypes() {
        let c = CustomizerState::default();
        // Sprint 10 追加後は 31 archetype (+3: filament_spool_holder / nozzle_holder / build_plate_rack)
        assert_eq!(
            c.filament_spool_holder.to_lol(),
            "filament_spool_holder(200, 68, 52)"
        );
        assert_eq!(c.nozzle_holder.to_lol(), "nozzle_holder(8, 8, 6)");
        assert_eq!(c.build_plate_rack.to_lol(), "build_plate_rack(5, 15, 200)");
    }

    // ── Sprint 11: organizer-drawer-wall.md 3 archetype UI state tests ──

    #[test]
    fn cutlery_tray_default_is_standard_3() {
        let c = CutleryTrayUiState::default();
        assert_eq!(c.slot_count, 3);
        assert!((c.slot_width - 35.0).abs() < 1e-6);
        assert!((c.slot_length - 220.0).abs() < 1e-6);
        assert_eq!(c.to_lol(), "cutlery_tray(3, 35, 220)");
    }

    #[test]
    fn pill_organizer_default_is_weekly_7x2() {
        let p = PillOrganizerUiState::default();
        assert_eq!(p.rows, 7);
        assert_eq!(p.cols, 2);
        assert!((p.cell_size - 20.0).abs() < 1e-6);
        assert_eq!(p.to_lol(), "pill_organizer(7, 2, 20)");
    }

    #[test]
    fn magnetic_strip_default_is_knife_rail_8() {
        let m = MagneticStripUiState::default();
        assert_eq!(m.magnet_count, 8);
        assert!((m.magnet_diameter - 6.0).abs() < 1e-6);
        assert!((m.magnet_spacing - 30.0).abs() < 1e-6);
        assert_eq!(m.to_lol(), "magnetic_strip(8, 6, 30)");
    }

    #[test]
    fn customizer_state_default_includes_all_34_archetypes() {
        let c = CustomizerState::default();
        // Sprint 11 追加後は 34 archetype (+3: cutlery_tray / pill_organizer / magnetic_strip)
        assert_eq!(c.cutlery_tray.to_lol(), "cutlery_tray(3, 35, 220)");
        assert_eq!(c.pill_organizer.to_lol(), "pill_organizer(7, 2, 20)");
        assert_eq!(c.magnetic_strip.to_lol(), "magnetic_strip(8, 6, 30)");
    }

    // ── Sprint 12 ミックス 3 archetype UI state tests ──

    #[test]
    fn hairdryer_holder_default_is_dyson() {
        let h = HairdryerHolderUiState::default();
        assert!((h.barrel_diameter - 85.0).abs() < 1e-6);
        assert!((h.holster_depth - 110.0).abs() < 1e-6);
        assert!((h.wall_thickness - 3.0).abs() < 1e-6);
        assert_eq!(h.to_lol(), "hairdryer_holder(85, 110, 3)");
    }

    #[test]
    fn kcup_holder_default_is_4x3() {
        let k = KcupHolderUiState::default();
        assert_eq!(k.rows, 3);
        assert_eq!(k.cols, 4);
        assert!((k.capsule_diameter - 53.0).abs() < 1e-6);
        assert_eq!(k.to_lol(), "kcup_holder(3, 4, 53)");
    }

    #[test]
    fn hex_key_holder_default_is_metric_9() {
        let h = HexKeyHolderUiState::default();
        assert_eq!(h.count, 9);
        assert!((h.min_key_mm - 1.5).abs() < 1e-6);
        assert!((h.max_key_mm - 10.0).abs() < 1e-6);
        assert_eq!(h.to_lol(), "hex_key_holder(9, 1.5, 10)");
    }

    #[test]
    fn customizer_state_default_includes_all_37_archetypes() {
        let c = CustomizerState::default();
        // Sprint 12 追加後は 37 archetype (+3: hairdryer_holder / kcup_holder / hex_key_holder)
        assert_eq!(c.hairdryer_holder.to_lol(), "hairdryer_holder(85, 110, 3)");
        assert_eq!(c.kcup_holder.to_lol(), "kcup_holder(3, 4, 53)");
        assert_eq!(c.hex_key_holder.to_lol(), "hex_key_holder(9, 1.5, 10)");
    }

    // ── Sprint 13 ミックス 3 archetype UI state tests ──

    #[test]
    fn wrap_holder_default_is_foil_12inch() {
        let w = WrapHolderUiState::default();
        assert!((w.roll_diameter - 55.0).abs() < 1e-6);
        assert!((w.roll_width - 305.0).abs() < 1e-6);
        assert!((w.wall_thickness - 3.0).abs() < 1e-6);
        assert_eq!(w.to_lol(), "wrap_holder(55, 305, 3)");
    }

    #[test]
    fn sock_divider_default_is_standard_4() {
        let d = SockDividerUiState::default();
        assert_eq!(d.cell_count, 4);
        assert!((d.cell_width - 80.0).abs() < 1e-6);
        assert!((d.height - 89.0).abs() < 1e-6);
        assert_eq!(d.to_lol(), "sock_divider(4, 80, 89)");
    }

    #[test]
    fn soap_tray_default_is_dual_bottle() {
        let s = SoapTrayUiState::default();
        assert!((s.tray_length - 200.0).abs() < 1e-6);
        assert!((s.tray_width - 90.0).abs() < 1e-6);
        assert_eq!(s.drain_slot_count, 6);
        assert_eq!(s.to_lol(), "soap_tray(200, 90, 6)");
    }

    #[test]
    fn customizer_state_default_includes_all_40_archetypes() {
        let c = CustomizerState::default();
        // Sprint 13 追加後は 40 archetype (+3: wrap_holder / sock_divider / soap_tray)
        assert_eq!(c.wrap_holder.to_lol(), "wrap_holder(55, 305, 3)");
        assert_eq!(c.sock_divider.to_lol(), "sock_divider(4, 80, 89)");
        assert_eq!(c.soap_tray.to_lol(), "soap_tray(200, 90, 6)");
    }

    // ── Sprint 14 ミックス 3 archetype UI state tests ──

    #[test]
    fn razor_holder_default_is_cartridge() {
        let r = RazorHolderUiState::default();
        assert!((r.slot_width - 12.0).abs() < 1e-6);
        assert!((r.slot_depth - 22.0).abs() < 1e-6);
        assert!((r.mount_hole_diameter - 4.5).abs() < 1e-6);
        assert_eq!(r.to_lol(), "razor_holder(12, 22, 4.5)");
    }

    #[test]
    fn chopstick_holder_default_is_adult_4() {
        let c = ChopstickHolderUiState::default();
        assert_eq!(c.pair_count, 4);
        assert!((c.slot_width - 13.0).abs() < 1e-6);
        assert!((c.slot_length - 260.0).abs() < 1e-6);
        assert_eq!(c.to_lol(), "chopstick_holder(4, 13, 260)");
    }

    #[test]
    fn swatch_holder_default_is_standard_8x4() {
        let s = SwatchHolderUiState::default();
        assert_eq!(s.rows, 8);
        assert_eq!(s.cols, 4);
        assert!((s.swatch_width - 32.0).abs() < 1e-6);
        assert_eq!(s.to_lol(), "swatch_holder(8, 4, 32)");
    }

    #[test]
    fn customizer_state_default_includes_all_43_archetypes() {
        let c = CustomizerState::default();
        // Sprint 14 追加後は 43 archetype (+3: razor_holder / chopstick_holder / swatch_holder)
        assert_eq!(c.razor_holder.to_lol(), "razor_holder(12, 22, 4.5)");
        assert_eq!(c.chopstick_holder.to_lol(), "chopstick_holder(4, 13, 260)");
        assert_eq!(c.swatch_holder.to_lol(), "swatch_holder(8, 4, 32)");
    }

    // ── Sprint 15 ミックス 4 archetype UI state tests ──

    #[test]
    fn tp_holder_default_is_standard() {
        let t = TpHolderUiState::default();
        assert!((t.inner_diameter - 40.0).abs() < 1e-6);
        assert!((t.roll_width - 110.0).abs() < 1e-6);
        assert!((t.wall_thickness - 5.0).abs() < 1e-6);
        assert_eq!(t.to_lol(), "tp_holder(40, 110, 5)");
    }

    #[test]
    fn sd_card_holder_default_is_full_sd_4x4() {
        let s = SdCardHolderUiState::default();
        assert_eq!(s.rows, 4);
        assert_eq!(s.cols, 4);
        assert!((s.card_width - 24.0).abs() < 1e-6);
        assert_eq!(s.to_lol(), "sd_card_holder(4, 4, 24)");
    }

    #[test]
    fn driver_rack_default_is_standard_8() {
        let d = DriverRackUiState::default();
        assert_eq!(d.slot_count, 8);
        assert!((d.slot_diameter - 25.0).abs() < 1e-6);
        assert!((d.height - 100.0).abs() < 1e-6);
        assert_eq!(d.to_lol(), "driver_rack(8, 25, 100)");
    }

    #[test]
    fn customizer_state_default_includes_all_46_archetypes() {
        let c = CustomizerState::default();
        // Sprint 15 追加後は 46 archetype (+3: tp_holder / sd_card_holder / driver_rack)
        assert_eq!(c.tp_holder.to_lol(), "tp_holder(40, 110, 5)");
        assert_eq!(c.sd_card_holder.to_lol(), "sd_card_holder(4, 4, 24)");
        assert_eq!(c.driver_rack.to_lol(), "driver_rack(8, 25, 100)");
    }

    // ── Sprint 16 ミックス 5 archetype UI state tests ──

    #[test]
    fn cotton_dispenser_default_is_standard_80() {
        let c = CottonDispenserUiState::default();
        assert_eq!(c.count, 80);
        assert!((c.inner_diameter - 90.0).abs() < 1e-6);
        assert!((c.height - 100.0).abs() < 1e-6);
        assert_eq!(c.to_lol(), "cotton_dispenser(80, 90, 100)");
    }

    #[test]
    fn sink_caddy_default_is_standard_l200() {
        let s = SinkCaddyUiState::default();
        assert!((s.tray_length - 200.0).abs() < 1e-6);
        assert!((s.tray_width - 100.0).abs() < 1e-6);
        assert_eq!(s.drain_hole_count, 8);
        assert_eq!(s.to_lol(), "sink_caddy(200, 100, 8)");
    }

    #[test]
    fn clamp_rack_default_is_standard_5() {
        let c = ClampRackUiState::default();
        assert_eq!(c.hook_count, 5);
        assert!((c.hook_width - 30.0).abs() < 1e-6);
        assert!((c.height - 150.0).abs() < 1e-6);
        assert_eq!(c.to_lol(), "clamp_rack(5, 30, 150)");
    }

    #[test]
    fn customizer_state_default_includes_all_49_archetypes() {
        let c = CustomizerState::default();
        // Sprint 16 追加後は 49 archetype (+3: cotton_dispenser / sink_caddy / clamp_rack)
        assert_eq!(c.cotton_dispenser.to_lol(), "cotton_dispenser(80, 90, 100)");
        assert_eq!(c.sink_caddy.to_lol(), "sink_caddy(200, 100, 8)");
        assert_eq!(c.clamp_rack.to_lol(), "clamp_rack(5, 30, 150)");
    }
}
