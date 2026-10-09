//! Opt-in catalogue of author-hosted freeware scenery for MSFS 2020.
//! Only ZIP assets from allowlisted GitHub release repositories are downloaded.
#[cfg(not(windows))]
use anyhow::bail;
use anyhow::{Context, Result, ensure};
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};
use walkdir::WalkDir;

#[derive(Debug, Clone, Deserialize)]
pub struct Scenery {
    pub id: String,
    pub icaos: Vec<String>,
    pub name: String,
    pub author: String,
    pub repo: String,
    pub license: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}
#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

pub fn catalog() -> Result<Vec<Scenery>> {
    let list: Vec<Scenery> = serde_json::from_str(include_str!("../scenery/catalog.json"))?;
    for entry in &list {
        ensure!(
            !entry.id.is_empty() && !entry.name.is_empty(),
            "Пустая запись каталога"
        );
        ensure!(
            entry.icaos.iter().all(|s| icao(s).is_ok()),
            "Неверный ICAO в каталоге"
        );
        validate_repo(&entry.repo)?;
    }
    Ok(list)
}

pub fn icao(input: &str) -> Result<String> {
    let value = input.trim().to_ascii_uppercase();
    ensure!(
        value.len() == 4 && value.bytes().all(|b| b.is_ascii_alphanumeric()),
        "Введите четырёхсимвольный ICAO (например, UUEE или PATK)"
    );
    Ok(value)
}

pub fn matches<'a>(items: &'a [Scenery], query: &str) -> Vec<&'a Scenery> {
    let needle = query.trim().to_ascii_uppercase();
    items
        .iter()
        .filter(|item| {
            needle.is_empty()
                || item.icaos.iter().any(|code| code.contains(&needle))
                || item.name.to_ascii_uppercase().contains(&needle)
        })
        .collect()
}

fn validate_repo(repo: &str) -> Result<()> {
    let parts: Vec<_> = repo.split('/').collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|part| {
                !part.is_empty()
                    && part.len() <= 100
                    && part
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
                    && *part != "."
                    && *part != ".."
            }),
        "Недопустимый GitHub-репозиторий"
    );
    Ok(())
}

pub fn community_from_usercfg(text: &str) -> Option<PathBuf> {
    text.lines().rev().find_map(|line| {
        let trimmed = line.trim();
        let rest = trimmed.strip_prefix("InstalledPackagesPath")?.trim();
        let base = rest.strip_prefix('"')?.split('"').next()?.trim();
        if base.is_empty() {
            None
        } else {
            Some(PathBuf::from(base).join("Community"))
        }
    })
}

pub fn detect_community() -> Option<PathBuf> {
    let mut config_paths = Vec::new();
    if let Some(roaming) = std::env::var_os("APPDATA") {
        config_paths.push(
            PathBuf::from(roaming)
                .join("Microsoft Flight Simulator")
                .join("UserCfg.opt"),
        );
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        config_paths.push(
            PathBuf::from(local)
                .join("Packages")
                .join("Microsoft.FlightSimulator_8wekyb3d8bbwe")
                .join("LocalCache")
                .join("UserCfg.opt"),
        );
    }
    for cfg in &config_paths {
        if let Ok(data) = fs::read_to_string(cfg)
            && let Some(path) = community_from_usercfg(&data)
            && path.is_dir()
        {
            return Some(path);
        }
    }
    // Fallbacks only when the game did not provide an installed-packages path.
    for cfg in &config_paths {
        if let Some(parent) = cfg.parent() {
            let path = parent.join("Packages").join("Community");
            if path.is_dir() {
                return Some(path);
            }
        }
    }
    None
}

fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent("RealFlow-Scenery/0.1 (https://github.com/frd21313123123/Mfs-real)")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(180))
        .build()?)
}

fn github_asset(entry: &Scenery) -> Result<GithubAsset> {
    validate_repo(&entry.repo)?;
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=30",
        entry.repo
    );
    let response = client()?
        .get(url)
        .send()?
        .error_for_status()
        .context("GitHub Releases недоступен")?;
    let releases: Vec<GithubRelease> = response.json()?;
    releases.into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .flat_map(|release| release.assets)
        .find(|asset| asset.name.to_ascii_lowercase().ends_with(".zip")
            && asset.size > 0 && asset.size <= 1_073_741_824
            && asset.browser_download_url.starts_with(&format!("https://github.com/{}/releases/download/", entry.repo)))
        .context("В GitHub Releases нет готового ZIP-сценария. Исходники не устанавливаются вместо собранного пакета.")
}

