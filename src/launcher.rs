use crate::{
    airport::Airport,
    bridge::MockBridge,
    cli::{Command as CliCommand, RunOptions},
    config::{Settings, user_dir},
    engine::{Ground, Manager, Phase, Stats},
    geo::Position,
    sources::Model,
};
use anyhow::{Context, Result, ensure};
use eframe::egui;
use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(Default, Clone)]
pub struct LaunchOptions {
    pub duration: u32,
    pub atc_csv: String,
    pub metadata: String,
    pub atc_airborne: bool,
    pub atc_voice: bool,
    pub pilot_callsign: String,
    pub pilot_destination: String,
    pub pilot_runway: String,
    pub voice_model: String,
}
pub fn command_args(
    settings: &Settings,
    profile: &Path,
    options: &LaunchOptions,
    action: &str,
    stop: &Path,
) -> Result<Vec<String>> {
    settings.validate()?;
    let profile = std::fs::canonicalize(profile)?;
    let mut settings = settings.clone();
    settings.resolve_paths(&profile);
    let mut args = vec!["--config".into(), profile.to_string_lossy().into_owned()];
    match action {
        "doctor" => args.push("doctor".into()),
        "scan" => args.push("scan".into()),
        "demo" | "atc-demo" => {
            args.extend([
                action.into(),
                "--steps".into(),
                "600".into(),
                "--output".into(),
                profile
                    .parent()
                    .unwrap()
                    .join(format!("launcher-{action}-results.json"))
                    .to_string_lossy()
                    .into_owned(),
            ]);
        }
        "run" => {
            ensure!(cfg!(windows), "MSFS доступен только в Windows");
            ensure!(options.duration > 0, "Укажите длительность больше нуля");
            let fsltl = crate::sources::discover(&settings.fsltl_path)
                .context("Выберите установленную папку FSLTL")?;
            args.extend([
                "run".into(),
                "--bridge".into(),
                "simconnect".into(),
                "--fsltl".into(),
                fsltl.to_string_lossy().into_owned(),
                "--duration".into(),
                options.duration.to_string(),
                "--stop-file".into(),
                stop.to_string_lossy().into_owned(),
            ]);
            if settings.enable_simconnect_position_writes {
                args.push("--allow-motion".into());
            }
            if !settings.airport_graph.is_empty() {
                let graph = Airport::load(Path::new(&settings.airport_graph))?;
                ensure!(
                    !graph.fictional,
                    "Демонстрационный граф нельзя использовать в MSFS"
                );
                ensure!(
                    settings.enable_simconnect_position_writes,
                    "Наземное движение требует записи позиций"
                );
                args.extend(["--airport".into(), settings.airport_graph.clone()]);
            }
            for (flag, value) in [
                ("--atc-csv", &options.atc_csv),
                ("--metadata", &options.metadata),
                ("--pilot-voice-model", &options.voice_model),
            ] {
                if !value.is_empty() {
                    let path = Path::new(value);
                    ensure!(path.exists(), "Не найден путь {value}");
                    args.extend([flag.into(), value.clone()]);
                }
            }
            if options.atc_airborne {
                args.push("--atc-airborne".into());
            }
            if options.atc_voice {
                args.push("--atc-voice".into());
            }
            if !options.pilot_callsign.is_empty() {
                args.extend([
                    "--pilot-callsign".into(),
                    options.pilot_callsign.clone(),
                    "--pilot-destination".into(),
                    options.pilot_destination.clone(),
                    "--pilot-runway".into(),
                    options.pilot_runway.clone(),
                ]);
            }
            // Use the same parser as the CLI to prevent divergent launch syntax.
            use clap::Parser;
            let parsed = crate::cli::Cli::try_parse_from(
                std::iter::once("realflow".to_owned()).chain(args.clone()),
            )?;
            ensure!(
                matches!(parsed.command, CliCommand::Run(RunOptions { .. })),
                "Invalid run command"
            );
        }
        _ => anyhow::bail!("Неизвестная команда"),
    }
    Ok(args)
}
struct Launcher {
    settings: Settings,
    profile: PathBuf,
    profile_text: String,
    options: LaunchOptions,
    status: String,
    log: String,
    process: Option<Child>,
    events: Option<mpsc::Receiver<String>>,
    stop: Option<PathBuf>,
    radio_input: String,
    preview: Option<Manager<MockBridge>>,
    stats: Stats,
    last_tick: Instant,
    sim_time: f64,
    confirm_msfs: bool,
    download_events: Option<mpsc::Receiver<DownloadEvent>>,
    download_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    installer: Option<PathBuf>,
}
enum DownloadEvent {
    Progress(u64),
    Done(std::result::Result<PathBuf, String>),
}
impl Drop for Launcher {
    fn drop(&mut self) {
        self.download_cancel
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }
}
impl Launcher {
    fn new(profile: PathBuf) -> Self {
        let (settings, status) = match Settings::load(&profile) {
            Ok(s) => (s, "Готов к работе".into()),
            Err(e) => (Settings::default(), format!("Ошибка профиля: {e}")),
        };
        Self {
            profile_text: profile.to_string_lossy().into_owned(),
            settings,
            profile,
            options: LaunchOptions {
                duration: 60,
                ..LaunchOptions::default()
            },
            status,
            log: String::new(),
            process: None,
            events: None,
            stop: None,
            radio_input: String::new(),
            preview: None,
            stats: Stats::default(),
            last_tick: Instant::now(),
            sim_time: 1e6,
            confirm_msfs: false,
            download_events: None,
            download_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            installer: None,
        }
    }
    fn download_installer(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("FlyByWire-Installer.exe")
            .add_filter("Windows installer", &["exe"])
            .save_file()
        else {
            return;
        };
        self.download_cancel
            .store(false, std::sync::atomic::Ordering::Relaxed);
        let cancel = self.download_cancel.clone();
        let (tx, rx) = mpsc::channel();
        self.download_events = Some(rx);
        self.installer = None;
        self.status = "Скачивание официального FlyByWire Installer…".into();
        std::thread::spawn(move || {
            let result = crate::traffic_download::download(&path, &cancel, |n| {
                let _ = tx.send(DownloadEvent::Progress(n));
            })
            .map(|()| path)
            .map_err(|e| format!("Не удалось скачать установщик: {e:#}"));
            let _ = tx.send(DownloadEvent::Done(result));
        });
    }
    fn find_traffic(&mut self) -> Result<()> {
        let root = crate::sources::discover(&self.settings.fsltl_path)
            .or_else(|| crate::sources::discover(""))
            .context("Модели пока не найдены. Установите FSLTL Traffic Base Models через FlyByWire Installer или выберите их папку вручную.")?;
        ensure!(
            !crate::sources::scan(&root)?.is_empty(),
            "В папке FSLTL нет моделей"
        );
        self.settings.fsltl_path = root.to_string_lossy().into_owned();
        self.save()?;
        self.status = format!("Модели найдены, путь сохранён: {}", root.display());
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        self.profile = PathBuf::from(self.profile_text.trim());
        ensure!(!self.profile.as_os_str().is_empty(), "Выберите профиль");
        self.settings.save(&self.profile)?;
        self.status = format!("Сохранено: {}", self.profile.display());
        Ok(())
    }
    fn load(&mut self, path: PathBuf) -> Result<()> {
        let settings = Settings::load(&path)?;
        self.settings = settings;
        self.profile = path;
        self.profile_text = self.profile.to_string_lossy().into_owned();
        self.status = "Профиль загружен".into();
        Ok(())
    }
    fn start(&mut self, action: &str) -> Result<()> {
        ensure!(self.process.is_none(), "Запуск уже выполняется");
        self.save()?;
        let exe = std::env::current_exe()?
            .parent()
            .unwrap()
            .join(if cfg!(windows) {
                "realflow.exe"
            } else {
                "realflow"
            });
        ensure!(
            exe.is_file(),
            "Поместите realflow и realflow-launcher в одну папку"
        );
        let stop = user_dir().join(format!(
            "stop-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        let args = command_args(&self.settings, &self.profile, &self.options, action, &stop)?;
        let mut command = Command::new(exe);
        command
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let (tx, rx) = mpsc::sync_channel(512);
        for reader in [
            Box::new(stdout) as Box<dyn std::io::Read + Send>,
            Box::new(stderr),
        ] {
            let tx = tx.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(reader).lines() {
                    match line {
                        Ok(line) => {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }
        self.process = Some(child);
        self.events = Some(rx);
        self.stop = Some(stop);
        self.log.clear();
        self.status = "Выполняется…".into();
        Ok(())
    }
    fn stop_process(&mut self) -> Result<()> {
        if let Some(path) = &self.stop {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, [])?;
            self.status = "Остановка и очистка самолётов…".into();
        }
        Ok(())
    }
    fn poll(&mut self) {
        let updates: Vec<_> = self
            .download_events
            .as_ref()
            .map(|rx| rx.try_iter().collect())
            .unwrap_or_default();
        for update in updates {
            match update {
                DownloadEvent::Progress(bytes) => {
                    self.status = format!("Скачивание установщика: {} МБ…", bytes / 1024 / 1024)
                }
                DownloadEvent::Done(result) => {
                    self.download_events = None;
                    match result {
                        Ok(path) => {
                            self.status = "Установщик скачан и проверен. Запустите его и установите FSLTL Traffic Base Models.".into();
                            self.installer = Some(path);
                        }
                        Err(error) => self.status = error,
                    }
                }
            }
        }
        if let Some(events) = &self.events {
            for line in events.try_iter().take(512) {
                self.log.push_str(&line);
                self.log.push('\n');
            }
            if self.log.len() > 100_000 {
                let mut start = self.log.len() - 80_000;
                while !self.log.is_char_boundary(start) {
                    start += 1;
                }
                self.log.drain(..start);
            }
        }
        if let Some(child) = &mut self.process {
            match child.try_wait() {
                Ok(Some(status)) => {
                    self.status = format!("Завершено: {status}");
                    self.process = None;
                    if let Some(path) = self.stop.take() {
                        let _ = std::fs::remove_file(path);
                    }
                }
                Err(e) => self.status = e.to_string(),
                _ => {}
            }
        }
    }
    fn preview_toggle(&mut self) -> Result<()> {
        if self.preview.is_some() {
            self.preview = None;
            self.status = "Превью остановлено".into();
            return Ok(());
        }
        self.settings.validate()?;
        let mut ground = Ground::new(Airport::demo(), self.settings.ground_limit);
        if self.settings.ground_limit > 0 {
            ground.add("TEST101", "MOCK A320", "GATE_A", "RUNWAY_EXIT", false)?;
        }
        if self.settings.ground_limit > 1 {
            ground.add("TEST102", "MOCK A320", "GATE_B", "RUNWAY_EXIT", false)?;
        }
        self.preview = Some(Manager::new(
            self.settings.clone(),
            vec![Model::fixture()],
            MockBridge::new(),
            Some(ground),
        )?);
        self.sim_time = 1e6;
        self.last_tick = Instant::now();
        self.status = "Офлайн-превью: вымышленные самолёты и аэропорт".into();
        Ok(())
    }
    fn map(&self, ui: &mut egui::Ui) {
        let graph = Airport::demo();
        let (rect, _) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 300.), egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 8., egui::Color32::from_rgb(12, 23, 34));
        let (min_lat, max_lat, min_lon, max_lon) = graph
            .nodes
            .values()
            .fold((90f64, -90f64, 180f64, -180f64), |(a, b, c, d), n| {
                (a.min(n.lat), b.max(n.lat), c.min(n.lon), d.max(n.lon))
            });
        let pos = |p: Position| {
            egui::pos2(
                rect.left()
                    + 25.
                    + ((p.lon - min_lon) / (max_lon - min_lon)) as f32 * (rect.width() - 50.),
                rect.bottom()
                    - 25.
                    - ((p.lat - min_lat) / (max_lat - min_lat)) as f32 * (rect.height() - 50.),
            )
        };
        for edge in graph.edges.values() {
            painter.line_segment(
                [
                    pos(graph.nodes[&edge.src].position()),
                    pos(graph.nodes[&edge.dst].position()),
                ],
                egui::Stroke::new(
                    if edge.runway.is_empty() { 2. } else { 5. },
                    if edge.runway.is_empty() {
                        egui::Color32::GRAY
                    } else {
                        egui::Color32::GOLD
                    },
                ),
            );
        }
        for n in graph.nodes.values() {
            if n.kind == "gate" || n.kind == "hold" {
                painter.text(
                    pos(n.position()),
                    egui::Align2::LEFT_BOTTOM,
                    &n.id,
                    egui::FontId::monospace(10.),
                    egui::Color32::WHITE,
                );
            }
        }
        if let Some(m) = &self.preview
            && let Some(g) = &m.ground
        {
            for p in g
                .aircraft
                .values()
                .filter(|p| ![Phase::Airborne, Phase::Complete].contains(&p.phase))
            {
                painter.circle_filled(pos(p.position), 5., egui::Color32::from_rgb(69, 212, 206));
            }
        }
    }
    fn result(&mut self, result: Result<()>) {
        if let Err(e) = result {
            self.status = format!("Ошибка: {e:#}");
        }
    }
}
fn file_row(ui: &mut egui::Ui, label: &str, value: &mut String, directory: bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.text_edit_singleline(value);
        if ui.button("Выбрать…").clicked() {
            let picker = rfd::FileDialog::new();
            let path = if directory {
                picker.pick_folder()
            } else {
                picker.pick_file()
            };
            if let Some(path) = path {
                *value = path.to_string_lossy().into_owned();
            }
        }
    });
}
impl eframe::App for Launcher {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll();
        ctx.request_repaint_after(Duration::from_millis(100));
        if ctx.input(|i| i.viewport().close_requested()) && self.process.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            let result = self.stop_process();
            self.result(result);
        }
        if self.preview.is_some() && self.last_tick.elapsed() >= Duration::from_millis(120) {
            self.last_tick = Instant::now();
            for _ in 0..3 {
                self.sim_time += 1.;
                match self.preview.as_mut().unwrap().tick(
                    1.,
                    self.sim_time,
                    Position::new(47.452, -122.3095, 5000.),
                    8,
                ) {
                    Ok(stats) => self.stats = stats,
                    Err(e) => {
                        self.status = e.to_string();
                        self.preview = None;
                        break;
                    }
                }
            }
        }
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.heading("REALFLOW · Rust");
            ui.label("Трафик и ATC для MSFS 2020 · нативный лаунчер");
            ui.horizontal(|ui| {
                ui.label("JSON-профиль");
                ui.text_edit_singleline(&mut self.profile_text);
                if ui.button("Открыть").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("JSON", &["json"])
                        .pick_file()
                {
                    let result = self.load(path);
                    self.result(result);
                }
                if ui.button("Сохранить").clicked() {
                    let result = self.save();
                    self.result(result);
                }
                if ui.button("Сохранить как…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("JSON", &["json"])
                        .set_file_name("config.json")
                        .save_file()
                {
                    self.profile_text = path.to_string_lossy().into_owned();
                    let result = self.save();
                    self.result(result);
                }
            });
            ui.label(&self.status);
        });
        egui::CentralPanel::default().show(ctx,|ui|{egui::ScrollArea::vertical().show(ui,|ui|{
            ui.columns(2,|columns|{
                let ui=&mut columns[0];ui.heading("Настройки трафика");
                egui::ComboBox::from_label("Источник").selected_text(&self.settings.mode).show_ui(ui,|ui|{for mode in ["hybrid","live","simulation"]{ui.selectable_value(&mut self.settings.mode,mode.into(),mode);}});
                ui.add(egui::Slider::new(&mut self.settings.airborne_limit,0..=35).text("В воздухе"));ui.add(egui::Slider::new(&mut self.settings.ground_limit,0..=30).text("На земле"));
                ui.checkbox(&mut self.settings.sync_real_flights,"OpenSky: реальные наблюдения");ui.add(egui::DragValue::new(&mut self.settings.fetch_seconds).range(60..=86400).prefix("Интервал, сек: "));
                ui.add(egui::Slider::new(&mut self.settings.max_radius_km,2. ..=250.).text("Радиус, км"));
                file_row(ui,"FSLTL",&mut self.settings.fsltl_path,true);
                ui.collapsing("Скачать модели трафика", |ui| {
                    ui.label("FSLTL устанавливается через официальный FlyByWire Installer. Выберите в нём FSLTL → Traffic Base Models и установите пакет.");
                    ui.hyperlink_to("Официальная страница скачивания", crate::traffic_download::RELEASES_URL);
                    if ui.add_enabled(cfg!(windows) && self.download_events.is_none(), egui::Button::new("Скачать FlyByWire Installer")).clicked() { self.download_installer(); }
                    if self.download_events.is_some() && ui.button("Отменить скачивание").clicked() { self.download_cancel.store(true, std::sync::atomic::Ordering::Relaxed); }
                    if let Some(path) = self.installer.clone()
                        && ui.add_enabled(cfg!(windows), egui::Button::new("Запустить скачанный установщик")).clicked() {
                        let result = Command::new("explorer.exe").arg(&path).spawn().map(|_| ()).context("Не удалось открыть установщик");
                        self.result(result);
                    }
                    if ui.button("Найти установленные модели и сохранить путь").clicked() { let r = self.find_traffic(); self.result(r); }
                    ui.label("Если папка Community нестандартная, выберите папку fsltl-traffic-base вручную выше.");
                });file_row(ui,"Граф аэропорта",&mut self.settings.airport_graph,false);
                ui.collapsing("Дополнительные параметры",|ui|{ui.checkbox(&mut self.settings.realistic_taxi,"Реалистичное руление");ui.checkbox(&mut self.settings.runway_control,"Контроль полосы");ui.add(egui::DragValue::new(&mut self.settings.fps_target).range(1..=240).prefix("Целевой FPS: "));ui.label("Эти три параметра сохраняются для совместимости. Движок пока не применяет переключатели; блокировки полос всегда включены.");});
                let ui=&mut columns[1];ui.heading("Запуск MSFS и ATC");ui.checkbox(&mut self.settings.enable_simconnect_position_writes,"Экспериментальная запись позиций");ui.add(egui::DragValue::new(&mut self.options.duration).range(1..=86400).prefix("Длительность, сек: "));
                file_row(ui,"Частоты CSV",&mut self.options.atc_csv,false);file_row(ui,"Типы самолётов CSV",&mut self.options.metadata,false);
                ui.checkbox(&mut self.options.atc_airborne,"Воздушный ATC для синтетического AI");ui.checkbox(&mut self.options.atc_voice,"Озвучивать радио (Windows SAPI)");
                ui.collapsing("Радио пилота",|ui|{for(label,value)in [("Позывной",&mut self.options.pilot_callsign),("ICAO назначения",&mut self.options.pilot_destination),("Полоса",&mut self.options.pilot_runway)]{ui.horizontal(|ui|{ui.label(label);ui.text_edit_singleline(value);});}file_row(ui,"Модель Vosk",&mut self.options.voice_model,true);});
                ui.label("MSFS должен быть запущен. Граф аэропорта необходимо проверить в игре. OpenSky-ключи берутся из окружения.");
            });
            ui.separator();ui.horizontal_wrapped(|ui|{
                if ui.button(if self.preview.is_some(){"Остановить превью"}else{"Офлайн-превью"}).clicked(){let r=self.preview_toggle();self.result(r);}
                let idle=self.process.is_none();for(label,action)in [("Диагностика","doctor"),("Сканировать FSLTL","scan"),("Демо трафика","demo"),("Демо ATC","atc-demo")]{if ui.add_enabled(idle,egui::Button::new(label)).clicked(){let r=self.start(action);self.result(r);}}
                if ui.add_enabled(idle&&cfg!(windows),egui::Button::new("Запустить MSFS")).clicked(){self.confirm_msfs=true;}
                if ui.add_enabled(!idle,egui::Button::new("Остановить запуск")).clicked(){let r=self.stop_process();self.result(r);}
            });
            ui.label(format!("Офлайн-превью · воздух {} · земля {} · объекты {}",self.stats.synthetic+self.stats.live,self.stats.ground,self.stats.objects));
            self.map(ui);ui.label("Вымышленная геометрия. Превью не подключается к игре.");
            ui.horizontal(|ui|{ui.label("Радио пилота");ui.text_edit_singleline(&mut self.radio_input);if ui.add_enabled(self.process.is_some(),egui::Button::new("Передать")).clicked()&& let Some(stdin)=self.process.as_mut().and_then(|p|p.stdin.as_mut()){let result=writeln!(stdin,"{}",self.radio_input).and_then(|_|stdin.flush()).map_err(Into::into);self.result(result);self.radio_input.clear();}});
            ui.label("Команды: /status, /quit. Изменения настроек действуют при следующем запуске.");
            ui.add(egui::TextEdit::multiline(&mut self.log).font(egui::TextStyle::Monospace).desired_width(f32::INFINITY).desired_rows(10).interactive(false));
        });});
        if self.confirm_msfs {
            egui::Window::new("Экспериментальное подключение")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("SimConnect и движение FSLTL пока не проверены в игре. Продолжить?");
                    ui.horizontal(|ui| {
                        if ui.button("Запустить").clicked() {
                            self.confirm_msfs = false;
                            let r = self.start("run");
                            self.result(r);
                        }
                        if ui.button("Отмена").clicked() {
                            self.confirm_msfs = false;
                        }
                    });
                });
        }
    }
}
pub fn main() -> eframe::Result {
    use clap::Parser;
    #[derive(Parser)]
    #[command(version, about = "Native RealFlow settings launcher")]
    struct Arguments {
        #[arg(long,default_value_os_t=user_dir().join("config.json"))]
        config: PathBuf,
    }
    let args = Arguments::parse();
    eframe::run_native(
        "RealFlow Launcher",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1120., 850.])
                .with_min_inner_size([880., 650.]),
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(Launcher::new(args.config)))
        }),
    )
}
