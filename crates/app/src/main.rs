mod sdf;
mod state;
mod ui;

use anyhow::Result;
use eframe::egui;
use state::AppState;
use ui::gallery::GalleryState;
use ui::settings::SettingsState;
use ui::viewer::SdfViewer;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();

    let data_dir = directories::ProjectDirs::from("net", "alicelaw", "3dvbgaran")
        .map(|d: directories::ProjectDirs| d.data_dir().to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from(".3dvbgaran"));

    std::fs::create_dir_all(&data_dir)?;

    // ALICE ノード初期化 + P2P バックグラウンド起動
    let mut node = tdvbgaran_network::node::AliceNode::init(&data_dir)?;
    let p2p_runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()?;
    let _ = node.start_background(&p2p_runtime);
    tracing::info!("ALICE node initialized");

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_title("3dvbgaran — Text to 3D"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "3dvbgaran",
        options,
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);

            // SDF リソースを wgpu レンダラーに登録
            if let Some(render_state) = cc.wgpu_render_state.as_ref() {
                let sdf_resources = sdf::SdfResources::init(render_state);
                render_state
                    .renderer
                    .write()
                    .callback_resources
                    .insert(sdf_resources);
            }

            let render_state = cc.wgpu_render_state.clone();
            Ok(Box::new(App::new(data_dir, render_state, node)))
        }),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    let font_data = include_bytes!("../../../assets/NotoSansJP.ttf");
    fonts.font_data.insert(
        "NotoSansJP".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(font_data)),
    );

    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "NotoSansJP".to_owned());

    fonts
        .families
        .entry(egui::FontFamily::Monospace)
        .or_default()
        .push("NotoSansJP".to_owned());

    ctx.set_fonts(fonts);
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Tab {
    Generate,
    Gallery,
    History,
    Settings,
}

struct App {
    state: AppState,
    viewer: SdfViewer,
    settings: SettingsState,
    gallery: GalleryState,
    node: tdvbgaran_network::node::AliceNode,
    current_tab: Tab,
    render_state: Option<egui_wgpu::RenderState>,
}

impl App {
    fn new(
        data_dir: std::path::PathBuf,
        render_state: Option<egui_wgpu::RenderState>,
        node: tdvbgaran_network::node::AliceNode,
    ) -> Self {
        Self {
            state: AppState::new(data_dir),
            viewer: SdfViewer::default(),
            settings: SettingsState::default(),
            gallery: GalleryState::default(),
            node,
            current_tab: Tab::Generate,
            render_state,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if matches!(
            self.state.generation_status,
            state::GenerationStatus::Generating
        ) {
            ctx.request_repaint();
        }

        // General tier: SDF 自動公開
        if let Some((id, lol, prompt)) = self.state.pending_publish.take() {
            self.node.publish_sdf(&id, &lol, &prompt);
        }

        // pending WGSL があれば SDF パイプラインを再構築
        if let Some(wgsl) = self.viewer.pending_wgsl.take()
            && let Some(rs) = &self.render_state
            && let Some(res) = rs.renderer.write().callback_resources.get_mut::<sdf::SdfResources>()
        {
            res.rebuild_with_wgsl(&rs.device, &wgsl);
        }

        // カメラ情報を SdfResources に反映
        if self.viewer.has_sdf
            && let Some(rs) = &self.render_state
            && let Some(res) = rs.renderer.write().callback_resources.get_mut::<sdf::SdfResources>()
        {
            let cam = &self.viewer.camera;
            res.camera_pos = cam.position.into();
            res.camera_target = cam.target.into();
            res.camera_up = cam.up.into();
            res.camera_fov = cam.fov;
        }

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.current_tab, Tab::Generate, "Generate");
                ui.selectable_value(&mut self.current_tab, Tab::Gallery, "Gallery");
                ui.selectable_value(&mut self.current_tab, Tab::History, "History");
                ui.selectable_value(&mut self.current_tab, Tab::Settings, "Settings");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let usage = self.state.daily_usage();
                    let limit = self.state.tier.limits().daily_generations;
                    ui.label(format!("{:?} | {}/{}", self.state.tier, usage, limit));
                });
            });
        });

        match self.current_tab {
            Tab::Generate => {
                egui::SidePanel::left("prompt_panel")
                    .default_width(350.0)
                    .show(ctx, |ui| {
                        ui::prompt::show(ui, &mut self.state);
                    });

                egui::CentralPanel::default().show(ctx, |ui| {
                    ui::viewer::show(ui, &self.state, &mut self.viewer);
                });
            }
            Tab::Gallery => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui::gallery::show(ui, &mut self.node, &mut self.viewer, &mut self.gallery);
                });
            }
            Tab::History => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui::history::show(ui, &mut self.state);
                });
            }
            Tab::Settings => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui::settings::show(ui, &mut self.state, &mut self.settings);
                });
            }
        }
    }
}