pub fn download_and_install(
    item: &Scenery,
    target: &Path,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<Vec<String>> {
    ensure!(!cancel.load(Ordering::Relaxed), "Загрузка отменена");
    let asset = github_asset(item)?;
    let stage = make_stage(target)?;
    let archive = stage.path().join("scenery.zip");
    let mut response = client()?
        .get(&asset.browser_download_url)
        .send()?
        .error_for_status()
        .context("Не удалось скачать релиз автора")?;
    if let Some(length) = response.content_length() {
        ensure!(length <= 1_073_741_824, "Размер архива превышает 1 ГБ");
    }
    let mut output = fs::File::create(&archive)?;
    let mut downloaded = 0u64;
    let mut buffer = [0u8; 65536];
    loop {
        ensure!(!cancel.load(Ordering::Relaxed), "Загрузка отменена");
        let bytes = response.read(&mut buffer)?;
        if bytes == 0 {
            break;
        }
        downloaded += bytes as u64;
        ensure!(downloaded <= 1_073_741_824, "Архив превышает 1 ГБ");
        output.write_all(&buffer[..bytes])?;
        progress(downloaded);
    }
    output.flush()?;
    drop(output);
    ensure!(downloaded > 0, "Сервер вернул пустой архив");
    ensure!(!cancel.load(Ordering::Relaxed), "Загрузка отменена");
    install_from_archive_with_stage(&archive, target, stage.path())
}

pub fn install_local_zip(archive: &Path, target: &Path) -> Result<Vec<String>> {
    ensure!(archive.is_file(), "ZIP-файл не найден");
    ensure!(
        archive
            .extension()
            .and_then(|x| x.to_str())
            .is_some_and(|x| x.eq_ignore_ascii_case("zip")),
        "Поддерживается только ZIP"
    );
    ensure!(
        archive.metadata()?.len() <= 1_073_741_824,
        "Архив превышает 1 ГБ"
    );
    let stage = make_stage(target)?;
    install_from_archive_with_stage(archive, target, stage.path())
}

fn make_stage(target: &Path) -> Result<tempfile::TempDir> {
    ensure!(
        target.is_dir(),
        "Выберите существующую папку Community или другую папку назначения"
    );
    ensure!(
        !target.is_symlink(),
        "Папка назначения не должна быть символической ссылкой"
    );
    tempfile::Builder::new()
        .prefix(".realflow-scenery-")
        .tempdir_in(target)
        .context("Нет доступа на запись в папку назначения")
}

fn install_from_archive_with_stage(
    archive: &Path,
    target: &Path,
    stage: &Path,
) -> Result<Vec<String>> {
    let unpacked = stage.join("unpacked");
    fs::create_dir(&unpacked)?;
    extract_zip(archive, &unpacked)?;
    let packages = discover_packages(&unpacked)?;
    ensure!(
        !packages.is_empty(),
        "В ZIP нет папок MSFS с manifest.json и layout.json"
    );
    let names: Vec<String> = packages
        .iter()
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for name in &names {
        ensure!(
            !target.join(name).exists(),
            "Пакет {name} уже существует. Удалите/сохраните его копию вручную перед обновлением."
        );
    }
    let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
    for (source, name) in packages.iter().zip(&names) {
        let destination = target.join(name);
        if let Err(error) = fs::rename(source, &destination) {
            for (old, current) in moved.iter().rev() {
                let _ = fs::rename(current, old);
            }
            return Err(error).with_context(|| format!("Не удалось установить {name}"));
        }
        moved.push((source.clone(), destination));
    }
    Ok(names)
}

/// Detect complete Community package roots, never just arbitrary zip contents.
pub fn discover_packages(root: &Path) -> Result<Vec<PathBuf>> {
    let mut found = Vec::new();
    for entry in WalkDir::new(root).follow_links(false).max_depth(7) {
        let entry = entry?;
        if !entry.file_type().is_dir() {
            continue;
        }
        let dir = entry.path();
        if dir.join("manifest.json").is_file() && dir.join("layout.json").is_file() {
            let manifest: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.join("manifest.json"))?)
                    .context("Неверный manifest.json сценария")?;
            let layout: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.join("layout.json"))?)
                    .context("Неверный layout.json сценария")?;
            ensure!(
                manifest.is_object() && (layout.is_object() || layout.is_array()),
                "Повреждены метаданные сценария"
            );
            ensure!(dir.file_name().is_some(), "Безымянная папка");
            // Avoid copying an entire umbrella containing other Community packages.
            if !found.iter().any(|p: &PathBuf| dir.starts_with(p)) {
                found.push(dir.to_path_buf());
            }
        }
    }
    found.sort();
    let mut unique_names = std::collections::HashSet::new();
    for p in &found {
        let name = p
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_ascii_lowercase();
        ensure!(
            unique_names.insert(name),
            "Дублирующиеся имена пакетов внутри ZIP"
        );
        for node in WalkDir::new(p).follow_links(false).max_depth(32) {
            let node = node?;
            ensure!(
                !node.file_type().is_symlink(),
                "В архиве обнаружена символическая ссылка"
            );
        }
    }
    Ok(found)
}

#[cfg(windows)]
fn extract_zip(archive: &Path, destination: &Path) -> Result<()> {
    // Environment variables avoid shell interpolating paths supplied by the user.
    const SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem
$archive = $env:REALFLOW_SCENERY_ARCHIVE
$root = [IO.Path]::GetFullPath($env:REALFLOW_SCENERY_DEST)
$prefix = $root.TrimEnd('\') + '\'
$zip = [IO.Compression.ZipFile]::OpenRead($archive)
try {
  $bytes = [long]0
  $count = 0
  foreach ($entry in $zip.Entries) {
    $count++
    $bytes += $entry.Length
    if ($count -gt 20000 -or $bytes -gt 4294967296) { throw 'Archive limits exceeded' }
    $full = [IO.Path]::GetFullPath([IO.Path]::Combine($root, $entry.FullName.Replace('/', '\')))
    if (-not $full.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
      throw 'Unsafe archive path'
    }
  }
} finally { $zip.Dispose() }
[IO.Compression.ZipFile]::ExtractToDirectory($archive, $root)
"#;
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
        .env("REALFLOW_SCENERY_ARCHIVE", archive)
        .env("REALFLOW_SCENERY_DEST", destination)
        .status()
        .context("PowerShell не найден")?;
    ensure!(status.success(), "Не удалось безопасно распаковать ZIP");
    Ok(())
}

#[cfg(not(windows))]
fn extract_zip(_archive: &Path, _destination: &Path) -> Result<()> {
    bail!("Автоматическая установка сценариев поддерживается в Windows")
}
