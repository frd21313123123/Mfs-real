//! Download the official installer; FSLTL models remain distributed by FlyByWire.
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use sha2::{Digest, Sha512};
use std::{
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

pub const RELEASES_URL: &str = "https://github.com/flybywiresim/installer/releases/latest";
const METADATA_URL: &str =
    "https://github.com/flybywiresim/installer/releases/latest/download/latest.yml";
const MAX_SIZE: u64 = 512 * 1024 * 1024;

fn metadata(text: &str) -> Result<(String, Vec<u8>)> {
    let field = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(key))
            .map(str::trim)
    };
    let path = field("path:").context("В официальном релизе отсутствует Windows-установщик")?;
    ensure!(
        path.starts_with("FlyByWire-Installer-")
            && path.ends_with("-x64.exe")
            && path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b)),
        "Некорректное имя установщика"
    );
    let hash = STANDARD.decode(field("sha512:").context("Отсутствует контрольная сумма")?)?;
    ensure!(hash.len() == 64, "Некорректная контрольная сумма");
    Ok((path.to_owned(), hash))
}

fn copy_verified(
    mut input: impl Read,
    mut output: impl Write,
    expected: &[u8],
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    let mut hash = Sha512::new();
    let mut total = 0;
    let mut buffer = [0u8; 65536];
    loop {
        ensure!(!cancelled.load(Ordering::Relaxed), "Скачивание отменено");
        let n = input.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        total += n as u64;
        ensure!(total <= MAX_SIZE, "Установщик превышает допустимый размер");
        hash.update(&buffer[..n]);
        output.write_all(&buffer[..n])?;
        progress(total);
    }
    ensure!(total > 0, "Скачан пустой файл");
    ensure!(
        hash.finalize().as_slice() == expected,
        "Контрольная сумма не совпала; файл не сохранён"
    );
    Ok(())
}

pub fn download(
    destination: &Path,
    cancelled: &AtomicBool,
    mut progress: impl FnMut(u64),
) -> Result<()> {
    let client = reqwest::blocking::Client::builder()
        .https_only(true)
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(1800))
        .user_agent(concat!("RealFlow/", env!("CARGO_PKG_VERSION")))
        .build()?;
    let response = client.get(METADATA_URL).send()?.error_for_status()?;
    let mut text = String::new();
    response.take(65537).read_to_string(&mut text)?;
    ensure!(text.len() <= 65536, "Слишком большой файл описания релиза");
    let (name, hash) = metadata(&text)?;
    let response = client
        .get(format!(
            "https://github.com/flybywiresim/installer/releases/latest/download/{name}"
        ))
        .send()?
        .error_for_status()?;
    ensure!(
        response.content_length().is_none_or(|n| n <= MAX_SIZE),
        "Установщик превышает допустимый размер"
    );
    // Stage in the destination directory: failures leave any existing file intact.
    let parent = destination
        .parent()
        .context("Не выбрана папка сохранения")?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    let mut reported = 0;
    copy_verified(response, &mut staged, &hash, cancelled, |bytes| {
        if bytes >= reported + 1024 * 1024 {
            reported = bytes;
            progress(bytes);
        }
    })?;
    staged.as_file().sync_all()?;
    ensure!(!cancelled.load(Ordering::Relaxed), "Скачивание отменено");
    staged.persist(destination).map_err(|e| e.error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_metadata_and_rejects_paths() {
        let checksum = STANDARD.encode([0u8; 64]);
        let valid = format!("path: FlyByWire-Installer-3.7.2-x64.exe\nsha512: {checksum}\n");
        assert!(metadata(&valid).is_ok());
        for path in [
            "../evil.exe",
            "FlyByWire-Installer-../../evil-x64.exe",
            "https://evil/x64.exe",
        ] {
            assert!(metadata(&format!("path: {path}\nsha512: {checksum}\n")).is_err());
        }
        assert!(metadata("path: FlyByWire-Installer-3-x64.exe\nsha512: bad").is_err());
    }
    #[test]
    fn checks_corruption_and_cancellation() {
        let data = b"official installer bytes";
        let hash = Sha512::digest(data);
        let mut output = Vec::new();
        let cancel = AtomicBool::new(false);
        assert!(copy_verified(&data[..], &mut output, &hash, &cancel, |_| {}).is_ok());
        assert_eq!(output, data);
        assert!(copy_verified(&b"corrupted"[..], Vec::new(), &hash, &cancel, |_| {}).is_err());
        cancel.store(true, Ordering::Relaxed);
        assert!(copy_verified(&data[..], Vec::new(), &hash, &cancel, |_| {}).is_err());
    }
}
