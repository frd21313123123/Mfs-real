//! Scenery tab in the native RealFlow launcher.
use crate::scenery::{self, Scenery};
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::{AtomicBool, Ordering}, mpsc},
};

enum Event {
    Bytes(u64),
    Done(Result<Vec<String>, String>),
}

pub struct SceneryPanel {
    query: String,
    destination: String,
    items: Vec<Scenery>,
    error: Option<String>,
    status: String,
    events: Option<mpsc::Receiver<Event>>,
    cancel: Arc<AtomicBool>,
}

impl Default for SceneryPanel {
    fn default() -> Self {
        let (items, error) = match scenery::catalog() {
            Ok(items) => (items, None),
            Err(error) => (Vec::new(), Some(error.to_string())),
        };
        Self {
            query: String::new(),
            destination: scenery::detect_community()
                .map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            items, error,
            status: "Выберите ICAO и нажмите «Скачать и установить»".into(),
            events: None,
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl Drop for SceneryPanel {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl SceneryPanel {
    fn target(&self) -> Result<PathBuf, String> {
        let target = PathBuf::from(self.destination.trim());
        if !target.is_dir() {
            return Err("Папка назначения не существует. Выберите Community или свою папку.".into());
        }
        Ok(target)
    }

    fn start_download(&mut self, item: Scenery) {
        if self.events.is_some() { return; }
        let target = match self.target() {
            Ok(target) => target,
            Err(error) => { self.status = error; return; }
        };
        self.cancel.store(false, Ordering::Relaxed);
        let cancel = Arc::clone(&self.cancel);
        let (tx, rx) = mpsc::channel();
        self.events = Some(rx);
        self.status = format!("Поиск готового ZIP-релиза: {}", item.name);
        std::thread::spawn(move || {
            let result = scenery::download_and_install(&item, &target, &cancel, |count| {
                let _ = tx.send(Event::Bytes(count));
            }).map_err(|error| format!("{error:#}"));
            let _ = tx.send(Event::Done(result));
        });
    }

    fn import_zip(&mut self, zip: PathBuf) {
        if self.events.is_some() { return; }
        let target = match self.target() {
            Ok(target) => target,
            Err(error) => { self.status = error; return; }
        };
        let (tx, rx) = mpsc::channel();
        self.events = Some(rx);
        self.status = format!("Установка из ZIP: {}", zip.display());
        std::thread::spawn(move || {
            let result = scenery::install_local_zip(&zip, &target)
                .map_err(|error| format!("{error:#}"));
            let _ = tx.send(Event::Done(result));
        });
    }

    fn poll(&mut self) {
        let messages: Vec<_> = self.events.as_ref()
            .map(|r| r.try_iter().collect()).unwrap_or_default();
        for event in messages {
            match event {
                Event::Bytes(n) => self.status = format!("Загружено: {:.1} МБ", n as f64 / 1_048_576.),
                Event::Done(result) => {
                    self.events = None;
                    self.status = match result {
                        Ok(packages) => format!("Установлено: {}. Перезапустите MSFS 2020.", packages.join(", ")),
                        Err(error) => format!("Ошибка: {error}"),
                    };
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.poll();
        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.heading("Сценарии аэропортов MSFS 2020");
            ui.label("Бесплатные пакеты из официальных GitHub Releases авторов. Загрузка и распаковка в один клик.");
            ui.horizontal_wrapped(|ui| {
                ui.label("Папка установки:");
                ui.add(egui::TextEdit::singleline(&mut self.destination).desired_width(480.));
                if ui.button("Выбрать…").clicked()
                    && let Some(path) = rfd::FileDialog::new().pick_folder() {
                    self.destination = path.to_string_lossy().into_owned();
                }
                if ui.button("Найти Community").clicked() {
                    match scenery::detect_community() {
                        Some(path) => {
                            self.destination = path.to_string_lossy().into_owned();
                            self.status = "Активная Community найдена по настройкам MSFS".into();
                        }
                        None => self.status = "Не удалось найти Community автоматически. Укажите путь вручную.".into(),
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label("ICAO:");
                ui.add(egui::TextEdit::singleline(&mut self.query).desired_width(140.).hint_text("PATK, SAQU…"));
                if ui.button("Очистить").clicked() { self.query.clear(); }
                if ui.add_enabled(self.events.is_none(), egui::Button::new("Установить свой ZIP…")).clicked()
                    && let Some(zip) = rfd::FileDialog::new().add_filter("MSFS ZIP", &["zip"]).pick_file() {
                    self.import_zip(zip);
                }
            });
            if let Some(error) = &self.error { ui.colored_label(egui::Color32::RED, error); }
            ui.label(&self.status);
            if self.events.is_some() && ui.button("Отменить загрузку").clicked() {
                self.cancel.store(true, Ordering::Relaxed);
                self.status = "Запрошена отмена…".into();
            }
            ui.separator();
            let matching: Vec<Scenery> = scenery::matches(&self.items, &self.query)
                .into_iter().cloned().collect();
            if matching.is_empty() {
                ui.label("Сценариев для этого ICAO в подключённом каталоге пока нет.");
                ui.label("Можно скачать ZIP с сайта автора и установить его кнопкой «Установить свой ZIP».");
                ui.hyperlink_to("Поиск других сценариев на Flightsim.to", "https://flightsim.to/");
            } else {
                ui.label(format!("Найдено пакетов: {}. Всего ICAO в каталоге: {}.",
                    matching.len(), self.items.iter().map(|x| x.icaos.len()).sum::<usize>()));
                for item in matching {
                    ui.group(|ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.vertical(|ui| {
                                ui.strong(&item.name);
                                ui.label(format!("ICAO: {} | Автор: {} | Лицензия: {}", item.icaos.join(", "), item.author, item.license));
                                ui.hyperlink_to("Страница автора и релизы", format!("https://github.com/{}/releases", item.repo));
                            });
                            if ui.add_enabled(self.events.is_none(),
                                egui::Button::new("Скачать и установить")).clicked() {
                                self.start_download(item.clone());
                            }
                        });
                    });
                }
            }
            ui.separator();
            ui.small("Готовые пакеты не входят в RealFlow. Существующие файлы не перезаписываются. Для появления сценария может потребоваться перезапуск симулятора.");
            ui.small("Каталог расширяемый: scenery/catalog.json. Flightsim.to и платные магазины без разрешённого API не скачиваются автоматически.");
        });
    }
}
